use crate::error::*;
use crate::object::{PlainRef, Resolve, Object, NoResolve, ObjectWrite, Updater, DeepClone, Cloner};

use std::sync::Arc;
use std::{str, fmt, io};
use std::ops::{Index, Range};
use std::ops::Deref;
use std::convert::TryInto;
use std::borrow::{Borrow, Cow};
use indexmap::IndexMap;
use itertools::Itertools;
use istring::{SmallString, IBytes};
use datasize::DataSize;

#[derive(Clone, Debug, PartialEq)]
pub enum Primitive {
    Null,
    Integer (i32),
    Number (f32),
    Boolean (bool),
    String (PdfString),
    Stream (PdfStream),
    Dictionary (Dictionary),
    Array (Vec<Primitive>),
    Reference (PlainRef),
    Name (SmallString),
}
impl DataSize for Primitive {
    const IS_DYNAMIC: bool = true;
    const STATIC_HEAP_SIZE: usize = std::mem::size_of::<Self>();

    fn estimate_heap_size(&self) -> usize {
        match self {
            Primitive::String(ref s) => s.estimate_heap_size(),
            Primitive::Stream(ref s) => s.estimate_heap_size(),
            Primitive::Dictionary(ref d) => d.estimate_heap_size(),
            Primitive::Array(ref arr) => arr.estimate_heap_size(),
            Primitive::Name(ref s) => s.estimate_heap_size(),
            _ => 0
        }
    }
}

impl fmt::Display for Primitive {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Primitive::Null => write!(f, "null"),
            Primitive::Integer(i) => i.fmt(f),
            Primitive::Number(n) => n.fmt(f),
            Primitive::Boolean(b) => b.fmt(f),
            Primitive::String(ref s) => write!(f, "{:?}", s),
            Primitive::Stream(_) => write!(f, "stream"),
            Primitive::Dictionary(ref d) => d.fmt(f),
            Primitive::Array(ref arr) => write!(f, "[{}]", arr.iter().format(", ")),
            Primitive::Reference(r) => write!(f, "@{}", r.id),
            Primitive::Name(ref s) => write!(f, "/{}", s)
        }
    }
}
impl Primitive {
    pub fn serialize(&self, out: &mut impl io::Write) -> Result<()> {
        match self {
            Primitive::Null => write!(out, "null")?,
            Primitive::Integer(i) => write!(out, "{}", i)?,
            Primitive::Number(n) => write!(out, "{}", n)?,
            Primitive::Boolean(b) => write!(out, "{}", b)?,
            Primitive::String(ref s) => s.serialize(out)?,
            Primitive::Stream(ref s) => s.serialize(out)?,
            Primitive::Dictionary(ref d) => d.serialize(out)?,
            Primitive::Array(ref arr) => serialize_list(arr, out)?,
            Primitive::Reference(r) =>  write!(out, "{} {} R", r.id, r.gen)?,
            Primitive::Name(ref s) => serialize_name(s, out)?,
        }
        Ok(())
    }
    pub fn array<O, T, I, U>(i: I, update: &mut U) -> Result<Primitive>
        where O: ObjectWrite, I: Iterator<Item=T>,
        T: Borrow<O>, U: Updater
    {
        i.map(|t| t.borrow().to_primitive(update)).collect::<Result<_>>().map(Primitive::Array)
    }
    pub fn name(name: impl Into<SmallString>) -> Primitive {
        Primitive::Name(name.into())
    }
}

fn serialize_list(arr: &[Primitive], out: &mut impl io::Write) -> Result<()> {
    let mut parts = arr.iter();
    write!(out, "[")?;
    if let Some(first) = parts.next() {
        first.serialize(out)?;
    }
    for p in parts {
        write!(out, " ")?;
        p.serialize(out)?;
    }
    write!(out, "]")?;
    Ok(())
}

