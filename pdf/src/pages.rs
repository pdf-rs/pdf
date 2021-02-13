use crate::object::*;
use crate::file::*;

pub struct CatalogBuilder {
    pages: Vec<Page>
}
impl CatalogBuilder {
    pub fn build(self, update: &mut impl Updater) -> Catalog {

    }
}