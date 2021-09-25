/// Lexing an input file, in the sense of breaking it up into substrings based on delimiters and
/// whitespace.

use std::str::FromStr;
use std::ops::{Range, Deref};
use std::io::SeekFrom;
use std::slice::SliceIndex;

use crate::error::*;

mod str;
pub use self::str::{StringLexer, HexStringLexer};


/// `Lexer` has functionality to jump around and traverse the PDF lexemes of a string in any direction.
#[derive(Copy, Clone)]
#[allow(dead_code)]
pub struct Lexer<'a> {
    pos: usize,
    buf: &'a [u8],
}

// find the position where condition(data[pos-1]) == false and condition(data[pos]) == true
#[inline]
fn boundary_rev(data: &[u8], pos: usize, condition: impl Fn(u8) -> bool) -> usize {
    match data[.. pos].iter().rposition(|&b| !condition(b)) {
        Some(start) => start + 1,
        None => 0
    }
}
#[test]
fn test_boundary_rev() {
    assert_eq!(boundary_rev(&*b" hello", 3, not(is_whitespace)), 1);
    assert_eq!(boundary_rev(&*b" hello", 3, is_whitespace), 3);
}

// find the position where condition(data[pos-1]) == true and condition(data[pos]) == false
#[inline]
fn boundary(data: &[u8], pos: usize, condition: impl Fn(u8) -> bool) -> usize {
    match data[pos ..].iter().position(|&b| !condition(b)) {
        Some(start) => pos + start,
        None => data.len()
    }
}
#[test]
fn test_boundary() {
    assert_eq!(boundary(&*b" hello ", 3, not(is_whitespace)), 6);
    assert_eq!(boundary(&*b" hello ", 3, is_whitespace), 3);
    assert_eq!(boundary(&*b"01234  7orld", 5, is_whitespace), 7);
    assert_eq!(boundary(&*b"01234  7orld", 7, is_whitespace), 7);
    assert_eq!(boundary(&*b"q\n", 1, is_whitespace), 2);
}

#[inline]
fn is_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\r' | b'\n' | b'\t')
}
#[inline]
fn not<T>(f: impl Fn(T) -> bool) -> impl Fn(T) -> bool {
    move |t| !f(t)
}
impl<'a> Lexer<'a> {
    pub fn new(buf: &'a [u8]) -> Lexer<'a> {
        Lexer {
            pos: 0,
            buf,
        }
    }

    /// Returns next lexeme. Lexer moves to the next byte after the lexeme. (needs to be tested)
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Substr<'a>> {
        let (lexeme, pos) = self.next_word()?;
        self.pos = pos;
        Ok(lexeme)
    }

    /// consume the whitespace sequence following the stream start
    pub fn next_stream(&mut self) -> Result<()> {
        let pos = self.skip_whitespace(self.pos)?;
        if !self.buf[pos ..].starts_with(b"stream") {
            // bail!("next token isn't 'stream'");
        }
        
        let b0 = self.buf[pos + 6];
        if b0 == b'\n' {
            self.pos = pos + 7;
        } else if b0 == b'\r' {
            let b1 = self.buf[pos + 7];
            if b1 != b'\n' {
                panic!("invalid whitespace following 'stream'");
                // bail!("invalid whitespace following 'stream'");
            }
            self.pos = pos + 8;
        } else {
            panic!("invalid whitespace");
        }
        Ok(())
    }
    /// Gives previous lexeme. Lexer moves to the first byte of this lexeme. (needs to be tested)
    pub fn back(&mut self) -> Result<Substr<'a>> {
        //println!("back: {:?}", String::from_utf8_lossy(&self.buf[self.pos.saturating_sub(20) .. self.pos]));
        
        // first reverse until we find non-whitespace
        let end_pos = boundary_rev(self.buf, self.pos, is_whitespace);
        let start_pos = boundary_rev(self.buf, end_pos, not(is_whitespace));
        self.pos = start_pos;
        
