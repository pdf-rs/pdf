use xref::XRefTable;
use memmap::{Mmap, Protection};
use std::str;
use std::io::Read;
use types::StreamFilter;

pub struct File<B> {
    backend:    B,
    refs:       XRefTable
}

fn locate_xref_offset(data: &[u8]) -> usize {
    // locate the xref offset at the end of the file
    // `\nPOS\n%%EOF` where POS is the position encoded as base 10 integer.
    // u64::MAX has 20 digits + \n\n(2) + %%EOF(5) = 27 bytes max.
    let mut it = data.iter();
    let end = it.rposition(|&n| n == b'\n').unwrap();
    let start = it.rposition(|&n| n == b'\n').unwrap();
    assert_eq!(&data[end ..], b"%%EOF");
    str::from_utf8(&data[start + 1 .. end]).unwrap().parse().unwrap()
}

impl<B> File<B> {
    fn open(path: &str) -> File<Mmap> {
        let file_mmap = Mmap::open_path(path, Protection::Read).unwrap();
        
        let data = file_mmap.as_slice();
        let xref_offset = locate_xref_offset(data);
        println!("xref offset: {}", xref_offset);
        
        unimplemented!()
    }
}

#[test]
fn locate_offset() {
    use std::fs::File;
    let mut buf = Vec::new();
    let mut f = File::open("example.pdf").unwrap();
    f.read_to_end(&mut buf);
    locate_xref_offset(&buf);
}


#[derive(Object, PrimitiveConv)]
#[pdf(Type="XRef")]
struct XRefInfo {
    // Normal Stream fields
    #[pdf(key = "Filter")]
    filter: Vec<StreamFilter>,

    // XRefStream fields
    #[pdf(key = "Size")]
    size: i32,

    #[pdf(key = "Index")]
    index: Vec<(i32, i32)>,

    #[pdf(key = "Prev")]
    prev: i32,

    #[pdf(key = "W")]
    w: Vec<i32,>
}

struct XRefStream {
    pub data: Vec<u8>,
    pub info: XRefInfo,
}


#[derive(Object, PrimitiveConv)]
#[pdf(Type="ObjStm")]
struct ObjStmInfo {
    // Normal Stream fields
    #[pdf(key = "Filter")]
    filter: Vec<StreamFilter>,

    // ObjStmStream fields
    #[pdf(key = "N")]
    n: i32,

    #[pdf(key = "First")]
    first: i32,

    #[pdf(key = "Extends")]
    extends: i32,

}

struct ObjectStream {
    pub data: Vec<u8>,
    pub info: ObjStmInfo,
}