pub fn serialize_name(s: &str, out: &mut impl io::Write) -> Result<()> {
    write!(out, "/")?;
    for b in s.chars() {
        match b {
            '\\' | '(' | ')' => write!(out, r"\")?,
            c if c > '~' => panic!("only ASCII"),
            _ => ()
        }
        write!(out, "{}", b)?;
    }
    Ok(())
}

/// Primitive Dictionary type.
#[derive(Default, Clone, PartialEq)]
pub struct Dictionary {
    dict: IndexMap<Name, Primitive>
}
impl Dictionary {
    pub fn new() -> Dictionary {
        Dictionary { dict: IndexMap::new()}
    }
    pub fn len(&self) -> usize {
        self.dict.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, key: &str) -> Option<&Primitive> {
        self.dict.get(key)
    }
    pub fn insert(&mut self, key: impl Into<Name>, val: impl Into<Primitive>) -> Option<Primitive> {
        self.dict.insert(key.into(), val.into())
    }
    pub fn iter(&self) -> impl Iterator<Item=(&Name, &Primitive)> {
        self.dict.iter()
    }
    pub fn remove(&mut self, key: &str) -> Option<Primitive> {
        self.dict.remove(key)
    }
    /// like remove, but takes the name of the calling type and returns `PdfError::MissingEntry` if the entry is not found
    pub fn require(&mut self, typ: &'static str, key: &str) -> Result<Primitive> {
        self.remove(key).ok_or(
            PdfError::MissingEntry {
                typ,
                field: key.into()
            }
        )
    }
    /// assert that the given key/value pair is in the dictionary (`required=true`),
    /// or the key is not present at all (`required=false`)
    pub fn expect(&self, typ: &'static str, key: &str, value: &str, required: bool) -> Result<()> {
        match self.dict.get(key) {
            Some(ty) => {
                let ty = ty.as_name()?;
                if ty != value {
                    Err(PdfError::KeyValueMismatch {
                        key: key.into(),
                        value: value.into(),
                        found: ty.into()
                    })
                } else {
                    Ok(())
                }
            },
            None if required => Err(PdfError::MissingEntry { typ, field: key.into() }),
            None => Ok(())
        }
    }
    pub fn append(&mut self, other: Dictionary) {
        self.dict.extend(other.dict);
    }
}
impl DataSize for Dictionary {
    const IS_DYNAMIC: bool = true;
    const STATIC_HEAP_SIZE: usize = std::mem::size_of::<Self>();
    fn estimate_heap_size(&self) -> usize {
        self.iter().map(|(k, v)| 16 + k.estimate_heap_size() + v.estimate_heap_size()).sum()
    }
}
impl ObjectWrite for Dictionary {
    fn to_primitive(&self, _update: &mut impl Updater) -> Result<Primitive> {
        Ok(Primitive::Dictionary(self.clone()))
    }
}
impl DeepClone for Dictionary {
    fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self> {
        Ok(Dictionary {
            dict: self.dict.iter()
                .map(|(key, value)| Ok((key.clone(), value.deep_clone(cloner)?)))
                .try_collect::<_, _, PdfError>()?
        })
    }
}
impl Deref for Dictionary {
    type Target = IndexMap<Name, Primitive>;
    fn deref(&self) -> &IndexMap<Name, Primitive> {
        &self.dict
    }
}
impl Dictionary {
    fn serialize(&self, out: &mut impl io::Write) -> Result<()> {
        writeln!(out, "<<")?;
        for (key, val) in self.iter() {
            write!(out, "{} ", key)?;
            val.serialize(out)?;
            writeln!(out)?;
        }
        writeln!(out, ">>")?;
        Ok(())
    }
}
impl fmt::Debug for Dictionary {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "{{")?;
        for (k, v) in self {
            writeln!(f, "{:>15}: {}", k, v)?;
        }
        write!(f, "}}")
    }
}
impl fmt::Display for Dictionary {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<{}>", self.iter().format_with(", ", |(k, v), f| f(&format_args!("{}={}", k, v))))
    }
}
impl<'a> Index<&'a str> for Dictionary {
    type Output = Primitive;
    fn index(&self, idx: &'a str) -> &Primitive {
        self.dict.index(idx)
    }
}
impl IntoIterator for Dictionary {
    type Item = (Name, Primitive);
    type IntoIter = indexmap::map::IntoIter<Name, Primitive>;
    fn into_iter(self) -> Self::IntoIter {
        self.dict.into_iter()
    }
}
impl<'a> IntoIterator for &'a Dictionary {
    type Item = (&'a Name, &'a Primitive);
    type IntoIter = indexmap::map::Iter<'a, Name, Primitive>;
    fn into_iter(self) -> Self::IntoIter {
        self.dict.iter()
    }
}