        Ok(self.new_substr(start_pos .. end_pos))
    }

    /// Look at the next lexeme. Will return empty substr if the next character is EOF.
    pub fn peek(&self) -> Result<Substr<'a>> {
        match self.next_word() {
            Ok((substr, _)) => Ok(substr),
            Err(PdfError::EOF) => Ok(self.new_substr(self.pos..self.pos)),
            Err(e) => Err(e),
        }

    }

    /// Returns `Ok` if the next lexeme matches `expected` - else `Err`.
    pub fn next_expect(&mut self, expected: &'static str) -> Result<()> {
        let word = self.next()?;
        if word.equals(expected.as_bytes()) {
            Ok(())
        } else {
            Err(PdfError::UnexpectedLexeme {
                pos: self.pos,
                lexeme: word.to_string(),
                expected
            })
        }
    }

    /// skip whitespaces and return the position of the first non-whitespace character
    #[inline]
    fn skip_whitespace(&self, pos: usize) -> Result<usize> {
        // Move away from eventual whitespace
        let pos = boundary(self.buf, pos, is_whitespace);
        if pos >= self.buf.len() {
            Err(PdfError::EOF)
        } else {
            Ok(pos)
        }
    }

    /// Used by next, peek and back - returns substring and new position
    /// If forward, places pointer at the next non-whitespace character.
    /// If backward, places pointer at the start of the current word.
    // TODO ^ backward case is actually not tested or.. thought about that well.
    fn next_word(&self) -> Result<(Substr<'a>, usize)> {
        if self.pos == self.buf.len() {
            return Err(PdfError::EOF);
        }
        let mut pos = self.skip_whitespace(self.pos)?;
        while self.buf.get(pos) == Some(&b'%') {
            pos += 1;
            if let Some(off) = self.buf[pos..].iter().position(|&b| b == b'\n') {
                pos += off+1;
            }
            
            // Move away from eventual whitespace
            pos = self.skip_whitespace(pos)?;
        }
        
        let start_pos = pos;

        // If first character is delimiter, this lexeme only contains that character.
        //  - except << and >> which go together, and / which marks the start of a
        // name token.
        if self.is_delimiter(pos) {
            if self.buf[pos] == b'/' {
                pos = self.advance_pos(pos)?;
                while !self.is_whitespace(pos) && !self.is_delimiter(pos) {
                    pos = self.advance_pos(pos)?;
                }
                return Ok((self.new_substr(start_pos..pos), pos));
            }

            if let Some(slice) = self.buf.get(pos..=pos+1) {
                if slice == b"<<" || slice == b">>" {
                    pos = self.advance_pos(pos)?;
                }
            }

            pos = self.advance_pos(pos)?;
            return Ok((self.new_substr(start_pos..pos), pos));
        }

        // Read to past the end of lexeme
        while !self.is_whitespace(pos) && !self.is_delimiter(pos) {
            pos = self.advance_pos(pos)?;
        }
        let result = self.new_substr(start_pos..pos);

        // Move away from whitespace again
        //pos = self.skip_whitespace(pos)?;
        Ok((result, pos))
    }

    /// Just a helper for next_word.
    #[inline]
    fn advance_pos(&self, pos: usize) -> Result<usize> {
        if pos < self.buf.len() {
            Ok(pos + 1)
        } else {
            Err(PdfError::EOF)
        }
    }

    #[inline]
    pub fn next_as<T>(&mut self) -> Result<T>
        where T: FromStr, T::Err: std::error::Error + Send + Sync + 'static
    {
        self.next().and_then(|word| word.to::<T>())
    }

    #[inline]
    pub fn get_pos(&self) -> usize {
        self.pos
    }

    #[inline]
    pub fn new_substr(&self, mut range: Range<usize>) -> Substr<'a> {
        // if the range is backward, fix it
        // start is inclusive, end is exclusive. keep that in mind
        if range.start > range.end {
            let new_end = range.start + 1;
            range.start = range.end + 1;
            range.end = new_end;
        }

        Substr {
            slice: &self.buf[range],
        }
    }


    /// Just a helper function for set_pos, set_pos_from_end and offset_pos.
    #[inline]
    fn seek(&mut self, new_pos: SeekFrom) -> Substr<'a> {
        let wanted_pos;
        match new_pos {
            SeekFrom::Start(offset) => wanted_pos = offset as usize,
            SeekFrom::End(offset) => wanted_pos = self.buf.len() - offset as usize - 1,
            SeekFrom::Current(offset) => wanted_pos = self.pos + offset as usize,
        }

        let range = if self.pos < wanted_pos {
            self.pos..wanted_pos
        } else {
            wanted_pos..self.pos
        };
        self.pos = wanted_pos; // TODO restrict
        self.new_substr(range)
    }

    /// Returns the substr between the old and new positions
    #[inline]
    pub fn set_pos(&mut self, new_pos: usize) -> Substr<'a> {
        self.seek(SeekFrom::Start(new_pos as u64))
    }
    /// Returns the substr between the old and new positions
    #[inline]
    pub fn set_pos_from_end(&mut self, new_pos: usize) -> Substr<'a> {
        self.seek(SeekFrom::End(new_pos as i64))
    }
    /// Returns the substr between the old and new positions
    #[inline]
    pub fn offset_pos(&mut self, offset: usize) -> Substr<'a> {
        self.seek(SeekFrom::Current(offset as i64))
    }

    /// Moves pos to start of next line. Returns the skipped-over substring.
    #[allow(dead_code)]
    pub fn seek_newline(&mut self) -> Substr{
        let start = self.pos;
        while self.buf[self.pos] != b'\n' 
            && self.incr_pos() { }
        self.incr_pos();

        self.new_substr(start..self.pos)
    }


    // TODO: seek_substr and seek_substr_back should use next() or back()?
    /// Moves pos to after the found `substr`. Returns Substr with traversed text if `substr` is found.
    #[allow(dead_code)]
    pub fn seek_substr(&mut self, substr: impl AsRef<[u8]>) -> Option<Substr<'a>> {
        //
        let substr = substr.as_ref();
        let start = self.pos;
        let mut matched = 0;
        loop {
            if self.buf[self.pos] == substr[matched] {
                matched += 1;
            } else {
                matched = 0;
            }
            if matched == substr.len() {
                break;
            }
            if self.pos >= self.buf.len() {
                return None
            }
            self.pos += 1;
        }
        self.pos += 1;
        Some(self.new_substr(start..(self.pos - substr.len())))
    }

    //TODO perhaps seek_substr_back should, like back(), move to the first letter of the substr.
    /// Searches for string backward. Moves to after the found `substr`, returns the traversed
    /// Substr if found.
    pub fn seek_substr_back(&mut self, substr: &[u8]) -> Result<Substr<'a>> {
        let end = self.pos;
        match self.buf[.. end].windows(substr.len()).rposition(|w| w == substr) {
            Some(start) => {
                self.pos = start + substr.len();
                Ok(self.new_substr(self.pos .. end))
            }
            None => Err(PdfError::NotFound {word: String::from(std::str::from_utf8(substr).unwrap())})
        }
    }

    /// Read and return slice of at most n bytes.
    #[allow(dead_code)]
    pub fn read_n(&mut self, n: usize) -> Substr<'a> {
        let start_pos = self.pos;
        self.pos += n;
        if self.pos >= self.buf.len() {
            self.pos = self.buf.len() - 1;
        }
        if start_pos < self.buf.len() {
            self.new_substr(start_pos..self.pos)
        } else {
            self.new_substr(0..0)
        }
    }

    /// Returns slice from current position to end.
    #[inline]
    pub fn get_remaining_slice(&self) -> &[u8] {
        &self.buf[self.pos..]
    }

    #[inline]
    fn incr_pos(&mut self) -> bool {
        if self.pos >= self.buf.len() - 1 {
            false
        } else {
            self.pos += 1;
            true
        }
    }
    #[inline]
    fn is_whitespace(&self, pos: usize) -> bool {
        self.buf.get(pos).map(|&b| is_whitespace(b)).unwrap_or(false)
    }

    #[inline]
    fn is_delimiter(&self, pos: usize) -> bool {
        self.buf.get(pos).map(|b| b"()<>[]{}/%".contains(&b)).unwrap_or(false)
    }
}



