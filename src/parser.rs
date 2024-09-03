use std::iter::{Iterator, Peekable};
use std::num::ParseFloatError;
use std::num::ParseIntError;
use std::str::CharIndices;
use std::str::FromStr;

#[derive(Debug, PartialEq)]
enum Token<'a> {
    SimpleString(&'a str),
    Error(&'a str),
    Integer(i64),
    BulkString(Option<&'a str>),
    Array(Option<Vec<Token<'a>>>),
    Boolean(bool),
    Set(Option<Vec<Token<'a>>>),
    Double(&'a str),
    BigNumber(&'a str),
    BigErr(&'a str),
    VerbatimString(&'a str, &'a str),
}

#[derive(Debug, PartialEq)]
enum Error {
    I64(ParseIntError),
    F64(ParseFloatError),
    Boolean,
}

impl From<ParseIntError> for Error {
    fn from(err: ParseIntError) -> Self {
        Error::I64(err)
    }
}

impl From<ParseFloatError> for Error {
    fn from(err: ParseFloatError) -> Self {
        Error::F64(err)
    }
}

type ParseResult<T> = std::result::Result<T, Error>;

#[derive(Debug)]
struct Lexer<'a> {
    inner: &'a str,
    scanner: Peekable<CharIndices<'a>>,
    position: usize,
}

impl<'a> Lexer<'a> {
    fn new(inner: &'a str) -> Self {
        Self {
            inner,
            scanner: inner.char_indices().peekable(),
            position: 0,
        }
    }

    fn skip_line(&mut self) -> Option<()> {
        if self.inner.get(self.position..=self.position + 1).is_some() {
            self.next_if(|(_, c)| *c == '\r');
            self.next_if(|(_, c)| *c == '\n');
            Some(())
        } else {
            None
        }
    }

    fn next_if<F>(&mut self, condition: F) -> Option<(usize, char)>
    where
        F: FnOnce(&(usize, char)) -> bool,
    {
        self.scanner.next_if(condition).map(|c| {
            self.position = c.0 + 1;
            c
        })
    }

    fn scan_string<F>(&mut self, condition: F) -> Option<&'a str>
    where
        F: FnOnce(&(usize, char)) -> bool + Copy,
    {
        let start_position = self.position;
        let mut end_position = self.position;
        while let Some((position, _)) = self.scanner.next_if(condition) {
            end_position = position;
        }
        let text = if start_position < end_position {
            self.position = end_position + 1;
            self.inner.get(start_position..=end_position)?
        } else {
            ""
        };
        Some(text)
    }

    fn get_symbol_position(&mut self) -> usize {
        self.next_if(|(_, c)| *c == '+' || *c == '-')
            .unwrap_or((self.position, '+'))
            .0
    }

    fn scan_number(&mut self) -> (usize, usize) {
        let start_position = self.position;
        let mut end_position = self.position;
        while let Some((position, _)) = self.scanner.next_if(|(_, c)| c.is_ascii_digit()) {
            end_position = position;
        }
        (start_position, end_position)
    }

    fn get_integer(&mut self) -> Option<ParseResult<i64>> {
        let symbol_position = self.get_symbol_position();
        let (_, end_position) = self.scan_number();
        let text = self.inner.get(symbol_position..=end_position)?;
        Some(i64::from_str(text).map_err(Error::I64))
    }

    fn get_collections<F>(
        &mut self,
        count_result: ParseResult<i64>,
        mut call_back: F,
    ) -> Option<ParseResult<i64>>
    where
        F: FnMut(Token<'a>),
    {
        match count_result {
            Err(e) => Some(Err(e)),
            Ok(count) => {
                if count >= 0 {
                    let tmp_count = count as usize;
                    for _ in 0..tmp_count {
                        match self.next()? {
                            Ok(token) => call_back(token),
                            Err(e) => return Some(Err(e)),
                        }
                    }
                    Some(Ok(count))
                } else {
                    self.skip_line()?;
                    Some(Ok(count))
                }
            }
        }
    }

    fn scan_simple_string(&mut self) -> Option<ParseResult<Token<'a>>> {
        self.next_if(|(_, c)| *c == '+')?;
        let text = self.scan_string(|(_, c)| *c != '\r' && *c != '\n')?;
        self.skip_line()?;
        Some(Ok(Token::SimpleString(text)))
    }

    fn scan_error(&mut self) -> Option<ParseResult<Token<'a>>> {
        self.next_if(|(_, c)| *c == '-')?;
        let text = self.scan_string(|(_, c)| *c != '\r' && *c != '\n')?;
        self.skip_line()?;
        Some(Ok(Token::Error(text)))
    }

    fn scan_integer(&mut self) -> Option<ParseResult<Token<'a>>> {
        self.next_if(|(_, c)| *c == ':')?;
        let result = self.get_integer()?;
        self.skip_line()?;
        Some(result.map(Token::Integer))
    }

    fn scan_bulk_string(&mut self) -> Option<ParseResult<Token<'a>>> {
        self.next_if(|(_, c)| *c == '$')?;
        let count_result = self.get_integer()?;
        self.skip_line()?;

        match count_result {
            Ok(count) => {
                if count >= 0 {
                    let count = count as usize;
                    let end_position = self.position + count;
                    let text = self.scan_string(|(position, c)| {
                        *position < end_position && *c != '\r' && *c != '\n'
                    })?;
                    self.skip_line()?;
                    Some(Ok(Token::BulkString(Some(text))))
                } else {
                    Some(Ok(Token::BulkString(None)))
                }
            }
            Err(e) => Some(Err(e)),
        }
    }

    fn scan_array(&mut self) -> Option<ParseResult<Token<'a>>> {
        self.next_if(|(_, c)| *c == '*')?;
        let count_result = self.get_integer()?;
        self.skip_line()?;

        let mut list = Vec::new();
        match self.get_collections(count_result, |token| list.push(token)) {
            None => None,
            Some(Ok(count)) if count >= 0 => Some(Ok(Token::Array(Some(list)))),
            Some(Ok(_)) => Some(Ok(Token::Array(None))),
            Some(Err(e)) => Some(Err(e)),
        }
    }

    fn scan_boolean(&mut self) -> Option<ParseResult<Token<'a>>> {
        self.next_if(|(_, c)| *c == '#')?;
        let token = {
            match self.next_if(|(_, c)| *c == 't' || *c == 'f')? {
                (_, 't') => Token::Boolean(true),
                (_, 'f') => Token::Boolean(false),
                _ => return Some(Err(Error::Boolean)),
            }
        };
        self.skip_line()?;
        Some(Ok(token))
    }

    fn scan_set(&mut self) -> Option<ParseResult<Token<'a>>> {
        self.next_if(|(_, c)| *c == '#')?;
        let count_result = self.get_integer()?;
        self.skip_line()?;

        let mut set = Vec::new();
        match self.get_collections(count_result, |token| {
            set.push(token);
        }) {
            None => None,
            Some(Ok(count)) if count >= 0 => Some(Ok(Token::Set(Some(set)))),
            Some(Ok(_)) => Some(Ok(Token::Set(None))),
            Some(Err(e)) => Some(Err(e)),
        }
    }

    fn scan_double(&mut self) -> Option<ParseResult<Token<'a>>> {
        self.next_if(|(_, c)| *c == ',')?;
        let start_position = self.get_symbol_position();
        let mut end_position = start_position;
        let (_, position) = self.scan_number();
        end_position = position;

        if self.next_if(|(_, c)| *c == '.').is_some() {
            let (_, position) = self.scan_number();
            end_position = position;
        }

        if self.next_if(|(_, c)| *c == 'e' || *c == 'E').is_some() {
            self.get_symbol_position();
            let (_, position) = self.scan_number();
            end_position = position;
        }
        let text = self.inner.get(start_position..=end_position)?;
        self.skip_line()?;
        Some(Ok(Token::Double(text)))
    }

    fn scan_big_number(&mut self) -> Option<ParseResult<Token<'a>>> {
        self.next_if(|(_, c)| *c == '(')?;
        let start_position = self.get_symbol_position();
        let (_, end_position) = self.scan_number();
        let text = self.inner.get(start_position..=end_position)?;
        self.skip_line()?;
        Some(Ok(Token::BigNumber(text)))
    }

    fn scan_big_error(&mut self) -> Option<ParseResult<Token<'a>>> {
        self.next_if(|(_, c)| *c == '!')?;
        let text = self.scan_string(|(_, c)| *c != '\r' && *c != '\n')?;
        self.skip_line()?;
        Some(Ok(Token::BigErr(text)))
    }

    fn scan_verbatim_string(&mut self) -> Option<ParseResult<Token<'a>>> {
        self.next_if(|(_, c)| *c == '=')?;
        let len = self.get_integer()?;
        self.skip_line()?;

        let len = len.ok()? as usize;

        let start_position = self.position;
        dbg!(&start_position);
        let formatter = self.scan_string(|(position, _)| *position < start_position + 3)?;
        self.next_if(|(_, c)| *c == ':')?;
        let text = self.scan_string(|(position, _)| *position < len + start_position)?;
        self.skip_line()?;

        Some(Ok(Token::VerbatimString(formatter, text)))
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = ParseResult<Token<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        // switch (parser->curr_location[0]) {
        //     case '$': return parseBulk(parser, p_ctx);
        //     case '+': return parseSimpleString(parser, p_ctx);
        //     case '-': return parseError(parser, p_ctx);
        //     case ':': return parseLong(parser, p_ctx);
        //     case '*': return parseArray(parser, p_ctx);
        //     case '~': return parseSet(parser, p_ctx);
        //     case '%': return parseMap(parser, p_ctx);
        //     case '#': return parseBool(parser, p_ctx);
        //     case ',': return parseDouble(parser, p_ctx);
        //     case '_': return parseNull(parser, p_ctx);
        //     case '(': return parseBigNumber(parser, p_ctx);
        //     case '=': return parseVerbatimString(parser, p_ctx);
        //     case '|': return parseAttributes(parser, p_ctx);
        //     default: if (parser->callbacks.error) parser->callbacks.error(p_ctx);
        // }
        match self.scanner.peek()? {
            (_, '+') => self.scan_simple_string(),
            (_, '-') => self.scan_error(),
            (_, ':') => self.scan_integer(),
            (_, '$') => self.scan_bulk_string(),
            (_, '*') => self.scan_array(),
            (_, '~') => self.scan_set(),
            (_, ',') => self.scan_double(),
            (_, '#') => self.scan_boolean(),
            (_, '(') => self.scan_big_number(),
            (_, '!') => self.scan_big_error(),
            (_, '=') => self.scan_verbatim_string(),
            _ => {
                todo!()
            }
        }
    }
}