/// Primitive Stream (as opposed to the higher-level `Stream`)
#[derive(Clone, Debug, PartialEq, DataSize)]
pub struct PdfStream {
    pub info: Dictionary,
    pub (crate) inner: StreamInner,
}

#[derive(Clone, Debug, PartialEq, DataSize)]
pub enum StreamInner {
    InFile { id: PlainRef, file_range: Range<usize> },
    Pending { data: Arc<[u8]> },
}
impl Object for PdfStream {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Stream (stream) => Ok(stream),
            Primitive::Reference (r) => PdfStream::from_primitive(resolve.resolve(r)?, resolve),
            p => Err(PdfError::UnexpectedPrimitive {expected: "Stream", found: p.get_debug_name()})
        }
    }
}
impl ObjectWrite for PdfStream {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match self.inner {
            StreamInner::InFile { id, .. } => Ok(Primitive::Reference(id)),
            StreamInner::Pending { .. } => Ok(self.clone().into()),
        }
    }
}
impl PdfStream {
    pub fn serialize(&self, out: &mut impl io::Write) -> Result<()> {

        match self.inner {
            StreamInner::InFile { id, .. } => {
                Primitive::Reference(id).serialize(out)?;
            }
            StreamInner::Pending { ref data } => {
                self.info.serialize(out)?;
                writeln!(out, "stream")?;
                out.write_all(data)?;
                writeln!(out, "\nendstream")?;
            }
        }
        Ok(())
    }
    pub fn raw_data(&self, resolve: &impl Resolve) -> Result<Arc<[u8]>> {
        match self.inner {
            StreamInner::InFile { id, ref file_range } => resolve.stream_data(id, file_range.clone()),
            StreamInner::Pending { ref data } => Ok(data.clone())
        }
    }
}
impl DeepClone for PdfStream {
    fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self> {
        let data = match self.inner {
            StreamInner::InFile { id, ref file_range } => cloner.stream_data(id, file_range.clone())?,
            StreamInner::Pending { ref data } => data.clone()
        };
        Ok(PdfStream {
            info: self.info.deep_clone(cloner)?, inner: StreamInner::Pending { data }
        })
    }
}


macro_rules! unexpected_primitive {
    ($expected:ident, $found:expr) => (
        Err(PdfError::UnexpectedPrimitive {
            expected: stringify!($expected),
            found: $found
        })
    )
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Ord, PartialOrd, DataSize)]
pub struct Name(pub SmallString);
impl Name {
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl Deref for Name {
    type Target = str;
    #[inline]
    fn deref(&self) -> &str {
        &self.0
    }
}
impl From<String> for Name {
    #[inline]
    fn from(s: String) -> Name {
        Name(s.into())
    }
}
impl From<SmallString> for Name {
    #[inline]
    fn from(s: SmallString) -> Name {
        Name(s)
    }
}
impl<'a> From<&'a str> for Name {
    #[inline]
    fn from(s: &'a str) -> Name {
        Name(s.into())
    }
}
impl PartialEq<str> for Name {
    #[inline]
    fn eq(&self, rhs: &str) -> bool {
        self.as_str() == rhs
    }
}
impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "/{}", self.0)
    }
}
impl std::borrow::Borrow<str> for Name {
    #[inline]
    fn borrow(&self) -> &str {
        self.0.as_str()
    }
}
#[test]
fn test_name() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let s = "Hello World!";
    let hasher = DefaultHasher::new();

    fn hash(hasher: &DefaultHasher, value: impl Hash) -> u64 {
        let mut hasher = hasher.clone();
        value.hash(&mut hasher);
        hasher.finish()
    }
    assert_eq!(hash(&hasher, Name(s.into())), hash(&hasher, s));
}