/// A slice from some original string - a lexeme.
pub struct Substr<'a> {
    slice: &'a [u8],
}
impl<'a> Substr<'a> {
    // to: &S -> U. Possibly expensive conversion.
    // as: &S -> &U. Cheap borrow conversion
    // into: S -> U. Cheap ownership transfer conversion.

    pub fn to_string(&self) -> String {
        String::from_utf8_lossy(self.as_slice()).into()
    }
    pub fn to_vec(&self) -> Vec<u8> {
        self.slice.to_vec()
    }
    pub fn to<T>(&self) -> Result<T>
        where T: FromStr, T::Err: std::error::Error + Send + Sync + 'static
    {
        std::str::from_utf8(self.slice)?.parse::<T>().map_err(|e| PdfError::Parse { source: e.into() })
    }
    pub fn is_integer(&self) -> bool {
        self.to::<i32>().is_ok()
    }
    pub fn is_real_number(&self) -> bool {
        self.to::<f32>().is_ok()   
    }

    pub fn as_slice(&self) -> &'a [u8] {
        self.slice
    }
    pub fn as_str(&self) -> Result<&str> {
        std::str::from_utf8(self.slice).map_err(|e| PdfError::Parse { source: e.into() })
    }

    pub fn equals(&self, other: impl AsRef<[u8]>) -> bool {
        self.slice == other.as_ref()
    }

    pub fn reslice<R: SliceIndex<[u8], Output = [u8]>>(&self, range: R) -> Substr<'a> {
        Substr { slice: &self.slice[range] }
    }
}

impl<'a> Deref for Substr<'a> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}
impl<'a> PartialEq<&[u8]> for Substr<'a> {
    fn eq(&self, rhs: &&[u8]) -> bool {
        self.equals(rhs)
    }
}

impl<'a> PartialEq<&str> for Substr<'a> {
    fn eq(&self, rhs: &&str) -> bool {
        self.equals(rhs.as_bytes())
    }
}
