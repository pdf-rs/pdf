use std::iter::Iterator;
use err::*;
use num_traits::int::PrimInt;

/// A lexer for PDF strings.

#[derive(Clone)]
pub struct StringLexer<'a> {
    pos: usize,
    nested: i32, // How far in () we are nested
    buf: &'a [u8],
}

impl<'a> StringLexer<'a> {
    /// `buf` should start right after the `(` delimiter, but span all the way to EOF. StringLexer
    /// will determine the end of the string.
    pub fn new(buf: &'a [u8]) -> StringLexer<'a> {
        StringLexer {
            pos: 0,
            nested: 0,
            buf: buf,
        }
    }
    pub fn iter<'b>(&'b mut self) -> StringLexerIter<'a, 'b> {
        StringLexerIter {lexer: self}
    }
    /// Get offset/pos from start of string
    pub fn get_offset(&self) -> usize {
        self.pos + 1
    }

    pub fn next_lexeme(&mut self) -> Result<Option<u8>> {
        let c = self.next_byte()?;
        match c {
            b'\\' => {
                let c = self.next_byte()?;
                Ok(
                match c {
                    b'n' => Some(b'\n'),
                    b'r' => Some(b'\r'),
                    b't' => Some(b'\t'),
                    b'b' => bail!("\\b encountered - don't know what to do"),
                    b'f' => bail!("\\f encountered - don't know what to do"),
                    b'(' => Some(b'('),
                    b')' => Some(b')'),
                    b'\n' => self.next_lexeme()?, // ignore \\\n
                    b'\\' => Some(b'\\'),

                    _ => {
                        self.back()?;
                        // A character code must follow. 1-3 numbers.
                        let mut char_code: u8 = 0;
                        let mut octal_digits = Vec::new();
                        for _ in 0..3 {
                            let c = self.peek_byte()?;
                            if c >= b'0' && c <= b'9' {
                                self.next_byte()?;
                                octal_digits.push(c - b'0');
                            } else {
                                break;
                            }
                        }
                        if octal_digits.len() == 0 {
                            bail!("Wrong character following `\\`: {}", self.peek_byte()?);
                        }
                        // Convert string of octal digits to number
                        octal_digits.reverse(); // little-endian
                        for (i, digit) in octal_digits.iter().enumerate() {
                            char_code += digit * 8.pow(i as u32);
                        }

                        Some(char_code)
                    }
                }
                )
            },

            b'(' => {
                self.nested += 1;
                Ok(Some(b'('))
            },
            b')' => {
                self.nested -= 1;
                if self.nested < 0 {
                    Ok(None)
                } else {
                    Ok(Some(b')'))
                }
            },

            c => Ok(Some(c))

        }
    }

    fn next_byte(&mut self) -> Result<u8> {
        if self.pos < self.buf.len() - 1 {
            self.pos += 1;
            Ok(self.buf[self.pos])
        } else {
            bail!(ErrorKind::EOF);
        }
    }
    fn back(&mut self) -> Result<()> {
        if self.pos > 0 {
            self.pos -= 1;
            Ok(())
        } else {
            bail!(ErrorKind::EOF);
        }
    }
    fn peek_byte(&mut self) -> Result<u8> {
        if self.pos < self.buf.len() - 1 {
            Ok(self.buf[self.pos + 1])
        } else {
            bail!(ErrorKind::EOF);
        }
    }
}

// "'a is valid for at least 'b"
pub struct StringLexerIter<'a: 'b, 'b> {
    lexer: &'b mut StringLexer<'a>,
}

impl<'a, 'b> Iterator for StringLexerIter<'a, 'b> {
    type Item = Result<u8>;
    fn next(&mut self) -> Option<Result<u8>> {
        match self.lexer.next_lexeme() {
            Err(e) => Some(Err(e)),
            Ok(Some(s)) => Some(Ok(s)),
            Ok(None) => None,
        }
    }
}