/// Primitive String type.
#[derive(Clone, PartialEq, Eq, Hash, DataSize)]
pub struct PdfString {
    pub data: IBytes,
}
impl fmt::Debug for PdfString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "\"")?;
        for &b in self.data.as_slice() {
            match b {
                b'"' => write!(f, "\\\"")?,
                b' ' ..= b'~' => write!(f, "{}", b as char)?,
                o @ 0 ..= 7  => write!(f, "\\{}", o)?,
                x => write!(f, "\\x{:02x}", x)?
            }
        }
        write!(f, "\"")
    }
}
impl Object for PdfString {
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::String (string) => Ok(string),
            Primitive::Reference(id) => PdfString::from_primitive(r.resolve(id)?, &NoResolve),
            _ => unexpected_primitive!(String, p.get_debug_name()),
        }
    }
}
impl ObjectWrite for PdfString {
    fn to_primitive(&self, _update: &mut impl Updater) -> Result<Primitive> {
        Ok(Primitive::String(self.clone()))
    }
}

impl PdfString {
    pub fn serialize(&self, out: &mut impl io::Write) -> Result<()> {
        if self.data.iter().any(|&b| b >= 0x80) {
            write!(out, "<")?;
            for &b in self.data.as_slice() {
                write!(out, "{:02x}", b)?;
            }
            write!(out, ">")?;
        } else {
            write!(out, r"(")?;
            for &b in self.data.as_slice() {
                match b {
                    b'\\' | b'(' | b')' => write!(out, r"\")?,
                    _ => ()
                }
                out.write_all(&[b])?;
            }
            write!(out, r")")?;
        }
        Ok(())
    }
}
impl AsRef<[u8]> for PdfString {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl PdfString {
    pub fn new(data: IBytes) -> PdfString {
        PdfString {
            data
        }
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
    pub fn into_bytes(self) -> IBytes {
        self.data
    }
    /// without encoding information the PdfString cannot be decoded into a String
    /// therefore only lossy decoding is possible replacing unknown characters.
    /// For decoding correctly see
    /// pdf_tools/src/lib.rs
    pub fn to_string_lossy(&self) -> String {
        if self.data.starts_with(&[0xfe, 0xff]) {
            crate::font::utf16be_to_string_lossy(&self.data[2..])
        }
        else {
            String::from_utf8_lossy(&self.data).into()
        }
    }
    /// without encoding information the PdfString cannot be sensibly decoded into a String
    /// converts to a Rust String but only works for valid UTF-8, UTF-16BE and ASCII characters
    /// if invalid bytes found an Error is returned
    pub fn to_string(&self) -> Result<String> {
        if self.data.starts_with(&[0xfe, 0xff]) {
            Ok(String::from(std::str::from_utf8(crate::font::utf16be_to_string(&self.data[2..])?.as_bytes())
                .map_err(|_| PdfError::Utf8Decode)?))
        }
        else {
            Ok(String::from(std::str::from_utf8(&self.data)
                .map_err(|_| PdfError::Utf8Decode)?))
        }
    }
}
impl<'a> From<&'a str> for PdfString {
    fn from(value: &'a str) -> Self {
        PdfString { data: value.into() }
    }
}

// TODO:
// Noticed some inconsistency here.. I think to_* and as_* should not take Resolve, and not accept
// Reference. Only from_primitive() for the respective type resolves References.
impl Primitive {
    /// For debugging / error messages: get the name of the variant
    pub fn get_debug_name(&self) -> &'static str {
        match *self {
            Primitive::Null => "Null",
            Primitive::Integer (..) => "Integer",
            Primitive::Number (..) => "Number",
            Primitive::Boolean (..) => "Boolean",
            Primitive::String (..) => "String",
            Primitive::Stream (..) => "Stream",
            Primitive::Dictionary (..) => "Dictionary",
            Primitive::Array (..) => "Array",
            Primitive::Reference (..) => "Reference",
            Primitive::Name (..) => "Name",
        }
    }
    /// resolve the primitive if it is a refernce, otherwise do nothing
    pub fn resolve(self, r: &impl Resolve) -> Result<Primitive> {
        match self {
            Primitive::Reference(id) => r.resolve(id),
            _ => Ok(self)
        }
    }
    pub fn as_integer(&self) -> Result<i32> {
        match *self {
            Primitive::Integer(n) => Ok(n),
            ref p => unexpected_primitive!(Integer, p.get_debug_name())
        }
    }
    pub fn as_u8(&self) -> Result<u8> {
        match *self {
            Primitive::Integer(n) if (0..256).contains(&n) => Ok(n as u8),
            Primitive::Integer(_) => bail!("invalid integer"),
            ref p => unexpected_primitive!(Integer, p.get_debug_name())
        }
    }
    pub fn as_u32(&self) -> Result<u32> {
        match *self {
            Primitive::Integer(n) if n >= 0 => Ok(n as u32),
            Primitive::Integer(_) => bail!("negative integer"),
            ref p => unexpected_primitive!(Integer, p.get_debug_name())
        }
    }
    pub fn as_usize(&self) -> Result<usize> {
        match *self {
            Primitive::Integer(n) if n >= 0 => Ok(n as usize),
            Primitive::Integer(_) => bail!("negative integer"),
            ref p => unexpected_primitive!(Integer, p.get_debug_name())
        }
    }
    pub fn as_number(&self) -> Result<f32> {
        match *self {
            Primitive::Integer(n) => Ok(n as f32),
            Primitive::Number(f) => Ok(f),
            ref p => unexpected_primitive!(Number, p.get_debug_name())
        }
    }
    pub fn as_bool(&self) -> Result<bool> {
        match *self {
            Primitive::Boolean (b) => Ok(b),
            ref p => unexpected_primitive!(Number, p.get_debug_name())
        }
    }
    pub fn as_name(&self) -> Result<&str> {
        match self {
            Primitive::Name(ref name) => Ok(name.as_str()),
            p => unexpected_primitive!(Name, p.get_debug_name())
        }
    }
    pub fn as_string(&self) -> Result<&PdfString> {
        match self {
            Primitive::String(ref data) => Ok(data),
            p => unexpected_primitive!(String, p.get_debug_name())
        }
    }
    pub fn as_array(&self) -> Result<&[Primitive]> {
        match self {
            Primitive::Array(ref v) => Ok(v),
            p => unexpected_primitive!(Array, p.get_debug_name())
        }
    }
    pub fn into_reference(self) -> Result<PlainRef> {
        match self {
            Primitive::Reference(id) => Ok(id),
            p => unexpected_primitive!(Reference, p.get_debug_name())
        }
    }
    pub fn into_array(self) -> Result<Vec<Primitive>> {
        match self {
            Primitive::Array(v) => Ok(v),
            p => unexpected_primitive!(Array, p.get_debug_name())
        }
    }
    pub fn into_dictionary(self) -> Result<Dictionary> {
        match self {
            Primitive::Dictionary(dict) => Ok(dict),
            p => unexpected_primitive!(Dictionary, p.get_debug_name())
        }
    }
    pub fn into_name(self) -> Result<Name> {
        match self {
            Primitive::Name(name) => Ok(Name(name)),
            p => unexpected_primitive!(Name, p.get_debug_name())
        }
    }
    pub fn into_string(self) -> Result<PdfString> {
        match self {
            Primitive::String(data) => Ok(data),
            p => unexpected_primitive!(String, p.get_debug_name())
        }
    }
    pub fn to_string_lossy(&self) -> Result<String> {
        let s = self.as_string()?;
        Ok(s.to_string_lossy())
    }
    pub fn to_string(&self) -> Result<String> {
        let s = self.as_string()?;
        s.to_string()
    }
    pub fn into_stream(self, _r: &impl Resolve) -> Result<PdfStream> {
        match self {
            Primitive::Stream (s) => Ok(s),
            p => unexpected_primitive!(Stream, p.get_debug_name())
        }
    }
}

impl From<i32> for Primitive {
    fn from(x: i32) -> Primitive {
        Primitive::Integer(x)
    }
}
impl From<f32> for Primitive {
    fn from(x: f32) -> Primitive {
        Primitive::Number(x)
    }
}
impl From<bool> for Primitive {
    fn from(x: bool) -> Primitive {
        Primitive::Boolean(x)
    }
}
impl From<Name> for Primitive {
    fn from(Name(s): Name) -> Primitive {
        Primitive::Name(s)
    }
}
impl From<PdfString> for Primitive {
    fn from(x: PdfString) -> Primitive {
        Primitive::String (x)
    }
}
impl From<PdfStream> for Primitive {
    fn from(x: PdfStream) -> Primitive {
        Primitive::Stream (x)
    }
}
impl From<Dictionary> for Primitive {
    fn from(x: Dictionary) -> Primitive {
        Primitive::Dictionary (x)
    }
}
impl From<Vec<Primitive>> for Primitive {
    fn from(x: Vec<Primitive>) -> Primitive {
        Primitive::Array (x)
    }
}

impl From<PlainRef> for Primitive {
    fn from(x: PlainRef) -> Primitive {
        Primitive::Reference (x)
    }
}
impl<'a> TryInto<f32> for &'a Primitive {
    type Error = PdfError;
    fn try_into(self) -> Result<f32> {
        self.as_number()
    }
}
impl<'a> TryInto<i32> for &'a Primitive {
    type Error = PdfError;
    fn try_into(self) -> Result<i32> {
        self.as_integer()
    }
}
impl<'a> TryInto<Name> for &'a Primitive {
    type Error = PdfError;
    fn try_into(self) -> Result<Name> {
        match self {
            Primitive::Name(s) => Ok(Name(s.clone())),
            p => Err(PdfError::UnexpectedPrimitive {
                expected: "Name",
                found: p.get_debug_name()
            })
        }
    }
}
impl<'a> TryInto<&'a [Primitive]> for &'a Primitive {
    type Error = PdfError;
    fn try_into(self) -> Result<&'a [Primitive]> {
        self.as_array()
    }
}
impl<'a> TryInto<&'a [u8]> for &'a Primitive {
    type Error = PdfError;
    fn try_into(self) -> Result<&'a [u8]> {
        match *self {
            Primitive::Name(ref s) => Ok(s.as_bytes()),
            Primitive::String(ref s) => Ok(s.as_bytes()),
            ref p => Err(PdfError::UnexpectedPrimitive {
                expected: "Name or String",
                found: p.get_debug_name()
            })
        }
    }
}
impl<'a> TryInto<Cow<'a, str>> for &'a Primitive {
    type Error = PdfError;
    fn try_into(self) -> Result<Cow<'a, str>> {
        match *self {
            Primitive::Name(ref s) => Ok(Cow::Borrowed(s)),
            Primitive::String(ref s) => Ok(Cow::Owned(s.to_string_lossy())),
            ref p => Err(PdfError::UnexpectedPrimitive {
                expected: "Name or String",
                found: p.get_debug_name()
            })
        }
    }
}
impl<'a> TryInto<String> for &'a Primitive {
    type Error = PdfError;
    fn try_into(self) -> Result<String> {
        match *self {
            Primitive::Name(ref s) => Ok(s.as_str().into()),
            Primitive::String(ref s) => Ok(s.to_string_lossy()),
            ref p => Err(PdfError::UnexpectedPrimitive {
                expected: "Name or String",
                found: p.get_debug_name()
            })
        }
    }
}

