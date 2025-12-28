/// Lexing an input file, in the sense of breaking it up into substrings based on delimiters and
/// whitespace.

use std::str::FromStr;
use std::ops::{Range, Deref, RangeFrom};
use std::borrow::Cow;

use crate::error::*;
use crate::primitive::Name;

mod str;
pub use self::str::{StringLexer, HexStringLexer};


/// `Lexer` has functionality to jump around and traverse the PDF lexemes of a string in any direction.
#[derive(Copy, Clone)]
#[allow(dead_code)]
pub struct Lexer<'a> {
    pos: usize,
    buf: &'a [u8],
    file_offset: usize,
}

// find the position where condition(data[pos-1]) == false and condition(data[pos]) == true
#[inline]
fn boundary_rev(data: &[u8], pos: usize, condition: impl Fn(u8) -> bool) -> usize {
    match data[.. pos].iter().rposition(|&b| !condition(b)) {
        Some(start) => start + 1,
        None => 0
    }
}

// find the position where condition(data[pos-1]) == true and condition(data[pos]) == false
#[inline]
fn boundary(data: &[u8], pos: usize, condition: impl Fn(u8) -> bool) -> usize {
    match data[pos ..].iter().position(|&b| !condition(b)) {
        Some(start) => pos + start,
        None => data.len()
    }
}

#[inline]
fn is_whitespace(b: u8) -> bool {
    matches!(b, 0 | b' ' | b'\r' | b'\n' | b'\t')
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
            file_offset: 0
        }
    }
    pub fn with_offset(buf: &'a [u8], file_offset: usize) -> Lexer<'a> {
        Lexer {
            pos: 0,
            buf,
            file_offset
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
        
        let &b0 = self.buf.get(pos + 6).ok_or(PdfError::EOF)?;
        if b0 == b'\n' {
            self.pos = pos + 7;
        } else if b0 == b'\r' {
            let &b1 = self.buf.get(pos + 7).ok_or(PdfError::EOF)?;
            if b1 != b'\n' {
                bail!("invalid whitespace following 'stream'");
                // bail!("invalid whitespace following 'stream'");
            }
            self.pos = pos + 8;
        } else {
            bail!("invalid whitespace");
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
                    match self.advance_pos(pos) {
                        Ok(p) => pos = p,
                        Err(_) => break,
                    }
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
            match self.advance_pos(pos) {
                Ok(p) => pos = p,
                Err(_) => break,
            }
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
            file_offset: self.file_offset + range.start,
            slice: &self.buf[range],
        }
    }

    /// Just a helper function for set_pos, set_pos_from_end and offset_pos.
    #[inline]
    pub fn set_pos(&mut self, wanted_pos: usize) -> Substr<'a> {
        let new_pos = wanted_pos.min(self.buf.len());
        let range = if self.pos < new_pos {
            self.pos..new_pos
        } else {
            new_pos..self.pos
        };
        self.pos = new_pos;
        self.new_substr(range)
    }

    /// Returns the substr between the old and new positions
    #[inline]
    pub fn set_pos_from_end(&mut self, new_pos: usize) -> Substr<'a> {
        self.set_pos(self.buf.len().saturating_sub(new_pos).saturating_sub(1))
    }
    /// Returns the substr between the old and new positions
    #[inline]
    pub fn offset_pos(&mut self, offset: usize) -> Substr<'a> {
        self.set_pos(self.pos.wrapping_add(offset))
    }

    /// Moves pos to start of next line. Returns the skipped-over substring.
    #[allow(dead_code)]
    pub fn seek_newline(&mut self) -> Substr<'_> {
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
            if self.pos >= self.buf.len() {
                return None
            }
            if self.buf[self.pos] == substr[matched] {
                matched += 1;
            } else {
                matched = 0;
            }
            if matched == substr.len() {
                break;
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
            None => Err(PdfError::NotFound {word: String::from_utf8_lossy(substr).into() })
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
    pub fn get_remaining_slice(&self) -> &'a [u8] {
        &self.buf[self.pos..]
    }

    /// for debugging
    pub fn ctx(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.buf[self.pos.saturating_sub(40)..self.buf.len().min(self.pos+40)])
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
        self.buf.get(pos).map(|b| b"()<>[]{}/%".contains(b)).unwrap_or(false)
    }

}



