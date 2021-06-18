extern crate pdf;

use std::env::args;
use std::fmt;
use std::collections::HashMap;
use pdf::file::File;
use pdf::object::{*};
use pdf::primitive::PdfString;

struct Indent(usize);
impl fmt::Display for Indent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for _ in 0 .. self.0 {
            write!(f, "    ")?;
        }
        Ok(())
    } 
}

fn walk_outline(r: &impl Resolve, mut node: RcRef<OutlineItem>, map: &impl Fn(&str) -> usize, depth: usize) {
    let indent = Indent(depth);
    loop {
        if let Some(ref title) = node.title {
            println!("{}title: {:?}", indent, title.as_str().unwrap());
        }
        if let Some(ref dest) = node.dest {
            let name = dest.as_str().unwrap();
            let page_nr = map(&name);
            println!("{}dest: {:?} -> page nr. {:?}", indent, name, page_nr);
        }
        if let Some(entry_ref) = node.first {
            let entry = r.get(entry_ref).unwrap();
            walk_outline(r, entry, map, depth + 1);
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

    let mut pages_map: HashMap<String, PlainRef> = HashMap::new();

    let mut count = 0;
    let mut dests_cb = |key: &PdfString, val: &Dest| {
        //println!("{:?} {:?}", key, val);
        pages_map.insert(key.as_str().unwrap().into_owned(), val.page.get_inner());
        
        count += 1;
    };

    if let Some(ref names) = catalog.names {
        if let Some(ref dests) = names.dests {
            dests.walk(&file, &mut dests_cb).unwrap();
        }
    }

    let mut pages = HashMap::new();
    fn add_tree(r: &impl Resolve, pages: &mut HashMap<PlainRef, usize>, tree: &PageTree, current_page: &mut usize) {
        for &node_ref in &tree.kids {
            let node = r.get(node_ref).unwrap();
            match *node {
                PagesNode::Tree(ref tree) => {
                    add_tree(r, pages, tree, current_page);
                }
                PagesNode::Leaf(ref _page) => {
                    pages.insert(node_ref.get_inner(), *current_page);
                    *current_page += 1;
                }
            }
        }
    }
    add_tree(&file, &mut pages, &catalog.pages, &mut 0);
    
    let get_page_nr = |name: &str| -> usize {
        let page = pages_map[name];
        pages[&page]
    };

    if let Some(ref outlines) = catalog.outlines {
        if let Some(entry_ref) = outlines.first {
            let entry = file.get(entry_ref).unwrap();
            walk_outline(&file, entry, &get_page_nr, 0);
        }
    }

    
    println!("{} items", count);
}