fn parse_or<T: str::FromStr + Clone>(buffer: &str, range: Range<usize>, default: T) -> T {
    buffer.get(range)
        .map(|s| str::parse::<T>(s).unwrap_or_else(|_| default.clone()))
        .unwrap_or(default)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub rel: TimeRel,
    pub tz_hour: u8,
    pub tz_minute: u8,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum TimeRel {
    Earlier,
    Later,
    Universal
}
datasize::non_dynamic_const_heap_size!(Date, std::mem::size_of::<Date>());

impl Object for Date {
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        match p.resolve(r)? {
            Primitive::String (PdfString {data}) => {
                let s = str::from_utf8(&data)?;
                if s.starts_with("D:") {
                    let year = match s.get(2..6) {
                        Some(year) => {
                            str::parse::<u16>(year)?
                        }
                        None => bail!("Missing obligatory year in date")
                    };
                    
                    let (time, rel, zone) = match s.find(['+', '-', 'Z']) {
                        Some(p) => {
                            let rel = match &s[p..p+1] {
                                "-" => TimeRel::Earlier,
                                "+" => TimeRel::Later,
                                "Z" => TimeRel::Universal,
                                _ => unreachable!()
                            };
                            (&s[..p], rel, &s[p+1..])
                        }
                        None => (s, TimeRel::Universal, "")
                    };

                    let month = parse_or(time, 6..8, 1);
                    let day = parse_or(time, 8..10, 1);
                    let hour = parse_or(time, 10..12, 0);
                    let minute = parse_or(time, 12..14, 0);
                    let second = parse_or(time, 14..16, 0);
                    let tz_hour = parse_or(zone, 0..2, 0);
                    let tz_minute = parse_or(zone, 3..5, 0);
                    
                    Ok(Date {
                        year, month, day,
                        hour, minute, second,
                        tz_hour, tz_minute,
                        rel
                    })
                } else {
                    bail!("Failed parsing date");
                }
            }
            p => unexpected_primitive!(String, p.get_debug_name()),
        }
    }
}

