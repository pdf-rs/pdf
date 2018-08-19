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
                    b'b' => unimplemented!(),
                    b'f' => unimplemented!(),
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

/* not done..


#[cfg(test)]
mod tests {
    use reader::lexer::StringLexer;
    #[test]
    fn tests() {
        let vec = b"a\\nb\\rc\\td\\(f/)\\\\hei)";
        let lexer = StringLexer::new(vec);
        let lexemes: Vec<Result<u8>> = lexer.iter().collect();
    }
}

*/
