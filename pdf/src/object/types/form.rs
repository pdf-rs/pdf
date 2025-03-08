use super::prelude::*;

#[derive(Object, Debug, DataSize, DeepClone, ObjectWrite, Clone, Default)]
#[pdf(Type = "XObject?", Subtype = "Form")]
pub struct FormDict {
    #[pdf(key = "FormType", default = "1")]
    pub form_type: i32,

    #[pdf(key = "Name")]
    pub name: Option<Name>,

    #[pdf(key = "LastModified")]
    pub last_modified: Option<PdfString>,

    #[pdf(key = "BBox")]
    pub bbox: Rectangle,

    #[pdf(key = "Matrix")]
    pub matrix: Option<Primitive>,

    #[pdf(key = "Resources")]
    pub resources: Option<MaybeRef<Resources>>,

    #[pdf(key = "Group")]
    pub group: Option<Dictionary>,

    #[pdf(key = "Ref")]
    pub reference: Option<Dictionary>,

    #[pdf(key = "Metadata")]
    pub metadata: Option<Ref<Stream<()>>>,

    #[pdf(key = "PieceInfo")]
    pub piece_info: Option<Dictionary>,

    #[pdf(key = "StructParent")]
    pub struct_parent: Option<i32>,

    #[pdf(key = "StructParents")]
    pub struct_parents: Option<i32>,

    #[pdf(key = "OPI")]
    pub opi: Option<Dictionary>,

    #[pdf(other)]
    pub other: Dictionary,
}

#[derive(Object, ObjectWrite, Debug, Clone, DataSize)]
pub struct InteractiveFormDictionary {
    #[pdf(key = "Fields")]
    pub fields: Vec<RcRef<FieldDictionary>>,

    #[pdf(key = "NeedAppearances", default = "false")]
    pub need_appearences: bool,

    #[pdf(key = "SigFlags", default = "0")]
    pub sig_flags: u32,

    #[pdf(key = "CO")]
    pub co: Option<Vec<RcRef<FieldDictionary>>>,

    #[pdf(key = "DR")]
    pub dr: Option<MaybeRef<Resources>>,

    #[pdf(key = "DA")]
    pub da: Option<PdfString>,

    #[pdf(key = "Q")]
    pub q: Option<i32>,

    #[pdf(key = "XFA")]
    pub xfa: Option<Primitive>,
}

#[derive(Object, ObjectWrite, Debug, Copy, Clone, PartialEq, DataSize)]
pub enum FieldType {
    #[pdf(name = "Btn")]
    Button,
    #[pdf(name = "Tx")]
    Text,
    #[pdf(name = "Ch")]
    Choice,
    #[pdf(name = "Sig")]
    Signature,
    #[pdf(name = "SigRef")]
    SignatureReference,
}

#[derive(Object, ObjectWrite, Debug)]
#[pdf(Type = "SV")]
pub struct SeedValueDictionary {
    #[pdf(key = "Ff", default = "0")]
    pub flags: u32,
    #[pdf(key = "Filter")]
    pub filter: Option<Name>,
    #[pdf(key = "SubFilter")]
    pub sub_filter: Option<Vec<Name>>,
    #[pdf(key = "V")]
    pub value: Option<Primitive>,
    #[pdf(key = "DigestMethod")]
    pub digest_method: Vec<PdfString>,
    #[pdf(other)]
    pub other: Dictionary,
}

#[derive(Object, ObjectWrite, Debug)]
#[pdf(Type = "Sig?")]
pub struct SignatureDictionary {
    #[pdf(key = "Filter")]
    pub filter: Name,
    #[pdf(key = "SubFilter")]
    pub sub_filter: Name,
    #[pdf(key = "ByteRange")]
    pub byte_range: Vec<usize>,
    #[pdf(key = "Contents")]
    pub contents: PdfString,
    #[pdf(key = "Cert")]
    pub cert: Vec<PdfString>,
    #[pdf(key = "Reference")]
    pub reference: Option<Primitive>,
    #[pdf(key = "Name")]
    pub name: Option<PdfString>,
    #[pdf(key = "M")]
    pub m: Option<PdfString>,
    #[pdf(key = "Location")]
    pub location: Option<PdfString>,
    #[pdf(key = "Reason")]
    pub reason: Option<PdfString>,
    #[pdf(key = "ContactInfo")]
    pub contact_info: Option<PdfString>,
    #[pdf(key = "V")]
    pub v: i32,
    #[pdf(key = "R")]
    pub r: i32,
    #[pdf(key = "Prop_Build")]
    pub prop_build: Dictionary,
    #[pdf(key = "Prop_AuthTime")]
    pub prop_auth_time: i32,
    #[pdf(key = "Prop_AuthType")]
    pub prop_auth_type: Name,
    #[pdf(other)]
    pub other: Dictionary,
}

