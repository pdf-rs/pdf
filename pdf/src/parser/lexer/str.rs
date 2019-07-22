use std::iter::Iterator;
use err::*;
use num_traits::int::PrimInt;

/// A lexer for PDF strings. Breaks the string up into single characters (`u8`)
/// It's also possible to get the number of indices of the original array that was traversed by the
/// Iterator.
///
/// ```
/// let mut string: Vec<u8> = Vec::new();
/// let bytes_traversed = {
///     let mut string_lexer = StringLexer::new(lexer.get_remaining_slice());
///     for character in string_lexer.iter() {
///         let character = character?;
///         string.push(character);
///     }
///     string_lexer.get_offset() as i64
/// };
/// // bytes_traversed now holds the number of bytes in the original array traversed.
/// ```
///

#[derive(Clone)]
pub struct StringLexer<'a> {
    pos: usize, // points to next byte
    nested: i32, // How far in () we are nested
    buf: &'a [u8],
}

impl<'a> StringLexer<'a> {
    /// `buf` should start right after the `(` delimiter, and may span all the way to EOF. StringLexer
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
        self.pos
    }

    /// (mostly just used by Iterator, but might be useful)
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
                    b'b' => Some(b'\x08'),
                    b'f' => Some(b'\x0c'),
                    b'(' => Some(b'('),
                    b')' => Some(b')'),
                    b'\n' => self.next_lexeme()?, // ignore \\\n
                    b'\\' => Some(b'\\'),

                    _ => {
                        self.back()?;
                        let start = self.get_offset();
                        let mut char_code: u8 = 0;
                        
                        // A character code must follow. 1-3 numbers.
                        for _ in 0..3 {
                            let c = self.peek_byte()?;
                            if c >= b'0' && c <= b'9' {
                                self.next_byte()?;
                                char_code = char_code * 8 + (c - b'0');
                            } else {
                                break;
                            }
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
        if self.pos < self.buf.len() {
            self.pos += 1;
            Ok(self.buf[self.pos-1])
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
        if self.pos < self.buf.len() {
            Ok(self.buf[self.pos])
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

pub struct HexStringLexer<'a> {
    pos: usize, // points to next byte
    buf: &'a [u8],
}

impl<'a> HexStringLexer<'a> {
    /// `buf` should start right after the `<` delimiter, and may span all the way to EOF.
    /// HexStringLexer will determine the end of the string.
    pub fn new(buf: &'a [u8]) -> HexStringLexer<'a> {
        HexStringLexer { pos: 0, buf }
    }

    pub fn iter<'b>(&'b mut self) -> HexStringLexerIter<'a, 'b> {
        HexStringLexerIter { lexer: self }
    }

    /// Get offset/position from start of string
    pub fn get_offset(&self) -> usize {
        self.pos
    }

    fn next_non_whitespace_char(&mut self) -> Result<u8> {
        let mut byte = self.read_byte()?;
        while byte == b' ' || byte == b'\t' || byte == b'\n' || byte == b'\r' || byte == b'\x0c' {
            byte = self.read_byte()?;
        }
        Ok(byte)
    }

    pub fn next_hex_byte(&mut self) -> Result<Option<u8>> {
        let c1 = self.next_non_whitespace_char()?;
        let high_nibble: u8 = match c1 {
            b'0'...b'9' => c1 - b'0',
            b'A'...b'F' => c1 - b'A' + 0xA,
            b'a'...b'f' => c1 - b'a' + 0xA,
            b'>' => return Ok(None),
            _ => bail!(ErrorKind::HexDecode {
                pos: self.pos,
                bytes: [c1, self.peek_byte().unwrap_or(0)]
            }),
        };
        let c2 = self.next_non_whitespace_char()?;
        let low_nibble: u8 = match c2 {
            b'0'...b'9' => c2 - b'0',
            b'A'...b'F' => c2 - b'A' + 0xA,
            b'a'...b'f' => c2 - b'a' + 0xA,
            b'>' => {
                self.back()?;
                0
            }
            _ => bail!(ErrorKind::HexDecode {
                pos: self.pos,
                bytes: [c1, c2]
            }),
        };
        Ok(Some((high_nibble << 4) | low_nibble))
    }

    fn read_byte(&mut self) -> Result<u8> {
        if self.pos < self.buf.len() {
            self.pos += 1;
            Ok(self.buf[self.pos - 1])
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
        if self.pos < self.buf.len() {
            Ok(self.buf[self.pos])
        } else {
            bail!(ErrorKind::EOF)
        }
    }
}

pub struct HexStringLexerIter<'a: 'b, 'b> {
    lexer: &'b mut HexStringLexer<'a>,
}

impl<'a, 'b> Iterator for HexStringLexerIter<'a, 'b> {
    type Item = Result<u8>;

    fn next(&mut self) -> Option<Result<u8>> {
        match self.lexer.next_hex_byte() {
            Err(e) => Some(Err(e)),
            Ok(Some(s)) => Some(Ok(s)),
            Ok(None) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Result;
    use parser::lexer::{HexStringLexer, StringLexer};

    #[test]
    fn tests() {
        let vec = b"a\\nb\\rc\\td\\(f/)\\\\hei)";
        let mut lexer = StringLexer::new(vec);
        let lexemes: Vec<u8> = lexer.iter().map(Result::unwrap).collect();
        assert_eq!(
            lexemes,
            vec![
                b'a',
                b'\n',
                b'b',
                b'\r',
                b'c',
                b'\t',
                b'd',
                b'(',
                b'f',
                b'/',
            ]
        );
    }

    #[test]
    fn hex_test() {
        let input = b"901FA3>";
        let mut lexer = HexStringLexer::new(input);
        let result: Vec<u8> = lexer.iter().map(Result::unwrap).collect();
        assert_eq!(
            result,
            vec![
                b'\x90',
                b'\x1f',
                b'\xa3',
            ]
        );

        let input = b"901FA>";
        let mut lexer = HexStringLexer::new(input);
        let result: Vec<u8> = lexer.iter().map(Result::unwrap).collect();
        assert_eq!(
            result,
            vec![
                b'\x90',
                b'\x1f',
                b'\xa0',
            ]
        );

        let input = b"1 9F\t5\r\n4\x0c62a>";
        let mut lexer = HexStringLexer::new(input);
        let result: Vec<u8> = lexer.iter().map(Result::unwrap).collect();
        assert_eq!(
            result,
            vec![
                b'\x19',
                b'\xf5',
                b'\x46',
                b'\x2a',
            ]
        );
    }
}
