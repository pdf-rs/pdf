use std::fs::File;
use std::io;
use std::io::{Write, BufReader, Seek, Read};

#[cfg(test)]
mod tests {
    use PDF;
    #[test]
    fn it_works() {
        let pdf = PDF::read_from_file("example.pdf");
        match pdf {
            Ok(_) => println!("Ok"),
            Err(_) => panic!("Some error occured in reading the file.")
        }
    }
}

/// Runtime representation of a PDF file.
struct PDF {
}
impl PDF {
    pub fn new() -> PDF {
        PDF {
        }
    }
    pub fn read_from_file(path: &str) -> Result<PDF,io::Error> {
        let mut reader = FileReader::new(path)?;
        let pdf = reader.read()?;
        Ok(pdf)
    }
}


///
struct FileReader {
    buf: BufReader<File>
}
impl FileReader {
    pub fn new(path: &str) -> Result<FileReader, io::Error> {
        let mut file  = try!(File::open(path));
        Ok(FileReader {
            buf: BufReader::new(file)
        })
    }
    pub fn read(&mut self) -> Result<PDF, io::Error> {
        println!("Whitespace chars: {} {} {}", b'\r', b'\n', b'\t'); 
        self.buf.seek(io::SeekFrom::End(0));
        self.find_backward(b"startxref").expect("Could not find startxref");
        let w: Vec<u8> = self.read_word().unwrap();
        println!("Word: {}", String::from_utf8(w).unwrap());

        Ok(PDF::new())
    }

    /// Finds location of keyword by searching backward
    /// Sets the location to the first character of this word
    fn find_backward(&mut self, keyword: &[u8]) -> Result<(), io::Error> {
        let mut c = [0; 1];

        let mut matched = keyword.len();
        loop {
            self.buf.seek(io::SeekFrom::Current(-2))?;  // two steps backward
            self.buf.read(&mut c)?;                     // one step ahead
            if c[0] == keyword[matched - 1] {
                matched -= 1;
            } else {
                matched = keyword.len();
            }
            if matched == 0 {
                break;
            }
        }
            self.buf.seek(io::SeekFrom::Current(-1))?;  // back to first character
        Ok(())
    }

    /// Read until whitespace and return result. Leaves the position on the next non-whitespace
    /// character.
    fn read_word(&mut self) -> Result<Vec<u8>, io::Error> {
        // Assumption: starts at beginning of current word.
        let mut result = Vec::new();
        let mut c = [0 as u8; 1];
        loop {
            self.buf.read(&mut c)?;
            result.push(c[0]);
            if c[0] == b' ' || c[0] == b'\r' || c[0] == b'\n' || c[0] == b'\t' {
                break;
            }
        }
        Ok(result)
    }
    fn get_file_pos(&mut self) -> u64 {
        let n = self.buf.seek(io::SeekFrom::Current(0));
        match n {
            Ok(pos) => pos,
            Err(_) => panic!("get_file_pos: something went wrong.")
        }
    }
    // fn next_word() -
}