#[derive(Object, ObjectWrite, Debug)]
#[pdf(Type = "SigRef?")]
pub struct SignatureReferenceDictionary {
    #[pdf(key = "TransformMethod")]
    pub transform_method: Name,
    #[pdf(key = "TransformParams")]
    pub transform_params: Option<Dictionary>,
    #[pdf(key = "Data")]
    pub data: Option<Primitive>,
    #[pdf(key = "DigestMethod")]
    pub digest_method: Option<Name>,
    #[pdf(other)]
    pub other: Dictionary,
}

#[derive(Object, ObjectWrite, Debug, Clone, DataSize)]
#[pdf(Type = "Annot?")]
pub struct Annot {
    #[pdf(key = "Subtype")]
    pub subtype: Name,

    #[pdf(key = "Rect")]
    pub rect: Option<Rectangle>,

    #[pdf(key = "Contents")]
    pub contents: Option<PdfString>,

    #[pdf(key = "P")]
    pub page: Option<PageRc>,

    #[pdf(key = "NM")]
    pub annotation_name: Option<PdfString>,

    #[pdf(key = "M")]
    pub date: Option<Date>,

    #[pdf(key = "F", default = "0")]
    pub annot_flags: u32,

    #[pdf(key = "AP")]
    pub appearance_streams: Option<MaybeRef<AppearanceStreams>>,

    #[pdf(key = "AS")]
    pub appearance_state: Option<Name>,

    #[pdf(key = "Border")]
    pub border: Option<Primitive>,

    #[pdf(key = "C")]
    pub color: Option<Primitive>,

    #[pdf(key = "InkList")]
    pub ink_list: Option<Primitive>,

    #[pdf(key = "L")]
    pub line: Option<Vec<f32>>,

    #[pdf(other)]
    pub other: Dictionary,
}

#[derive(Object, ObjectWrite, Debug, DataSize, Clone)]
pub struct FieldDictionary {
    #[pdf(key = "FT")]
    pub typ: Option<FieldType>,

    #[pdf(key = "Parent")]
    pub parent: Option<Ref<FieldDictionary>>,

    #[pdf(key = "Kids")]
    pub kids: Vec<Ref<Merged<FieldDictionary, Annot>>>,

    #[pdf(key = "T")]
    pub name: Option<PdfString>,

    #[pdf(key = "TU")]
    pub alt_name: Option<PdfString>,

    #[pdf(key = "TM")]
    pub mapping_name: Option<PdfString>,

    #[pdf(key = "Ff", default = "0")]
    pub flags: u32,

    #[pdf(key = "SigFlags", default = "0")]
    pub sig_flags: u32,

    #[pdf(key = "V")]
    pub value: Primitive,

    #[pdf(key = "DV")]
    pub default_value: Primitive,

    #[pdf(key = "DR")]
    pub default_resources: Option<MaybeRef<Resources>>,

    #[pdf(key = "AA")]
    pub actions: Option<Dictionary>,

    #[pdf(key = "Rect")]
    pub rect: Option<Rectangle>,

    #[pdf(key = "MaxLen")]
    pub max_len: Option<u32>,

    #[pdf(key = "Subtype")]
    pub subtype: Option<Name>,

    #[pdf(other)]
    pub other: Dictionary,
}

#[derive(Object, ObjectWrite, Debug, DataSize, Clone, DeepClone)]
pub struct AppearanceStreams {
    #[pdf(key = "N")]
    pub normal: Ref<AppearanceStreamEntry>,

    #[pdf(key = "R")]
    pub rollover: Option<Ref<AppearanceStreamEntry>>,

    #[pdf(key = "D")]
    pub down: Option<Ref<AppearanceStreamEntry>>,
}

#[derive(Clone, Debug, DeepClone)]
pub enum AppearanceStreamEntry {
    Single(FormXObject),
    Dict(HashMap<Name, AppearanceStreamEntry>),
}
impl Object for AppearanceStreamEntry {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p.resolve(resolve)? {
            p @ Primitive::Dictionary(_) => {
                Object::from_primitive(p, resolve).map(AppearanceStreamEntry::Dict)
            }
            p @ Primitive::Stream(_) => {
                Object::from_primitive(p, resolve).map(AppearanceStreamEntry::Single)
            }
            p => Err(PdfError::UnexpectedPrimitive {
                expected: "Dict or Stream",
                found: p.get_debug_name(),
            }),
        }
    }
}
impl ObjectWrite for AppearanceStreamEntry {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match self {
            AppearanceStreamEntry::Dict(d) => d.to_primitive(update),
            AppearanceStreamEntry::Single(s) => s.to_primitive(update),
        }
    }
}
impl DataSize for AppearanceStreamEntry {
    const IS_DYNAMIC: bool = true;
    const STATIC_HEAP_SIZE: usize = std::mem::size_of::<Self>();
    fn estimate_heap_size(&self) -> usize {
        match self {
            AppearanceStreamEntry::Dict(d) => d.estimate_heap_size(),
            AppearanceStreamEntry::Single(s) => s.estimate_heap_size(),
        }
    }
}
