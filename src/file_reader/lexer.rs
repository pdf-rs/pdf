use std;
use std::ops::Range;
use std::io::SeekFrom;
// Item = a range + a reference to the buffer.
//   - contains methods to convert to number or string.

// Thoughts (maybe TODO)
//  - If Lexer only contains `buf`, then we might as well just have one Lexer type which is an
//    Iterator and borrows rather than owns Vec<u8>

pub struct Lexer {
    buf: Vec<u8>,
}

impl<'a> Lexer {
    pub fn new(buf: Vec<u8>) -> Lexer {
        Lexer {
            buf: buf,
        }
    }
    pub fn iter(&self) -> LexerIter {
        LexerIter {
            pos: 0,
            lexer: self,
        }
    }
}

pub struct LexerIter<'a> {
    pos: usize,
    lexer: &'a Lexer,
}
impl<'a> LexerIter<'a> {
    pub fn new(lexer: &'a Lexer) -> LexerIter<'a> {
        LexerIter {
            pos: 0,
            lexer: lexer,
        }
    }
    pub fn new_substr(&self, range: Range<usize>) -> Substr<'a> {
        Substr {
            slice: &self.lexer.buf[range],
        }
    }

    pub fn seek(&mut self, new_pos: SeekFrom) {
        let wanted_pos;
        match new_pos {
            SeekFrom::Start(offset) => wanted_pos = offset as usize,
            SeekFrom::End(offset) => wanted_pos = self.lexer.buf.len() - offset as usize,
            SeekFrom::Current(offset) => wanted_pos = self.pos + offset as usize,
        }
        self.pos = wanted_pos; // TODO restrict
    }

    /// Moves pos to start of next line. Returns the skipped-over substring.
    pub fn seek_newline(&mut self) -> Substr{
        let start = self.pos;
        while self.lexer.buf[self.pos] != b'\n' 
            && self.incr_pos() { }
        self.incr_pos();

        Substr { slice: &self.lexer.buf[start..self.pos] }
    }

    /// Moves pos to after the found `substr`. Returns Substr with traversed text if `substr` is found.
    pub fn seek_substr(&mut self, substr: &[u8]) -> Option<Substr<'a>> {
        let start = self.pos;
        let mut matched = 0;
        loop {
            if self.lexer.buf[self.pos] == substr[matched] {
                matched += 1;
            } else {
                matched = 0;
            }
            if matched == substr.len() {
                break;
            }
            if self.pos >= self.lexer.buf.len() {
                return None
            }
            self.pos += 1;
        }
        self.pos += 1;
        Some(self.new_substr(start..(self.pos - substr.len())))
    }
    /* 
    pub fn seek_substr_back(&mut self, substr: &[u8]) -> Option<Substr<'a>> {
        let start = self.pos;
        let mut matched = substr.len();
        loop {
            if self.buf[self.pos] == keyword[matched - 1] {
                matched -= 1;
            } else {
                matched = keyword.len();
            }
            if matched == 0 {
                break;
            }
            if self.pos == 0 {
                return Err(Error::from("Keyword not found"));
            }
            self.pos -= 1;
        }
        self.read_word();
    }
    */

    fn incr_pos(&mut self) -> bool {
        if self.pos >= self.lexer.buf.len() - 1 {
            false
        } else {
            self.pos += 1;
            true
        }
    }
    fn is_whitespace(&self, pos: usize) -> bool {
        if pos >= self.lexer.buf.len() {
            false
        } else {
            self.lexer.buf[pos] == b' ' ||
            self.lexer.buf[pos] == b'\r' ||
            self.lexer.buf[pos] == b'\n' ||
            self.lexer.buf[pos] == b'\t'
        }
    }

    fn is_delimiter(&self, pos: usize) -> bool {
        if pos >= self.lexer.buf.len() {
            false
        } else {
            self.lexer.buf[pos] == b'(' ||
            self.lexer.buf[pos] == b')' ||
            self.lexer.buf[pos] == b'<' ||
            self.lexer.buf[pos] == b'>' ||
            self.lexer.buf[pos] == b'[' ||
            self.lexer.buf[pos] == b']' ||
            self.lexer.buf[pos] == b'{' ||
            self.lexer.buf[pos] == b'}' ||
            self.lexer.buf[pos] == b'/' ||
            self.lexer.buf[pos] == b'%'
        }
    }
}


impl<'a> Iterator for LexerIter<'a> {
    type Item = Substr<'a>;

    /// As a start, the only thing separating lexemes is whitespace.
    fn next(&mut self) -> Option<Substr<'a>> {
        // Move away from eventual whitespace
        while self.is_whitespace(self.pos) {
            if !self.incr_pos() {
                return None;
            }
        }
        let start_pos = self.pos;

        // If first character is delimiter, this lexeme only contains that character.
        //  - except << and >> which go together
        if self.is_delimiter(self.pos) {
            if self.lexer.buf[self.pos] == b'<' && self.lexer.buf[self.pos+1] == b'<'
                || self.lexer.buf[self.pos] == b'>' && self.lexer.buf[self.pos+1] == b'>' {
                self.incr_pos();
            }
            self.incr_pos();
            return Some(Substr { slice: &self.lexer.buf[start_pos..self.pos] } );
        }

        // Read to past the end of lexeme
        while !self.is_whitespace(self.pos) && !self.is_delimiter(self.pos) && self.incr_pos() {
        }
        Some(Substr { slice: &self.lexer.buf[start_pos..self.pos] } )
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
    pub fn equals(&self, other: &[u8]) -> bool {
        self.slice == other
    }
}






fn to_str(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap()
}