impl ObjectWrite for Date {
    fn to_primitive(&self, _update: &mut impl Updater) -> Result<Primitive> {
        let Date {
            year, month, day,
            hour, minute, second,
            tz_hour, tz_minute, rel,
        } = *self;
        if year > 9999 || day > 99 || hour > 23 || minute >= 60 || second >= 60 || tz_hour >= 24 || tz_minute >= 60 {
            bail!("not a valid date");
        }
        let o = match rel {
            TimeRel::Earlier => "-",
            TimeRel::Later => "+",
            TimeRel::Universal => "Z"
        };
        
        let s = format!("D:{year:04}{month:02}{day:02}{hour:02}{minute:02}{second:02}{o}{tz_hour:02}'{tz_minute:02}");
        Ok(Primitive::String(PdfString { data: s.into() }))
    }
}

#[cfg(test)]
mod tests {
    use crate::{primitive::{PdfString, TimeRel}, object::{NoResolve, Object}};

    use super::Date;
    #[test]
    fn utf16be_string() {
        let s = PdfString::new([0xfe, 0xff, 0x20, 0x09].as_slice().into());
        assert_eq!(s.to_string_lossy(), "\u{2009}");
    }

    #[test]
    fn utf16be_invalid_string() {
        let s = PdfString::new([0xfe, 0xff, 0xd8, 0x34].as_slice().into());
        let repl_ch = String::from(std::char::REPLACEMENT_CHARACTER);
        assert_eq!(s.to_string_lossy(), repl_ch);
    }

