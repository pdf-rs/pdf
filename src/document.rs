use types::{Root, Pages, Page, PagesNode};
use file::File;
use primitive::{Primitive};
use object::PlainRef;
use err::*;
use std::collections::HashMap;

pub struct Document {
    root:       Root
}

// TODO a lot of errors here, not sure what to do
/*
impl Document {
    pub fn from_root<B>(root: &Primitive, reader: &File<B>) -> Result<Document> {
        let root_ref = reader.trailer.get("Root").chain_err(|| "No root entry in trailer.")?;
        let root = Root::from_primitive(&root_ref, reader)?;
        
        Ok(Document {
            root:       root
        })
    }

    /// Get number of pages in the PDF document. Reads the `/Pages` dictionary.
    pub fn get_num_pages(&self) -> i32 {
        self.root.count
    }

    /// Traverses the Pages/Page tree to find the page `n`. `n=0` is the first page.
    pub fn get_page(&self, n: i32) -> Result<Page> {
        if n >= self.get_num_pages() {
            return Err(ErrorKind::OutOfBounds.into());
        }
        self.find_page(n, 0, &self.root.pages)
    }
    fn find_page(&self, page_nr: i32, mut offset: i32, pages: &Pages) -> Result<&Page> {
        for kid in &pages.kids {
            match kid {
                PagesNode::Tree(ref t) => {
                    if offset + t.count < page_nr {
                        offset += t.count;
                    } else {
                        self.find_page(page_nr, offset, t);
                    }
                },
                PagesNode::Leaf(ref p) => {
                    if offset > page_nr {
                        offset += 1;
                    } else {
                        assert_eq!(offset, page_nr);
                        return Ok(p);
                    }
                }
            }
        }
        bail!("not found!");
    }
}
*/
