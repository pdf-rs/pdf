extern crate pdf;

use std::rc::Rc;
use std::env::args;
use std::fmt;
use pdf::file::File;
use pdf::object::{Resolve, OutlineItem};
use pdf::primitive::{PdfString, Primitive};

struct Indent(usize);
impl fmt::Display for Indent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for _ in 0 .. self.0 {
            write!(f, "    ")?;
        }
        Ok(())
    } 
}

fn walk_outline(r: &impl Resolve, mut node: Rc<OutlineItem>, depth: usize) {
    let indent = Indent(depth);
    loop {
        if let Some(ref title) = node.title {
            println!("{}title: {:?}", indent, title.as_str().unwrap());
        }
        if let Some(ref dest) = node.dest {
            println!("{}dest: {:?}", indent, dest);
        }
        if let Some(entry_ref) = node.first {
            let entry = r.get(entry_ref).unwrap();
            walk_outline(r, entry, depth + 1);
        }
        if let Some(entry_ref) = node.next {
            node = r.get(entry_ref).unwrap();
            continue;
        }

        break;
    }
}

fn main() {
    let path = args().nth(1).expect("no file given");
    println!("read: {}", path);
    
    let file = File::<Vec<u8>>::open(&path).unwrap();
    let catalog = file.get_root();
    if let Some(ref names) = catalog.names {
        let mut count = 0;
        let mut cb = |key: &PdfString, val: &Primitive| {
            println!("{:?} {:?}", key, val);
            count += 1;
        };
        if let Some(ref pages) = names.pages {
            pages.walk(&file, &mut cb).unwrap();
        }
        if let Some(ref dests) = names.dests {
            dests.walk(&file, &mut cb).unwrap();
        }
        println!("{} items", count);
    }

    if let Some(ref outlines) = catalog.outlines {
        if let Some(entry_ref) = outlines.first {
            let entry = file.get(entry_ref).unwrap();
            walk_outline(&file, entry, 0);
        }
    }
}
