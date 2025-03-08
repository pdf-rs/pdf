extern crate pdf;

use pdf::file::FileOptions;
use pdf::object::*;
use pdf::primitive::{PdfString, Primitive};
use std::collections::HashMap;
use std::env::args;
use std::fmt;

struct Indent(usize);
impl fmt::Display for Indent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for _ in 0..self.0 {
            write!(f, "    ")?;
        }
        Ok(())
    }
}

fn walk_outline(
    r: &impl Resolve,
    mut node: RcRef<OutlineItem>,
    name_map: &impl Fn(&str) -> usize,
    page_map: &impl Fn(PlainRef) -> usize,
    depth: usize,
) {
    let indent = Indent(depth);
    loop {
        if let Some(ref title) = node.title {
            println!("{}title: {:?}", indent, title.to_string_lossy());
        }
        if let Some(ref dest) = node.dest {
            match dest {
                Primitive::String(ref s) => {
                    let name = s.to_string_lossy();
                    let page_nr = name_map(&name);
                    println!("{}dest: {:?} -> page nr. {:?}", indent, name, page_nr);
                }
                Primitive::Array(ref a) => match a[0] {
                    Primitive::Reference(r) => {
                        let page_nr = page_map(r);
                        println!("{}dest: {:?} -> page nr. {:?}", indent, a, page_nr);
                    }
                    _ => unimplemented!("invalid reference in array"),
                },
                _ => unimplemented!("invalid dest"),
            }
        }
        if let Some(Action::Goto(MaybeNamedDest::Direct(Dest {
            page: Some(page), ..
        }))) = node.action
        {
            let page_nr = page_map(page.get_inner());
            println!("{}action -> page nr. {:?}", indent, page_nr);
        }
        if let Some(ref a) = node.se {
            println!("{} -> {:?}", indent, a);
        }
        if let Some(entry_ref) = node.first {
            let entry = r.get(entry_ref).unwrap();
            walk_outline(r, entry, name_map, page_map, depth + 1);
        }
        if let Some(entry_ref) = node.next {
            node = r.get(entry_ref).unwrap();
            continue;
        }

        break;
    }
}

#[cfg(feature = "cache")]
fn main() {
    let path = args().nth(1).expect("no file given");
    println!("read: {}", path);

    let file = FileOptions::cached().open(&path).unwrap();
    let resolver = file.resolver();
    let catalog = file.get_root();

    let mut pages_map: HashMap<String, PlainRef> = HashMap::new();

    let mut count = 0;
    let mut dests_cb = |key: &PdfString, val: &Option<Dest>| {
        //println!("{:?} {:?}", key, val);
        if let Some(Dest {
            page: Some(page), ..
        }) = val
        {
            pages_map.insert(key.to_string_lossy(), page.get_inner());
        }

        count += 1;
    };

    if let Some(ref names) = catalog.names {
        if let Some(ref dests) = names.dests {
            dests.walk(&resolver, &mut dests_cb).unwrap();
        }
    }

    let mut pages = HashMap::new();
    fn add_tree(
        r: &impl Resolve,
        pages: &mut HashMap<PlainRef, usize>,
        tree: &PageTree,
        current_page: &mut usize,
    ) {
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
    add_tree(&resolver, &mut pages, &catalog.pages, &mut 0);

    let get_page_nr = |name: &str| -> usize {
        let page = pages_map[name];
        pages[&page]
    };
    let page_nr = |r: PlainRef| -> usize { pages[&r] };

    if let Some(ref outlines) = catalog.outlines {
        if let Some(entry_ref) = outlines.first {
            let entry = resolver.get(entry_ref).unwrap();
            walk_outline(&resolver, entry, &get_page_nr, &page_nr, 0);
        }
    }

    println!("{} items", count);

    if let Some(ref labels) = catalog.page_labels {
        labels
            .walk(&resolver, &mut |page: i32, label| {
                println!("{page} -> {:?}", label);
            })
            .unwrap();
    }
}