// redis协议解析器
#[derive(Debug)]
pub struct Parser<'a> {
    buf: &'a str,
}

impl<'a> Parser<'a> {
    pub fn new(buf: &'a str) -> Self {
        Self { buf }
    }

    pub fn parse(&self) {
        let mut lexer = Lexer::new(self.buf);
    }
}

mod tests {
    use super::{Lexer, Token};

    #[test]
    fn test_simple_string() {
        let mut lexer = Lexer::new("+OK\r\n");
        assert_eq!(lexer.next().unwrap(), Ok(Token::SimpleString("OK")));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_simple_string_2() {
        let mut lexer = Lexer::new("+OK\r\n+OK\r\n");
        assert_eq!(lexer.next().unwrap(), Ok(Token::SimpleString("OK")));
        assert_eq!(lexer.next().unwrap(), Ok(Token::SimpleString("OK")));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_error() {
        let mut lexer = Lexer::new("-ERR unknown command 'FOO'\r\n");
        assert_eq!(
            lexer.next().unwrap(),
            Ok(Token::Error("ERR unknown command 'FOO'"))
        );
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_error_2() {
        let mut lexer = Lexer::new("-ERR unknown command 'FOO'\r\n-10086\r\n");
        assert_eq!(
            lexer.next().unwrap(),
            Ok(Token::Error("ERR unknown command 'FOO'"))
        );
        assert_eq!(lexer.next().unwrap(), Ok(Token::Error("10086")));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_number() {
        let mut lexer = Lexer::new(":1000\r\n");
        assert_eq!(lexer.next().unwrap(), Ok(Token::Integer(1000)));
        assert_eq!(lexer.next(), None);

        let mut lexer = Lexer::new(":+0\r\n");
        assert_eq!(lexer.next().unwrap(), Ok(Token::Integer(0)));

        let mut lexer = Lexer::new(":-0\r\n");
        assert_eq!(lexer.next().unwrap(), Ok(Token::Integer(-0)));
    }

    #[test]
    fn test_number_2() {
        let mut lexer = Lexer::new(":1000\r\n:-1000\r\n");
        assert_eq!(lexer.next().unwrap(), Ok(Token::Integer(1000)));
        assert_eq!(lexer.next().unwrap(), Ok(Token::Integer(-1000)));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_bulk_string() {
        let mut lexer = Lexer::new("$5\r\nhello\r\n");
        assert_eq!(lexer.next().unwrap(), Ok(Token::BulkString(Some("hello"))));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_bulk_string_2() {
        let mut lexer = Lexer::new("$0\r\n\r\n");
        assert_eq!(lexer.next().unwrap(), Ok(Token::BulkString(Some(""))));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_bulk_string_3() {
        let mut lexer = Lexer::new("$-1\r\n");
        assert_eq!(lexer.next().unwrap(), Ok(Token::BulkString(None)));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_array() {
        let mut lexer = Lexer::new("*0\r\n");
        assert_eq!(lexer.next().unwrap(), Ok(Token::Array(Some(vec![]))));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_array_2() {
        let mut lexer = Lexer::new("*2\r\n$3\r\nfoo\r\n$3\r\nbar\r\n");
        assert_eq!(
            lexer.next().unwrap(),
            Ok(Token::Array(Some(vec![
                Token::BulkString(Some("foo")),
                Token::BulkString(Some("bar")),
            ])))
        );
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_array_3() {
        let mut lexer = Lexer::new("*3\r\n:1\r\n:2\r\n:3\r\n");
        assert_eq!(
            lexer.next().unwrap(),
            Ok(Token::Array(Some(vec![
                Token::Integer(1),
                Token::Integer(2),
                Token::Integer(3),
            ])))
        );
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_array_4() {
        let mut lexer = Lexer::new("*5\r\n:1\r\n:2\r\n:3\r\n:4\r\n$6\r\nfoobar\r\n");
        assert_eq!(
            lexer.next().unwrap(),
            Ok(Token::Array(Some(vec![
                Token::Integer(1),
                Token::Integer(2),
                Token::Integer(3),
                Token::Integer(4),
                Token::BulkString(Some("foobar")),
            ])))
        );
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_array_5() {
        let mut lexer = Lexer::new("*-1\r\n\r\n");
        assert_eq!(lexer.next().unwrap(), Ok(Token::Array(None)));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_boolean() {
        let mut lexer = Lexer::new("#t\r\n#f\r\n#\r\n");
        assert_eq!(lexer.next().unwrap(), Ok(Token::Boolean(true)));
        assert_eq!(lexer.next().unwrap(), Ok(Token::Boolean(false)));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_double() {
        let mut lexer = Lexer::new(",3.14\r\n,-3.14\r\n,5.9e3\r\n,2\r\n");
        assert_eq!(lexer.next().unwrap(), Ok(Token::Double("3.14")));
        assert_eq!(lexer.next().unwrap(), Ok(Token::Double("-3.14")));
        assert_eq!(lexer.next().unwrap(), Ok(Token::Double("5.9e3")));
        assert_eq!(lexer.next().unwrap(), Ok(Token::Double("2")));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_big_number() {
        let mut lexer = Lexer::new("(123\r\n(-123\r\n(+123\r\n");
        assert_eq!(lexer.next().unwrap(), Ok(Token::BigNumber("123")));
        assert_eq!(lexer.next().unwrap(), Ok(Token::BigNumber("-123")));
        assert_eq!(lexer.next().unwrap(), Ok(Token::BigNumber("+123")));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_big_error() {
        let mut lexer = Lexer::new("!OK\r\n");
        assert_eq!(lexer.next().unwrap(), Ok(Token::BigErr("OK")));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_verbatim_string() {
        let mut lexer = Lexer::new("=15\r\ntxt:Some string\r\n");
        assert_eq!(
            lexer.next().unwrap(),
            Ok(Token::VerbatimString("txt", "Some string"))
        );
        assert_eq!(lexer.next(), None);
    }
}
