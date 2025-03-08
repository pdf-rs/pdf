//! Models of PDF types

use datasize::DataSize;
use prelude::Font;
use std::collections::HashMap;

use crate as pdf;
use crate::content::deep_clone_op;
use crate::content::{parse_ops, serialize_ops, Content, FormXObject, Matrix, Op};
use crate::error::*;
use crate::object::*;

mod prelude {
    pub use datasize::DataSize;

    pub use super::*;
    pub use crate as pdf;
    pub use crate::{error::*, font::Font, object::*, primitive::Primitive};
}

// As requested we try to not get files become too long,
// so related types get split out into their own module.

macro_rules! mods {
    ($($name:ident),*) => {
        $( mod $name; )*
        $( pub use $name::*; )*
    };
}

// too lazy to keep two sets of mod declarations and imports syncronized, so a macro it is ..
mods!(
    dest,
    form,
    graphicsstate,
    nametree,
    numbertree,
    outline,
    page,
    pagesnode,
    pattern,
    structtree,
    xobject
);

#[derive(Object, ObjectWrite, Debug, DataSize)]
#[pdf(Type = "Catalog?")]
pub struct Catalog {
    #[pdf(key = "Version")]
    pub version: Option<Name>,

    #[pdf(key = "Pages")]
    pub pages: PagesRc,

    #[pdf(key = "PageLabels")]
    pub page_labels: Option<NumberTree<PageLabel>>,

    #[pdf(key = "Names")]
    pub names: Option<MaybeRef<NameDictionary>>,

    #[pdf(key = "Dests")]
    pub dests: Option<MaybeRef<Dictionary>>,

    // ViewerPreferences: dict
    // PageLayout: name
    // PageMode: name
    #[pdf(key = "Outlines")]
    pub outlines: Option<Outlines>,
    // Threads: array
    // OpenAction: array or dict
    // AA: dict
    // URI: dict
    // AcroForm: dict
    #[pdf(key = "AcroForm")]
    pub forms: Option<InteractiveFormDictionary>,

    // Metadata: stream
    #[pdf(key = "Metadata")]
    pub metadata: Option<Ref<Stream<()>>>,

    #[pdf(key = "StructTreeRoot")]
    pub struct_tree_root: Option<StructTreeRoot>,
    // MarkInfo: dict
    // Lang: text string
    // SpiderInfo: dict
    // OutputIntents: array
    // PieceInfo: dict
    // OCProperties: dict
    // Perms: dict
    // Legal: dict
    // Requirements: array
    // Collection: dict
    // NeedsRendering: bool
}

#[derive(Object, ObjectWrite, Debug, DataSize, Default, DeepClone, Clone)]
pub struct Resources {
    #[pdf(key = "ExtGState")]
    pub graphics_states: HashMap<Name, GraphicsStateParameters>,

    #[pdf(key = "ColorSpace")]
    pub color_spaces: HashMap<Name, ColorSpace>,

    #[pdf(key = "Pattern")]
    pub pattern: HashMap<Name, Ref<Pattern>>,

    // shading: Option<Shading>,
    #[pdf(key = "XObject")]
    pub xobjects: HashMap<Name, Ref<XObject>>,
    // /XObject is a dictionary that map arbitrary names to XObjects
    #[pdf(key = "Font")]
    pub fonts: HashMap<Name, Lazy<Font>>,

    #[pdf(key = "Properties")]
    pub properties: HashMap<Name, MaybeRef<Dictionary>>,
}

#[derive(Object, Debug, Copy, Clone, DataSize, DeepClone, ObjectWrite)]
pub enum RenderingIntent {
    AbsoluteColorimetric,
    RelativeColorimetric,
    Saturation,
    Perceptual,
}
impl RenderingIntent {
    pub fn from_str(s: &str) -> Option<RenderingIntent> {
        match s {
            "AbsoluteColorimetric" => Some(RenderingIntent::AbsoluteColorimetric),
            "RelativeColorimetric" => Some(RenderingIntent::RelativeColorimetric),
            "Perceptual" => Some(RenderingIntent::Perceptual),
            "Saturation" => Some(RenderingIntent::Saturation),
            _ => None,
        }
    }
    pub fn to_str(self) -> &'static str {
        match self {
            RenderingIntent::AbsoluteColorimetric => "AbsoluteColorimetric",
            RenderingIntent::RelativeColorimetric => "RelativeColorimetric",
            RenderingIntent::Perceptual => "Perceptual",
            RenderingIntent::Saturation => "Saturation",
        }
    }
}