    #[test]
    fn utf16be_invalid_bytelen() {
        let s = PdfString::new([0xfe, 0xff, 0xd8, 0x34, 0x20].as_slice().into());
        let repl_ch = String::from(std::char::REPLACEMENT_CHARACTER);
        assert_eq!(s.to_string_lossy(), repl_ch);
    }

    #[test]
    fn pdfstring_lossy_vs_ascii() {
        // verify UTF-16-BE fails on invalid
        let s = PdfString::new([0xfe, 0xff, 0xd8, 0x34].as_slice().into());
        assert!(s.to_string().is_err()); // FIXME verify it is a PdfError::Utf16Decode
        // verify UTF-16-BE supports umlauts
        let s = PdfString::new([0xfe, 0xff, 0x00, 0xe4 /*ä*/].as_slice().into());
        assert_eq!(s.to_string_lossy(), "ä");
        assert_eq!(s.to_string().unwrap(), "ä");
        // verify valid UTF-8 bytestream with umlaut works
        let s = PdfString::new([b'm', b'i', b't', 0xc3, 0xa4 /*ä*/].as_slice().into());
        assert_eq!(s.to_string_lossy(), "mitä");
        assert_eq!(s.to_string().unwrap(), "mitä");
        // verify valid ISO-8859-1 bytestream with umlaut fails
        let s = PdfString::new([b'm', b'i', b't', 0xe4/*ä in latin1*/].as_slice().into());
        let repl_ch = ['m', 'i', 't', std::char::REPLACEMENT_CHARACTER].iter().collect::<String>();
        assert_eq!(s.to_string_lossy(), repl_ch);
        assert!(s.to_string().is_err()); // FIXME verify it is a PdfError::Utf16Decode
    }

    #[test]
    fn date() {
        let p = PdfString::from("D:199812231952-08'00");
        let d = Date::from_primitive(p.into(), &NoResolve);
        
        let d2 = Date {
            year: 1998,
            month: 12,
            day: 23,
            hour: 19,
            minute: 52,
            second: 00,
            rel: TimeRel::Earlier,
            tz_hour: 8,
            tz_minute: 0
        };
        assert_eq!(d.unwrap(), d2);
    }
}
