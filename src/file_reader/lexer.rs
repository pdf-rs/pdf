use std;
use std::str::FromStr;
use std::ops::Range;
use std::io::SeekFrom;


#[derive(Copy, Clone)]
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

    pub fn get_pos(&self) -> usize {
        self.pos
    }

    pub fn new_substr(&self, range: Range<usize>) -> Substr<'a> {
        Substr {
            slice: &self.buf[range],
        }
    }

    pub fn seek(&mut self, new_pos: SeekFrom) {
        let wanted_pos;
        match new_pos {
            SeekFrom::Start(offset) => wanted_pos = offset as usize,
            SeekFrom::End(offset) => wanted_pos = self.buf.len() - offset as usize - 1,
            SeekFrom::Current(offset) => wanted_pos = self.pos + offset as usize,
        }
        self.pos = wanted_pos; // TODO restrict
    }

    /// Moves pos to start of next line. Returns the skipped-over substring.
    pub fn seek_newline(&mut self) -> Substr{
        let start = self.pos;
        while self.buf[self.pos] != b'\n' 
            && self.incr_pos() { }
        self.incr_pos();

        self.new_substr(start..self.pos)
    }

    /// Moves pos to after the found `substr`. Returns Substr with traversed text if `substr` is found.
    pub fn seek_substr(&mut self, substr: &[u8]) -> Option<Substr<'a>> {
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
    pub fn seek_substr_back(&mut self, substr: &[u8]) -> Option<Substr<'a>> {
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
                return None;
            }
            self.pos -= 1;
        }
        self.pos += substr.len();
        Some(self.new_substr(self.pos..start))
    }

    /// Read and return slice of at most n bytes.
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


impl<'a> Iterator for Lexer<'a> {
    type Item = Substr<'a>;

    /// Gives the next lexeme
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
            if self.buf[self.pos] == b'<' && self.buf[self.pos+1] == b'<'
                || self.buf[self.pos] == b'>' && self.buf[self.pos+1] == b'>' {
                self.incr_pos();
            }
            self.incr_pos();
            return Some(self.new_substr(start_pos..self.pos) );
        }

        // Read to past the end of lexeme
        while !self.is_whitespace(self.pos) && !self.is_delimiter(self.pos) && self.incr_pos() {
        }
        Some(self.new_substr(start_pos..self.pos) )
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
    pub fn to<T: FromStr>(&self) -> Option<T> {
        std::str::from_utf8(self.slice).unwrap().parse::<T>().ok()
    }
    pub fn is_integer(&self) -> bool {
        match self.to::<i32>() {
            Some(_) => true,
            None => false,
        }
    }

    pub fn equals(&self, other: &[u8]) -> bool {
        self.slice == other
    }
}






fn to_str(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap()
}