#[derive(Object, ObjectWrite, Clone, DeepClone, Debug)]
pub struct LageLabel {
    #[pdf(key = "S")]
    style: Option<Counter>,

    #[pdf(key = "P")]
    prefix: Option<PdfString>,

    #[pdf(key = "St")]
    start: Option<i32>,
}

#[derive(Debug, Clone, DataSize)]
pub enum DestView {
    // left, top, zoom
    XYZ {
        left: Option<f32>,
        top: Option<f32>,
        zoom: f32,
    },
    Fit,
    FitH {
        top: f32,
    },
    FitV {
        left: f32,
    },
    FitR(Rectangle),
    FitB,
    FitBH {
        top: f32,
    },
}

/// There is one `NameDictionary` associated with each PDF file.
#[derive(Object, ObjectWrite, Debug, DataSize)]
pub struct NameDictionary {
    #[pdf(key = "Pages")]
    pub pages: Option<NameTree<Primitive>>,

    #[pdf(key = "Dests")]
    pub dests: Option<NameTree<Option<Dest>>>,

    #[pdf(key = "AP")]
    pub ap: Option<NameTree<Primitive>>,

    #[pdf(key = "JavaScript")]
    pub javascript: Option<NameTree<Primitive>>,

    #[pdf(key = "Templates")]
    pub templates: Option<NameTree<Primitive>>,

    #[pdf(key = "IDS")]
    pub ids: Option<NameTree<Primitive>>,

    #[pdf(key = "URLS")]
    pub urls: Option<NameTree<Primitive>>,

    #[pdf(key = "EmbeddedFiles")]
    pub embedded_files: Option<NameTree<FileSpec>>,
    /*
    #[pdf(key="AlternativePresentations")]
    alternate_presentations: NameTree<AlternatePresentation>,
    #[pdf(key="Renditions")]
    renditions: NameTree<Rendition>,
    */
}

/* Embedded file streams can be associated with the document as a whole through
 * the EmbeddedFiles entry (PDF 1.4) in the PDF document’s name dictionary
 * (see Section 3.6.3, “Name Dictionary”).
 * The associated name tree maps name strings to file specifications that refer
 * to embedded file streams through their EF entries.
*/

#[derive(Object, ObjectWrite, Debug, Clone, DataSize, DeepClone)]
pub struct FileSpec {
    #[pdf(key = "EF")]
    pub ef: Option<Files<Ref<Stream<EmbeddedFile>>>>,
    /*
    #[pdf(key="RF")]
    rf: Option<Files<RelatedFilesArray>>,
    */
}

/// Used only as elements in `FileSpec`
#[derive(Object, ObjectWrite, Debug, Clone, DeepClone)]
pub struct Files<T> {
    #[pdf(key = "F")]
    pub f: Option<T>,
    #[pdf(key = "UF")]
    pub uf: Option<T>,
    #[pdf(key = "DOS")]
    pub dos: Option<T>,
    #[pdf(key = "Mac")]
    pub mac: Option<T>,
    #[pdf(key = "Unix")]
    pub unix: Option<T>,
}
impl<T: DataSize> DataSize for Files<T> {
    const IS_DYNAMIC: bool = T::IS_DYNAMIC;
    const STATIC_HEAP_SIZE: usize = 5 * Option::<T>::STATIC_HEAP_SIZE;

    fn estimate_heap_size(&self) -> usize {
        [&self.f, &self.uf, &self.dos, &self.mac, &self.unix]
            .into_iter()
            .filter_map(|o| o.as_ref())
            .map(|t| t.estimate_heap_size())
            .sum()
    }
}

/// PDF Embedded File Stream.
#[derive(Object, Debug, Clone, DataSize, DeepClone, ObjectWrite)]
pub struct EmbeddedFile {
    #[pdf(key = "Subtype")]
    subtype: Option<Name>,

