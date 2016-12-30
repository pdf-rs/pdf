use std;
use std::str::FromStr;
use std::ops::Range;
use std::io::SeekFrom;

use error::{Result, Error};


#[derive(Copy, Clone)]
#[allow(dead_code)]
pub struct Lexer<'a> {
    pos: usize,
    buf: &'a Vec<u8>,
}

impl<'a> Lexer<'a> {
    pub fn new(buf: &'a Vec<u8>) -> Lexer<'a> {
        Lexer {
            pos: 0,
            buf: buf,
        }
    }

    /// Gives the next lexeme
    pub fn next(&mut self) -> Result<Substr<'a>> {
        // Move away from eventual whitespace
        while self.is_whitespace(self.pos) {
            if !self.incr_pos() {
                return Err(Error::EOF);
            }
        }
        let start_pos = self.pos;

        // If first character is delimiter, this lexeme only contains that character.
        //  - except << and >> which go together
        if self.is_delimiter(self.pos) {
            if self.buf[self.pos] == b'<' && self.buf[self.pos+1] == b'<'
                || self.buf[self.pos] == b'>' && self.buf[self.pos+1] == b'>' {
                self.incr_pos();
            }
            self.incr_pos();
            return Ok(self.new_substr(start_pos..self.pos) );
        }

        // Read to past the end of lexeme
        while !self.is_whitespace(self.pos) && !self.is_delimiter(self.pos) && self.incr_pos() {
        }
        Ok(self.new_substr(start_pos..self.pos) )
    }

    pub fn next_as<T: FromStr>(&mut self) -> Result<T> {
        self.next().and_then(|word| word.to::<T>())
    }

    pub fn get_pos(&self) -> usize {
        self.pos
    }

    pub fn new_substr(&self, range: Range<usize>) -> Substr<'a> {
        Substr {
            slice: &self.buf[range],
        }
    }

    /// Returns the substr between the old and new positions
    pub fn seek(&mut self, new_pos: SeekFrom) -> Substr<'a> {
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

    /// Moves pos to start of next line. Returns the skipped-over substring.
    #[allow(dead_code)]
    pub fn seek_newline(&mut self) -> Substr{
        let start = self.pos;
        while self.buf[self.pos] != b'\n' 
            && self.incr_pos() { }
        self.incr_pos();

        self.new_substr(start..self.pos)
    }

    /// Moves pos to after the found `substr`. Returns Substr with traversed text if `substr` is found.
    #[allow(dead_code)]
    pub fn seek_substr(&mut self, substr: &[u8]) -> Option<Substr<'a>> {
        info!("Seeksubstr");
        //
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

    /// Searches for string backward. Moves to after the found `substr`, returns the traversed
    /// Substr if found.
    pub fn seek_substr_back(&mut self, substr: &[u8]) -> Result<Substr<'a>> {
        let start = self.pos;
        let mut matched = substr.len();
        loop {
            if self.buf[self.pos] == substr[matched - 1] {
                matched -= 1;
            } else {
                matched = substr.len();
            }
            if matched == 0 {
                break;
            }
            if self.pos == 0 {
                return Err(Error::NotFound {word: String::from(std::str::from_utf8(substr).unwrap())});
            }
            self.pos -= 1;
        }
        self.pos += substr.len();
        Ok(self.new_substr(self.pos..start))
    }

    /// Read and return slice of at most n bytes.
    #[allow(dead_code)]
    pub fn read_n(&mut self, n: usize) -> Substr<'a> {
        let start_pos = self.pos;
        self.pos += n;
        if self.pos >= self.buf.len() {
            self.pos = self.buf.len() - 1;
        }
        self.new_substr(start_pos..self.pos)

    }

    fn incr_pos(&mut self) -> bool {
        if self.pos >= self.buf.len() - 1 {
            false
        } else {
            self.pos += 1;
            true
        }
    }
    fn is_whitespace(&self, pos: usize) -> bool {
        if pos >= self.buf.len() {
            false
        } else {
            self.buf[pos] == b' ' ||
            self.buf[pos] == b'\r' ||
            self.buf[pos] == b'\n' ||
            self.buf[pos] == b'\t'
        }
    }

    fn is_delimiter(&self, pos: usize) -> bool {
        if pos >= self.buf.len() {
            false
        } else {
            self.buf[pos] == b'(' ||
            self.buf[pos] == b')' ||
            self.buf[pos] == b'<' ||
            self.buf[pos] == b'>' ||
            self.buf[pos] == b'[' ||
            self.buf[pos] == b']' ||
            self.buf[pos] == b'{' ||
            self.buf[pos] == b'}' ||
            self.buf[pos] == b'/' ||
            self.buf[pos] == b'%'
        }
    }
}



// Iterator item
pub struct Substr<'a> {
    slice: &'a [u8],
}
impl<'a> Substr<'a> {
    pub fn as_str(&self) -> &str {
        unsafe {
            std::str::from_utf8_unchecked(self.slice)
        }
    }
    pub fn as_string(&self) -> String {
        String::from(self.as_str())
    }
    pub fn to<T: FromStr>(&self) -> Result<T> {
        std::str::from_utf8(self.slice).unwrap().parse::<T>()
            .map_err(|_| Error::ParseError{word: String::from(self.as_str())})
    }
    pub fn is_integer(&self) -> bool {
        match self.to::<i32>() {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    pub fn equals(&self, other: &[u8]) -> bool {
        self.slice == other
    }
}
