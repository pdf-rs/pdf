use file::{Object, ObjectId};
use std::marker::PhantomData;
use std::collections::HashMap;
use std::io;

/* Some more basic types for which we explicitly impl Object */
pub struct Ref<T> {
    id: ObjectId,
    _marker: PhantomData<T>,
}
impl<T> Object for Ref<T> {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "{} {}R", self.id.obj_nr, self.id.gen_nr)
    }
}

/// Node in a page tree - type is either `Page` or `Pages`
pub enum PagesNode {
    Tree (Ref<Pages>),
    Leaf (Ref<Page>),
}
impl Object for PagesNode {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        match *self {
            PagesNode::Tree (ref t) => t.serialize(out),
            PagesNode::Leaf (ref l) => l.serialize(out),
        }
    }
}

impl<T> Object for Vec<T>
    where T: Object
{
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        Ok(()) // TODO
    }
}


// TODO: should impl Object for Primitive. But also need it for i32 - is this right?
impl Object for i32
{
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        Ok(()) // TODO
    }
}

/* Dictionary Types */

#[derive(Object)]
pub struct Catalog {
    #[pdf(key="Pages", opt=false)]
    pages:  Ref<Pages>,
    
    // #[pdf(key="Labels", opt=false]
    // labels: HashMap<usize, PageLabel>
}



#[derive(Object)]
pub struct Pages { // TODO would like to call it PageTree, but the macro would have to change
    #[pdf(key="Parent", opt=true)]
    parent: Option<Ref<Pages>>,
    #[pdf(key="Kids", opt=false)]
    kids: Vec<PagesNode>,
    #[pdf(key="Count", opt=false)]
    count: i32, // TODO implement Object 

    // #[pdf(key="Resources", opt=false]
    // resources: Option<Ref<Resources>>,
}

#[derive(Object)]
pub struct Page {
    #[pdf(key="Parent", opt=false)]
    parent: Ref<Pages>
}





/*
/// `/Type Page`
qtyped!(Page {
    parent: PageTree,
    resources: Option<Resources>,
});
/// `/Type Pages`
qtyped!(PageTree {
    parent: Option<PageTree>,
    kids: Vec<ObjectId>,
    count: i32,
    resources: Option<Resources>,
});
/// `/Type Resources` - resource dictionary.
qtyped!(Resources {
    ext_g_state: Option<ExtGState>,
    color_space: Dictionary,
    // TODO:
    // Pattern
    // Shading
    // XObject
    // Font
    // ProcSet
    // Properties

});

/// `/Type ExtGState` - graphics state parameter dictionary.
qtyped!(ExtGState {
    line_width: Option<String>,
    line_cap_style: Option<i32>,
    line_join_style: Option<i32>,
    // TODO ETC
});

/// `/Type Catalog`
pub struct Catalog {
    pub version: Option<String>,
    /// `/Pages`
    pub page_tree: PageTree,
    // TODO PageLabels
    pub names: Option<Dictionary>,
    // TODO rest
}
 */
