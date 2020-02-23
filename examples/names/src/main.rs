extern crate pdf;

use std::rc::Rc;
use std::env::args;
use std::fmt;
use std::collections::HashMap;
use std::borrow::Cow;
use pdf::file::File;
use pdf::object::{Resolve, OutlineItem, Dest, Page, Ref, PagesNode};
use pdf::primitive::{PdfString, Primitive};
use std::hash::{Hash, Hasher};

struct RcPointerId<T>(Rc<T>);
impl<T> PartialEq for RcPointerId<T> {
    fn eq(&self, other: &Self) -> bool {
        &*self.0 as *const T == &*other.0 as *const T
    }
}
impl<T> Eq for RcPointerId<T> {}
impl<T> Hash for RcPointerId<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (&*self.0 as *const T).hash(state);
    } 
}
struct Indent(usize);
impl fmt::Display for Indent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for _ in 0 .. self.0 {
            write!(f, "    ")?;
        }
        Ok(())
    } 
}

fn walk_outline(r: &impl Resolve, mut node: Rc<OutlineItem>, map: &impl Fn(&str) -> usize, depth: usize) {
    let indent = Indent(depth);
    loop {
        if let Some(ref title) = node.title {
            println!("{}title: {:?}", indent, tr.get(*title).unwrap());
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

    let mut pages_map = HashMap::new();

    let mut count = 0;
    let mut dests_cb = |key: &PdfString, val: &Dest| {
        //println!("{:?} {:?}", key, val);
        pages_map.insert(key.as_str().unwrap().into_owned(), val.clone());
        
        count += 1;
    };

    if let Some(ref names) = catalog.names {
        if let Some(ref dests) = names.dests {
            dests.walk(&file, &mut dests_cb).unwrap();
        }
    }

    let mut pages = HashMap::new();
    fn add_node(r: &impl Resolve, pages: &mut HashMap<RcPointerId<PagesNode>, usize>, node: Rc<PagesNode>, current_page: &mut usize) {
        let page = *current_page;
        match *node {
            PagesNode::Tree(ref tree) => {
                for &node_ref in &tree.kids {
                    let node = r.get(node_ref).unwrap();
                    add_node(r, pages, node, current_page);
                }
            }
            PagesNode::Leaf(ref page) => {
                *current_page += 1;
            }
        }
        pages.insert(RcPointerId(node), page);
    }
    add_node(&file, &mut pages, Rc::clone(&catalog.pages), &mut 0);
    
    let get_page_nr = |name: &str| -> usize {
        let rc = file.get(pages_map[name].page).unwrap();
        let rc = RcPointerId(rc);
        pages[&rc]
    };

    if let Some(ref outlines) = catalog.outlines {
        if let Some(entry_ref) = outlines.first {
            let entry = file.get(entry_ref).unwrap();
            walk_outline(&file, entry, &get_page_nr, 0);
        }
    }

    
    println!("{} items", count);
}
