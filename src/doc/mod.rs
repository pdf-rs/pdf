//! Abstraction over the `file` module. Stores objects in high-level representation. Introduces wrappers for all kinds of PDF Objects (`file::AnyObject`), for easy PDF reference following.

mod object;

pub use self::object::*;
use file;
use file::{ObjectId, Reader};
use err::*;
use std::collections::HashMap;

/// `Document` keeps all objects of the PDf file stored in a high-level representation.

pub struct Document {
    objects: HashMap<ObjectId, file::AnyObject>,
    root_id: ObjectId,
}

impl Document {
    pub fn from_path(path: &str) -> Result<Document> {
        let mut doc = Document {
            objects: HashMap::new(),
            root_id: ObjectId {obj_nr: 0, gen_nr: 0},
        };
        let reader = Reader::from_path(path)?;
        for result in reader.objects() {
            let (id, object) = result.chain_err(|| "Document: error getting object from Reader.")?;
            doc.objects.insert(id, object);
        }

        let root_ref = reader.trailer.get("Root").chain_err(|| "No root entry in trailer.")?;
        doc.root_id = root_ref.as_reference()?;
        Ok(doc)
    }


    /// Does not follow references.
    pub fn get_object(&self, id: ObjectId) -> Result<Object> {
        let obj: Result<&file::AnyObject> = self.objects.get(&id).ok_or_else(|| "Error getting object".into());
        Ok(
            Object::new(obj?, self)
        )
    }

    /// Get number of pages in the PDF document. Reads the `/Pages` dictionary.
    pub fn get_num_pages(&self) -> Result<i32> {
        Ok(self.get_object(self.root_id)?.as_dictionary()?
            .get("Pages")?.as_dictionary()?
            .get("Count")?.as_integer()?
        )
    }

    /// Traverses the Pages/Page tree to find the page `n`. `n=0` is the first page.
    pub fn get_page(&self, n: i32) -> Result<Dictionary> {
        if n >= self.get_num_pages()? {
            return Err(ErrorKind::OutOfBounds.into());
        }
        let pages_root = self.get_object(self.root_id)?
            .as_dictionary()?
            .get("Pages")?
            .as_dictionary()?;
        let result = self.find_page(n, &mut 0, pages_root)?;
        match result {
            Some(page) => Ok(page),
            None => bail!("Failed to find page"),
        }

    }
    fn find_page<'a>(&'a self, page_nr: i32, progress: &mut i32, node: Dictionary<'a>) -> Result<Option<Dictionary<'a>>> {
        if *progress > page_nr {
            // Search has already passed the correct one...
            bail!("Search has passed the page nr, without finding the page.");
        }

        let node_type: String = node.get("Type")?.as_name()?;
        if node_type == "Pages" { // Intermediate node
            // Number of leaf nodes (pages) in this subtree
            let count = node.get("Count")?.as_integer()?;

            // If the target page is a descendant of the intermediate node
            if *progress + count > page_nr {
                let kids = node.get("Kids")?.as_array()?;
                // Traverse children of node.
                for kid in &kids {
                    let next_node: Dictionary = kid.as_dictionary()?;
                    let result = self.find_page(page_nr, progress, next_node)?;
                    if let Some(found_page) = result {
                        return Ok(Some(found_page));
                    };
                }
                Ok(None)
            } else {
                Ok(None)
            }
        } else if node_type == "Page" { // Leaf node
            if page_nr == *progress {
                Ok(Some(node))
            } else {
                *progress += 1;
                Ok(None)
            }
        } else {
            Err(format!("Expected /Type to be Page or Pages - but it is {}", node_type).into())
        }
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
