use std::iter::{Iterator, Peekable};
use std::str::CharIndices;
use std::num::ParseIntError;
use std::num::ParseFloatError;
use std::str::FromStr;

#[derive(Debug, PartialEq)]
enum Token<'a> {
    SimpleString(&'a str),
    Error(&'a str),
    Integer(i64),
    BulkString(Option<Vec<&'a str>>),
}

#[derive(Debug, PartialEq)]
enum Error {
    I64(ParseIntError),
    F64(ParseFloatError),
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

    fn next_if<F>(&mut self, condition: F) -> Option<(usize, char)> where F: FnOnce(&(usize, char)) -> bool {
        self.scanner
            .next_if(condition)
            .map(|c| {
                self.position += 1;
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
        let text = self.inner.get(start_position..=end_position)?;
        self.position = end_position + 1;
        Some(text)
    }

    fn scan_number(&mut self) -> (usize, usize) {
        let start_position = self.position;
        let mut end_position = self.position;
        while let Some((position, _)) = self.scanner.next_if(|(_, c)| c.is_ascii_digit()) {
            end_position = position;
        }
        (start_position, end_position)
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
        let symbol_position = {
            self.next_if(|(_, c)| *c == '+' || *c == '-')
                .unwrap_or((self.position, '+'))
                .0
        };
        let (_, end_position) = self.scan_number();
        let text = self.inner.get(symbol_position..=end_position)?;
        self.skip_line()?;
        Some(i64::from_str(text).map(Token::Integer).map_err(Error::I64))
    }

    fn scan_bulk_string(&mut self) -> Option<ParseResult<Token<'a>>> {
        self.next_if(|(_, c)| *c == '$')?;
        let symbol_position = {
            self.next_if(|(_, c)| *c == '+' || *c == '-')
                .unwrap_or((self.position, '+'))
                .0
        };
        let (_, end_position) = self.scan_number();
        self.skip_line()?;
        let count_result = {
            let text = self.inner.get(symbol_position..=end_position)?;
            i64::from_str(text)
        };

        match (count_result) {
            Ok(count) => {
                if count >= 0 {
                    let count = count as usize;
                    let mut list = Vec::with_capacity(count);
                    for _ in 0..count {
                        let text = self.scan_string(|(_, c)| *c != '\r' && *c != '\n')?;
                        self.skip_line()?;
                        list.push(text);
                    }

                    Some(Ok(Token::BulkString(Some(list))))
                } else {
                    Some(Ok(Token::BulkString(None)))
                }
            }
            Err(e) => Some(Err(Error::I64(e)))
        }
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = ParseResult<Token<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.scanner.peek()? {
            (_, '+') => self.scan_simple_string(),
            (_, '-') => self.scan_error(),
            (_, ':') => self.scan_integer(),
            (_, '$') => self.scan_bulk_string(),
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
    use super::{
        Lexer,
        Token,
    };

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
        assert_eq!(lexer.next().unwrap(), Ok(Token::Error("ERR unknown command 'FOO'")));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_error_2() {
        let mut lexer = Lexer::new("-ERR unknown command 'FOO'\r\n-10086\r\n");
        assert_eq!(lexer.next().unwrap(), Ok(Token::Error("ERR unknown command 'FOO'")));
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
}