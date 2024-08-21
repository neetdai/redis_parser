use std::iter::{Iterator, Peekable};
use std::str::CharIndices;

#[derive(Debug)]
enum Token<'a> {
    SimpleString(&'a str),
    Error(&'a str),
}

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

    fn skip_line(&mut self) {
        if self.scanner.next_if(|(_, c)| *c == '\r').is_some() {
            self.position += 1;
            if self.scanner.next_if(|(_, c)| *c == '\n').is_some() {
                self.position += 1;
            }
        }
    }

    fn scan_string<F>(&mut self, condition: F) -> &'a str
    where
        F: FnOnce(&(usize, char)) -> bool + Copy,
    {
        let start_position = self.position;
        let mut end_position = self.position;
        while let Some((position, _)) = self.scanner.next_if(condition) {
            end_position = position;
        }
        let (left_str, _) = self.inner.split_at(end_position);
        let (_, right_str) = left_str.split_at(start_position);
        right_str
    }

    fn scan_simple_string(&mut self) -> Option<Token<'a>> {
        let (postion, _) = self.scanner.next_if(|(_, c)| *c == '+')?;
        self.position = postion;
        let token = {
            let text = self.scan_string(|(_, c)| *c != '\r' && *c != '\n');
            Token::SimpleString(text)
        };
        self.skip_line();
        Some(token)
    }

    fn scan_error(&mut self) -> Option<Token<'a>> {
        let (postion, _) = self.scanner.next_if(|(_, c)| *c == '-')?;
        self.position = postion;
        let token = {
            let text = self.scan_string(|(_, c)| *c != '\r' && *c != '\n');
            Token::Error(text)
        };
        self.skip_line();
        Some(token)
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.scanner.peek() {
            Some((_, '+')) => self.scan_simple_string(),
            Some((_, '-')) => self.scan_error(),
            Some(_) => {
                todo!()
            }
            None => None,
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
