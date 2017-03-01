pub mod object;

use self::object::Object;
use file;
use file::{ObjectId, Dictionary, Stream, Reader};
use err::*;

use std::collections::HashMap;

pub struct Document {
    objects: HashMap<ObjectId, file::Object>,
    root: ObjectId,
}

impl Document {
    pub fn from_path(path: &str) -> Result<Document> {
        let mut doc = Document {
            objects: HashMap::new(),
            root: ObjectId {obj_nr: 0, gen_nr: 0},
        };
        let reader = Reader::from_path(path)?;
        for result in reader.objects() {
            let (id, object) = result.chain_err(|| "Document: error getting object from Reader.")?;
            doc.objects.insert(id, object);
        }

        let root_ref = reader.trailer.get("Root").chain_err(|| "No root entry in trailer.")?;
        doc.root = root_ref.as_reference()?;
        Ok(doc)
    }

    pub fn get_object<'a>(&'a self, id: ObjectId) -> Result<Object<'a>> {
        let obj: Result<&file::Object> = self.objects.get(&id).ok_or("Error getting object".into());
        Ok(
            Object::new(obj?, &self)
        )
    }

    pub fn get_page(n: usize) -> Result<Dictionary> {
        bail!("");
    }

}


#[cfg(test)]
mod tests {
    use ::Document;
    use ::print_err;

    static FILE: &'static str = "la.pdf";

    #[test]
    fn construct() {
        let doc = Document::from_path(FILE).unwrap_or_else(|e| print_err(e));
    }
}
