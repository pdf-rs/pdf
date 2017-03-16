//! Abstraction over the `file` module. Stores objects in high-level representation. Introduces wrappers for all kinds of PDF Objects (`file::Primitive`), for easy PDF reference following.

pub mod object;
pub mod types;
use self::types::{Root, Pages, Page, PagesNode};

pub use self::object::*;
use file;
use file::{ObjectId, Reader, Primitive};
use err::*;
use std::collections::HashMap;

/// `Document` keeps all objects of the PDf file stored in a high-level representation.

pub struct Document {
    root_id:    ObjectId,
    root:       Root
}

impl Document {
    pub fn from_root(root: &Primitive, reader: &Reader) -> Result<Document> {
        let root_ref = reader.trailer.get("Root").chain_err(|| "No root entry in trailer.")?;
        let root = Root::from_primitive(&root_ref, reader)?;
        
        Ok(Document {
            root_id:    root_ref.as_reference()?,
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

#[cfg(test)]
mod tests {
    use doc::Document;
    use print_err;

    static FILE: &'static str = "la.pdf";

    #[test]
    fn construct() {
        let _ = Document::from_path(FILE).unwrap_or_else(|e| print_err(e));
    }
    #[test]
    fn pages() {
        let doc = Document::from_path(FILE).unwrap_or_else(|e| print_err(e));
        for n in 0..doc.get_num_pages().unwrap_or_else(|e| print_err(e)) {
            let _ = doc.get_page(n).unwrap_or_else(|e| print_err(e));
        }
    }
}