/// A slice from some original string - a lexeme.
#[derive(Copy, Clone, Debug)]
pub struct Substr<'a> {
    slice: &'a [u8],
    file_offset: usize,
}
impl<'a> Substr<'a> {
    pub fn new<T: AsRef<[u8]> + ?Sized>(data: &'a T, file_offset: usize) -> Self {
        Substr { slice: data.as_ref(), file_offset }
    }
    // to: &S -> U. Possibly expensive conversion.
    // as: &S -> &U. Cheap borrow conversion
    // into: S -> U. Cheap ownership transfer conversion.

    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        String::from_utf8_lossy(self.as_slice()).into()
    }
    pub fn to_name(&self) -> Result<Name> {
        Ok(Name(std::str::from_utf8(self.as_slice())?.into()))
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
        if self.slice.len() == 0 {
            return false;
        }
        let mut slice = self.slice;
        if slice[0] == b'-' {
            if slice.len() < 2 {
                return false;
            }
            slice = &slice[1..];
        }
        is_int(slice)
    }
    pub fn is_real_number(&self) -> bool {
        self.real_number().is_some()
    }
    pub fn real_number(&self) -> Option<Self> {
        if self.slice.len() == 0 {
            return None;
        }
        let mut slice = self.slice;
        if slice[0] == b'-' {
            if slice.len() < 2 {
                return None;
            }
            slice = &slice[1..];
        }
        if let Some(i) = slice.iter().position(|&b| b == b'.') {
            if !is_int(&slice[..i]) {
                return None;
            }
            slice = &slice[i+1..];
        }
        if let Some(len) = slice.iter().position(|&b| !b.is_ascii_digit()) {
            if len == 0 {
                return None;
            }
            let end = self.slice.len() - slice.len() + len;
            Some(Substr {
                file_offset: self.file_offset,
                slice: &self.slice[..end]
            })
        } else {
            Some(*self)
        }
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

    pub fn reslice(&self, range: RangeFrom<usize>) -> Substr<'a> {
        Substr {
            file_offset: self.file_offset + range.start,
            slice: &self.slice[range],
        }
    }

    pub fn file_range(&self) -> Range<usize> {
        self.file_offset .. self.file_offset + self.slice.len()
    }
}

#[inline]
fn is_int(b: &[u8]) -> bool {
    b.iter().all(|&b| b.is_ascii_digit())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boundary_rev() {
        assert_eq!(boundary_rev(b" hello", 3, not(is_whitespace)), 1);
        assert_eq!(boundary_rev(b" hello", 3, is_whitespace), 3);
    }

    #[test]
    fn test_boundary() {
        assert_eq!(boundary(b" hello ", 3, not(is_whitespace)), 6);
        assert_eq!(boundary(b" hello ", 3, is_whitespace), 3);
        assert_eq!(boundary(b"01234  7orld", 5, is_whitespace), 7);
        assert_eq!(boundary(b"01234  7orld", 7, is_whitespace), 7);
        assert_eq!(boundary(b"q\n", 1, is_whitespace), 2);
    }

    #[test]
    fn test_substr() {
        assert!(Substr::new("123", 0).is_real_number());
        assert!(Substr::new("123.", 0).is_real_number());
        assert!(Substr::new("123.45", 0).is_real_number());
        assert!(Substr::new(".45", 0).is_real_number());
        assert!(Substr::new("-.45", 0).is_real_number());
        assert!(!Substr::new("123.45", 0).is_integer());
        assert!(Substr::new("123", 0).is_integer());
    }
}
