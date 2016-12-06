use std;
use std::io;
use std::fs::File;
use std::io::{Write, BufReader, Seek, Read};
use std::ops::Range;
use std::str::FromStr;

use error::Error;
use repr::*;

// Thoughts
// Could be better with immutable self, where it returns the next position?
//  - calling function then may want to update self.pos.
// No I think it's best we let self.pos update automatically in functions...

// Thoughts on parsing
// Drop words.. start thinking about tokens.
// <</Name/lala>> has 4 tokens.


pub struct FileReader {
    buf: Vec<u8>,
    pos: usize,
    len: usize,
}


impl FileReader {
    pub fn new(path: &str) -> Result<FileReader, io::Error> {
        let mut file  = try!(File::open(path));
        let length = file.seek(io::SeekFrom::End(0))?;
        file.seek(io::SeekFrom::Start(0))?;
        let mut buf: Vec<u8> = Vec::new();
        buf.resize(length as usize, 0);
        file.read(&mut buf)?; // Read entire file into memory

        Ok(FileReader {
            buf: buf,
            pos: 0,
            len: length as usize,
        })
    }
    pub fn read(&mut self) -> Result<PDF, io::Error> {
        self.pos = self.len-1;
        self.find_backward(b"startxref").expect("Could not find startxref.");

        let xref_start = self.read_word_as::<usize>();
        self.read_xref(xref_start);

        self.pos = self.len-1;
        self.find_backward(b"trailer").expect("Could not find trailer.");
        // TODO It's possible to read a file without trailer. Just search for the Catalog!
        self.read_object();


        Ok(PDF::new())
    }


    /*** READ CROSS REFERENCE TABLE ***/
    /// Tries to read xref from the given position.
    fn read_xref(&mut self, start_pos: usize) -> Result<(), Error> {
        self.pos = start_pos;
        let r = self.read_word();
        if to_str(self.substr(r)) != "xref" {
            return Err(Error::from("Reading xref failed: first word is not xref"));
        }

        let first_obj = self.read_word_as::<usize>();
        let num_objs = self.read_word_as::<usize>();
        for i in first_obj..(first_obj + num_objs) {
            self.read_xref_entry(); // TODO ?-operator
        }
        Ok(())
    }
    fn read_xref_entry(&mut self) -> Result<XrefEntry, Error> {
        let arg1 = self.read_word_as::<usize>();
        let arg2 = self.read_word_as::<usize>();
        let arg3 = self.read_word_as::<String>();
        if arg3 == "f" {
            return Ok(XrefEntry::Free{obj_num: arg1, next_free: arg2});
        } else if arg3 == "n" {
            return Ok(XrefEntry::InUse{pos: arg1, gen_num: arg2});
        } else {
            return Err(Error::from("Error: read_xref_entry"));
        }
    }

    /*** READ OBJECTS ***/
    fn read_object(&mut self) -> Result<Object, Error> {
        self.ignore_whitespace();
        if self.buf[self.pos] == '<' as u8 && self.buf[self.pos + 1] == '<' as u8 {
            // Dictionary
            self.pos += 2;
            Ok(Object::Null)
        } else if self.buf[self.pos] == '<' as u8 {
            // Hex string
            Ok(Object::Null)
        } else if self.buf[self.pos] == '(' as u8 {
            Ok(Object::Null)
        } else {
            Ok(Object::Null)
        }
    }
    fn read_indirect_object(&mut self) -> Result<IndirectObject, Error> {
        let obj_nr = self.read_word_as::<i32>();
        let gen_nr = self.read_word_as::<i32>();
        let obj_word = self.read_word_as::<String>();
        let obj = self.read_object()?;
        if obj_word == "obj" {
            Ok(IndirectObject{
                obj_nr: obj_nr,
                gen_nr: gen_nr,
                object: obj,
            })
        } else {
            Err(Error::from("Error reading indirect object: 'obj' keyword not found"))
        }
    }
    fn read_name(&mut self) -> Result<Name, Error> {
        if self.buf[self.pos] == '/' as u8 {
            self.pos += 1;
            Err(Error::from("Not implemented"))
        } else {
            Err(Error::from("Error: expected name"))
        }
    }

    /*** OTHER ***/
    fn ignore_whitespace(&mut self) {
        while self.pos < self.len-1 && is_whitespace(self.buf[self.pos]) {
            self.pos += 1;
        }
    }
    /*** SEARCH / READ WORDS ***/
    /// Finds location of keyword by searching backward
    /// Sets the location to the first character of the next word if successful.
    fn find_backward(&mut self, keyword: &[u8]) -> Result<(), Error> {
        // TODO double check this alg
        let mut matched = keyword.len();
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
        Ok(())
    }
    /// Finds location of keyword by searching forward
    /// Sets the location to the first character of the next word if successful.
    fn find_forward(&mut self, keyword: &[u8]) -> Result<(), Error> {
        let mut matched = 0;
        loop {
            if self.buf[self.pos] == keyword[matched] {
                matched += 1;
            } else {
                matched = 0;
            }
            if matched == keyword.len() {
                break;
            }
            if self.pos == 0 {
                return Err(Error::from("Keyword not found"));
            }
            self.pos += 1;
        }
        self.read_word();
        Ok(())
    }

    /// Read until whitespace and return result. Leaves the position on the start of the next word.
    fn read_word(&mut self) -> Range<usize> {
        // Assumption: starts at beginning of current word.
        let mut result = Range{ start: self.pos, end: self.pos as usize};
        // Find range of word
        loop {
            self.pos += 1;
            if self.pos >= self.len - 1 {
                result.end = self.pos;
                return result;
            }
            if  is_whitespace(self.buf[self.pos]) {
                result.end = self.pos;
                break;
            }
        }
        // Move pos to start of next word
        loop {
            if !(self.buf[self.pos] == b' ' || self.buf[self.pos] == b'\r' || self.buf[self.pos] == b'\n' || self.buf[self.pos] == b'\t') {
                break;
            }
            self.pos += 1;
        }
        result
    }
    fn read_word_as<T: FromStr>(&mut self) -> T {
        let range = self.read_word();
        self.to::<T>(range)
    }
    fn substr(&self, range: Range<usize>) -> &[u8] {
        &self.buf[range]
    }
    fn to<T: FromStr>(&self, range: Range<usize>) -> T {
        std::str::from_utf8(&self.buf[range]).unwrap().parse::<T>().ok().unwrap()
    }
}

fn is_whitespace(character: u8) -> bool {
   character == b' ' ||
   character == b'\r' ||
   character == b'\n' ||
   character == b'\t'
}
fn to_str(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap()
}