    #[pdf(key = "Params")]
    pub params: Option<EmbeddedFileParamDict>,
}

#[derive(Object, Debug, Clone, DataSize, DeepClone, ObjectWrite)]
pub struct EmbeddedFileParamDict {
    #[pdf(key = "Size")]
    pub size: Option<i32>,

    #[pdf(key = "CreationDate")]
    creationdate: Option<Date>,

    #[pdf(key = "ModDate")]
    moddate: Option<Date>,

    #[pdf(key = "Mac")]
    mac: Option<Date>,

    #[pdf(key = "CheckSum")]
    checksum: Option<PdfString>,
}

/// ISO 32000-2:2020(E) 7.9.5 Rectangles (Pg 134)
/// specifying the lower-left x, lower-left y,
/// upper-right x, and upper-right y coordinates
/// of the rectangle, in that order. The other two
/// corners of the rectangle are then assumed to
/// have coordinates (ll x , ur y ) and
/// (ur x , ll y ).
/// Also see Table 74, key BBox definition Pg 221
/// defining top, left, bottom, right labeling
#[derive(Debug, Copy, Clone, DataSize, Default)]
pub struct Rectangle {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}
#[deprecated]
pub type Rect = Rectangle;

impl Object for Rectangle {
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        let arr = p.resolve(r)?.into_array()?;
        if arr.len() != 4 {
            bail!("len != 4 {:?}", arr);
        }
        Ok(Rectangle {
            left: arr[0].as_number()?,
            bottom: arr[1].as_number()?,
            right: arr[2].as_number()?,
            top: arr[3].as_number()?,
        })
    }
}
impl ObjectWrite for Rectangle {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        Primitive::array::<f32, _, _, _>(
            [self.left, self.bottom, self.right, self.top].iter(),
            update,
        )
    }
}

// Stuff from chapter 10 of the PDF 1.7 ref

#[derive(Object, ObjectWrite, Debug, DataSize)]
pub struct MarkInformation {
    // TODO no /Type
    /// indicating whether the document conforms to Tagged PDF conventions
    #[pdf(key = "Marked", default = "false")]
    pub marked: bool,
    /// Indicating the presence of structure elements that contain user properties attributes
    #[pdf(key = "UserProperties", default = "false")]
    pub user_properties: bool,
    /// Indicating the presence of tag suspects
    #[pdf(key = "Suspects", default = "false")]
    pub suspects: bool,
}

#[derive(Object, ObjectWrite, Debug, DataSize)]
pub enum Trapped {
    True,
    False,
    Unknown,
}

#[derive(Object, ObjectWrite, Debug, DataSize, Default)]
pub struct InfoDict {
    #[pdf(key = "Title")]
    pub title: Option<PdfString>,

    #[pdf(key = "Author")]
    pub author: Option<PdfString>,

    #[pdf(key = "Subject")]
    pub subject: Option<PdfString>,

    #[pdf(key = "Keywords")]
    pub keywords: Option<PdfString>,

    #[pdf(key = "Creator")]
    pub creator: Option<PdfString>,

    #[pdf(key = "Producer")]
    pub producer: Option<PdfString>,

    #[pdf(key = "CreationDate")]
    pub creation_date: Option<Date>,

    #[pdf(key = "ModDate")]
    pub mod_date: Option<Date>,

    #[pdf(key = "Trapped")]
    pub trapped: Option<Trapped>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_struct_type() {
        assert!(matches!(
            StructType::from_primitive(Primitive::Name("BibEntry".into()), &NoResolve),
            Ok(StructType::BibEntry)
        ));

        let result =
            StructType::from_primitive(Primitive::Name("CustomStructType".into()), &NoResolve);
        if let Ok(StructType::Other(name)) = &result {
            assert_eq!(name, "CustomStructType");
        } else {
            panic!("Incorrect result of {:?}", &result);
        }
    }

    #[test]
    fn test_field_type() {
        assert_eq!(
            FieldType::from_primitive(Primitive::Name("Tx".into()), &NoResolve).unwrap(),
            FieldType::Text
        );
    }
}
