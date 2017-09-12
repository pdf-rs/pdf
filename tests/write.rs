// TODO: commented out to make it compile
/*
extern crate pdf;

use pdf::file::File;
use pdf::types::*;
use pdf::stream::ObjectStream;

fn main() {
    let mut file = File::new(Vec::new());
    
    let page_tree_promise = file.promise();
    let mut page_tree = PageTree::root();
    let mut page = Page::new((&page_tree_promise).into());
    page.media_box = Some(Rect {
        left: 0.,
        right: 100.,
        top: 0.,
        bottom: 200.
    });
    
    // create the content stream
    let content = ObjectStream::new(&mut file);
    
    // add stream to file
    let content_ref = file.add(content);
    
    page_tree.add(file.add(PagesNode::Page(page)).unwrap());
    
    let catalog = Catalog::new(file.fulfill(page_tree_promise, page_tree).unwrap());
    
    let catalog_ref = file.add(catalog).unwrap();
    file.finish(catalog_ref);
}
*/
