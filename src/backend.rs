use memmap::{Mmap, Protection};
use err::*;
use std::fs::File;
use std::io::Read;

use std::ops::{
    RangeFull,
    RangeFrom,
    RangeTo,
    Range,
};


pub trait Backend: Sized {
    fn open(path: &str) -> Result<Self>;
    fn read<T: IndexRange>(&self, range: T) -> Result<&[u8]>;
    fn write<T: IndexRange>(&mut self, range: T) -> Result<&mut [u8]>;
    fn len(&self) -> usize;
}


impl Backend for Mmap {
    fn open(path: &str) -> Result<Mmap> {
        Ok(Mmap::open_path(path, Protection::Read)?)
    }
    fn read<T: IndexRange>(&self, range: T) -> Result<&[u8]> {
        let r = range.to_range(self.len());
        Ok(unsafe {
            &self.as_slice()[r]
        })
    }
    fn write<T: IndexRange>(&mut self, range: T) -> Result<&mut [u8]> {
        let r = range.to_range(self.len());
        Ok(unsafe {
            &mut self.as_mut_slice()[r]
        })
    }
    fn len(&self) -> usize {
        self.len()
    }
}


impl Backend for Vec<u8> {
    fn open(path: &str) -> Result<Self> {
        let mut buf = Vec::new();
        let mut f = File::open(path)?;
        f.read_to_end(&mut buf)?;
        Ok(buf)
    }
    fn read<T: IndexRange>(&self, range: T) -> Result<&[u8]> {
        let r = range.to_range(self.len());
        Ok(&self[r])
    }
    fn write<T: IndexRange>(&mut self, range: T) -> Result<&mut [u8]> {
        let r = range.to_range(self.len());
        Ok(&mut self[r])
    }
    fn len(&self) -> usize {
        self.len()
    }
}



/// `IndexRange` is implemented by Rust's built-in range types, produced
/// by range syntax like `..`, `a..`, `..b` or `c..d`.
pub trait IndexRange
{
    #[inline]
    /// Start index (inclusive)
    fn start(&self) -> Option<usize> { None }
    #[inline]
    /// End index (exclusive)
    fn end(&self) -> Option<usize> { None }

    /// `len`: the size of whatever container that is being indexed
    fn to_range(&self, len: usize) -> Range<usize> {
        self.start().unwrap_or(0) .. self.end().unwrap_or(len)
    }
}


impl IndexRange for RangeFull {}

impl IndexRange for RangeFrom<usize> {
    #[inline]
    fn start(&self) -> Option<usize> { Some(self.start) }
}

impl IndexRange for RangeTo<usize> {
    #[inline]
    fn end(&self) -> Option<usize> { Some(self.end) }
}

impl IndexRange for Range<usize> {
    #[inline]
    fn start(&self) -> Option<usize> { Some(self.start) }
    #[inline]
    fn end(&self) -> Option<usize> { Some(self.end) }
}
