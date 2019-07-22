//! `pdf_derive` provides a proc macro to derive the Object trait from the `pdf` crate.
//! # Usage
//! There are several ways to derive Object on a struct or enum:
//! ## 1. Struct from PDF Dictionary
//!
//! A lot of dictionary types defined in the PDF 1.7 reference have a finite amount of possible
//! fields. Each of these are usually either required or optional. The latter is achieved by using
//! a `Option<T>` or `Vec<T>` as type of a field.
//!
//! Usually, dictionary types
//! require that the entry `/Type` is some specific string. By default, `pdf_derive` assumes that
//! this should equal the name of the input struct. This can be overridden by setting the `Type`
//! attribute equal to either the expected value of the `/Type` entry, or to `false` in order to
//! omit the type check completly.
//!
//! Check similar to that of `/Type` can also be specified in the same manner. (but the `Type`
//! attribute is special because it accepts a bool).
//!
//! Examples:
//!
//! ```
//! #[derive(Object)]
//! #[pdf(Type="XObject", Subtype="Image")]
//! /// A variant of XObject
//! pub struct ImageDictionary {
//!     #[pdf(key="Width")]
//!     width: i32,
//!     #[pdf(key="Height")]
//!     height: i32,
//!     // [...]
//! }
//! ```
//!
//! This enforces that the dictionary's `/Type` entry is present and equals `/XObject`, and that the
//! `/Subtype` entry is present and equals `/Image`.
//!
//! Each field in the struct needs to implement `Object`. Implementation is provided already for
//! common types like i32, f32, usize, bool, String (from Primitive::Name), Option<T> and Vec<T>.
//! The two latter are initialized to default if the entry isn't found in the input dictionary.
//! Option<T> is therefore frequently used for fields that are optional according to the PDF
//! reference. Vec<T> can also be used for optional fields that can also be arrays (there are quite
//! a few of those in the PDF specs - one or many). However, as stated, it accepts absense of the
//! entry, so **required** fields of type array aren't yet facilitated for.
//!
//! Lastly, for each field, it's possible to define a default value by setting the `default`
//! attribute to a string that can parse as Rust code.
//!
//! Example:
//!
//! ```
//! #[derive(Object)]
//! #[pdf(Type = "XRef")]
//! pub struct XRefInfo {
//!     #[pdf(key = "Filter")]
//!     filter: Vec<StreamFilter>,
//!     #[pdf(key = "Size")]
//!     pub size: i32,
//!     #[pdf(key = "Index", default = "vec![0, size]")]
//!     pub index: Vec<i32>,
//!     // [...]
//! }
//! ```
//!
//!
//! ## 2. Struct from PDF Stream
//! PDF Streams consist of a stream dictionary along with the stream itself. It is assumed that all
//! structs that want to derive Object where the primitive it  converts from is a stream,
//! have a field `info: T`, where `T: Object`, and a field `data: Vec<u8>`.
//!
//! Deriving an Object that converts from Primitive::Stream, the flag `is_stream` is required in
//! the proc macro attributes.
//!
//! ## 3. Enum from PDF Name
//! Example:
//!
//! ```
//! #[derive(Object, Debug)]
//! pub enum StreamFilter {
//!     ASCIIHexDecode,
//!     ASCII85Decode,
//!     LZWDecode,
//!     FlateDecode,
//!     JPXDecode,
//!     DCTDecode,
//! }
//! ```
//!
//! In this case, `StreamFilter::from_primitive(primitive)` will return Ok(_) only if the primitive
//! is `Primitive::Name` and matches one of the enum variants
#![recursion_limit="128"]

extern crate proc_macro;
extern crate syn;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream;
use syn::*;

// Debugging:
/*
use std::fs::{OpenOptions};
use std::io::Write;
*/






#[proc_macro_derive(Object, attributes(pdf))]
pub fn object(input: TokenStream) -> TokenStream {
    // Construct a string representation of the type definition
    let s = input.to_string();
    
    // Parse the string representation
    let ast = syn::parse_derive_input(&s).unwrap();

    // Build the impl
    let gen = impl_object(&ast);
    
    // Debugging
    /*
    let mut file = OpenOptions::new()
        .write(true)
        .append(true)
        .open("/tmp/proj/src/main.rs")
        .unwrap();
    write!(file, "{}", gen).unwrap();
    */
    // Return the generated impl
    gen.parse().unwrap()
}


/// Returns (key, default, skip)
fn field_attrs(field: &Field) -> (String, Option<String>, bool) {
    field.attrs.iter()
    .filter_map(|attr| match attr.value {
        MetaItem::List(ref ident, ref list) if ident == "pdf" => {
            let (mut key, mut default, mut skip) = (None, None, false);
            for meta in list {
                match *meta {
                    NestedMetaItem::MetaItem(MetaItem::NameValue(ref ident, Lit::Str(ref value, _))) 
                    if ident == "key"
                        => key = Some(value.clone()),
                    NestedMetaItem::MetaItem(MetaItem::NameValue(ref ident, Lit::Str(ref value, _)))
                    if ident == "default"
                        => default = Some(value.clone()),
                    NestedMetaItem::MetaItem(MetaItem::Word(ref ident))
                    if ident == "skip"
                        => skip = true,
                    _ => panic!(r##"Derive error - Supported derive attributes: `key="Key"`, `default="some code"`."##)
                }
            }
            let key = match skip {
                true => String::from(""),
                false => key.expect("attr `key` missing"),
            };
            Some(( key, default, skip))
        },
        _ => None
    }).next().expect("no pdf meta attribute")
}




/// Just the attributes for the whole struct
#[derive(Default)]
struct GlobalAttrs {
    /// List of checks to do in the dictionary (LHS is the key, RHS is the expected value)
    checks: Vec<(String, String)>,
    type_name: Option<String>,
    type_required: bool,
    is_stream: bool,
}
impl GlobalAttrs {
    /// The PDF type may be explicitly specified as an attribute with type "Type". Else, it is the name
    /// of the struct.
    fn from_ast(ast: &DeriveInput) -> GlobalAttrs {
        let mut attrs = GlobalAttrs::default();

        for attr in &ast.attrs {
            match attr.value {
                MetaItem::List(ref ident, ref list) if ident == "pdf" => {
                    // Loop through list of attributes
                    for meta in list {
                        match *meta {
                            NestedMetaItem::MetaItem(MetaItem::NameValue(ref ident, ref value))
                                => if ident == "Type" {
                                    match *value {
                                        Lit::Str(ref value, _) => {
                                            if value.ends_with("?") {
                                                attrs.type_name = Some(value[.. value.len()-1].to_string());
                                                attrs.type_required = false;
                                            } else {
                                                attrs.type_name = Some(value.clone());
                                                attrs.type_required = true;
                                            }
                                        },
                                        _ => panic!("Value of 'Type' attribute must be a String."),
                                    }
                                } else {
                                    match *value {
                                        Lit::Str(ref value, _) => attrs.checks.push((String::from(ident.as_ref()), value.clone())),
                                        _ => panic!("Other checks must have RHS String."),
                                    }
                                },

                            NestedMetaItem::MetaItem(MetaItem::Word(ref ident))
                            if ident == "is_stream" => attrs.is_stream = true,
                            _ => {}
                        }
                    }
                },
                _ => {}
            }
        }

        attrs
    }
}

fn impl_object(ast: &DeriveInput) -> quote::Tokens {
    let attrs = GlobalAttrs::from_ast(&ast);
    if attrs.is_stream {
        match ast.body {
            Body::Struct(ref data) => impl_object_for_stream(ast, data.fields()),
            Body::Enum(_) => panic!("Enum can't be a PDF stream"),
        }
    } else {
        match ast.body {
            Body::Struct(ref data) => impl_object_for_struct(ast, data.fields()),
            Body::Enum(ref variants) => impl_object_for_enum(ast, variants),
        }
    }
    
    
}
/// Accepts Name to construct enum
fn impl_object_for_enum(ast: &DeriveInput, variants: &Vec<Variant>) -> quote::Tokens {
    let id = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let ser_code: Vec<_> = variants.iter().map(|var| {
        quote! {
            #id::#var => stringify!(#id::#var),
        }
    }).collect();

    let from_primitive_code = impl_from_name(ast, variants);
    quote! {
        impl #impl_generics ::pdf::object::Object for #id #ty_generics #where_clause {
            fn serialize<W: ::std::io::Write>(&self, out: &mut W) -> ::std::io::Result<()> {
                writeln!(out, "/{}",
                    match *self {
                        #( #ser_code )*
                    }
                )
            }
            fn from_primitive(p: Primitive, _resolve: &Resolve) -> ::pdf::Result<Self> {
                #from_primitive_code
            }
        }
    }
}

/// Returns code for from_primitive that accepts Name
fn impl_from_name(ast: &syn::DeriveInput, variants: &Vec<Variant>) -> quote::Tokens {
    let id = &ast.ident;
    let parts: Vec<quote::Tokens> = variants.iter().map(|var| {
        quote! {
            stringify!(#var) => #id::#var,
        }
    }).collect();
    quote! {
        Ok(
        match p {
            Primitive::Name (name) => {
                match name.as_str() {
                    #( #parts )*
                    s => bail!(format!("Enum {} from_primitive: no variant {}.", stringify!(#id), s)),
                }
            }
            _ => bail!(::pdf::Error::from(::pdf::ErrorKind::UnexpectedPrimitive { expected: "Name", found: p.get_debug_name() })),
        }
        )
    }

}

/// Accepts Dictionary to construct a struct
fn impl_object_for_struct(ast: &DeriveInput, fields: &[Field]) -> quote::Tokens {
    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
    let attrs = GlobalAttrs::from_ast(&ast);

    let parts: Vec<_> = fields.iter()
    .map(|field| {
        let (key, default, skip) = field_attrs(field);
        (field.ident.clone(), key, default, skip)
    }).collect();
    
    // Implement serialize()
    let fields_ser = parts.iter()
    .map( |&(ref field, ref key, ref _default, skip)|
        if skip {
            quote! {}
        } else {
            quote! {
                write!(out, "{} ", #key)?;
                self.#field.serialize(out)?;
                writeln!(out, "")?;
            }
        }
    );
    let checks_code = attrs.checks.iter().map(|&(ref key, ref val)|
        quote! {
            writeln!(out, "/{} /{}", #key, #val)?;
        }
    );

    // Implement from_primitive()
    let from_primitive_code =  impl_from_dict(ast, fields);
    let pdf_type = match attrs.type_name {
        Some(ref ty) => quote! { writeln!(out, "/Type /{}", #ty)?; },
        None => quote! {}
    };
    
    quote! {
        impl #impl_generics ::pdf::object::Object for #name #ty_generics #where_clause {
            fn serialize<W: ::std::io::Write>(&self, out: &mut W) -> ::std::io::Result<()> {
                writeln!(out, "<<")?;
                #pdf_type
                #( #checks_code )*
                #(#fields_ser)*
                writeln!(out, ">>")?;
                Ok(())
            }
            fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<Self> {
                #from_primitive_code
            }
        }
    }
}

/// Note: must have info and dict (TODO explain in docs)
fn impl_object_for_stream(ast: &DeriveInput, fields: &[Field]) -> quote::Tokens {
    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let info_ty = fields.iter()
    .filter_map(|field| {
        if let Some(ident) = field.ident.as_ref() {
            if ident.as_ref() == "info" {
                Some(field.ty.clone())
            } else {
                None
            }
        } else {
            None
        }
    }).next().unwrap();

    quote! {
        impl #impl_generics ::pdf::object::Object for #name #ty_generics #where_clause {
            fn serialize<W: ::std::io::Write>(&self, _out: &mut W) -> ::std::io::Result<()> {
                unimplemented!();
                /*
                writeln!(out, "<<")?;
                #type_code
                #(#fields_ser)*
                writeln!(out, ">>")?;
                Ok(())
                */
            }
            fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<Self> {
                let ::pdf::primitive::PdfStream {info, data}
                    = p.to_stream(resolve).chain_err(|| stringify!(#name))?;

                Ok(#name {
                    info: <#info_ty as Object>::from_primitive(::pdf::primitive::Primitive::Dictionary (info), resolve)?,
                    data: data,
                })
            }
        }
    }
}

/// Returns (let assignments, field assignments)
/// Example:
/// (`let name = ...;`,
///  `    name: name`)
/// 
fn impl_parts(fields: &[Field]) -> (Vec<quote::Tokens>, Vec<quote::Tokens>) {
    (fields.iter().map(|field| {
        let (key, default, skip) = field_attrs(field);
        if skip {
            return quote! {}; // skip this field..
        }
        let ref name = field.ident;

        let ty = field.ty.clone();


        if let Some(ref default) = default {
            let default = syn::parse_token_trees(&default).expect("Could not parse `default` code as Rust.");
            quote! {
                let #name = {
                    let primitive: Option<::pdf::primitive::Primitive>
                        = dict.remove(#key);
                    let x: #ty = match primitive {
                        Some(primitive) => <#ty as Object>::from_primitive(primitive, resolve).chain_err( || stringify!(#name) )?,
                        None => #( #default )*,
                    };
                    x
                };
            }
        } else {
            quote! {
                let #name = {
                    match dict.remove(#key) {
                        Some(primitive) =>
                            match <#ty as Object>::from_primitive(primitive, resolve) {
                                Ok(obj) => obj,
                                Err(e) => bail!(e.chain_err(|| format!("Key {}: cannot convert from primitive to type {}", #key, stringify!(#ty)))),
                            }
                        None =>  // Try to construct T from Primitive::Null
                            match <#ty as Object>::from_primitive(::pdf::primitive::Primitive::Null, resolve) {
                                Ok(obj) => obj,
                                Err(_) => bail!("Object {}, Key {} not found", stringify!(#name), #key),
                            },
                    }
                    // ^ By using Primitive::Null when we don't find the key, we allow 'optional'
                    // types like Option and Vec to be constructed from non-existing values
                };
            }
        }
    }).collect(),
    fields.iter().map(|field| {
        let ref name = field.ident;
        quote! { #name: #name, }
    }).collect())
}


/// Returns code for from_primitive that accepts Dictionary
fn impl_from_dict(ast: &DeriveInput, fields: &[Field]) -> quote::Tokens {
    let name = &ast.ident;
    let attrs = GlobalAttrs::from_ast(&ast);
    
    
    
    let (let_parts, field_parts) = impl_parts(&fields);

    let checks: Vec<_> = attrs.checks.iter().map(|&(ref key, ref val)|
        quote! {
            let ty = dict.remove(#key)
                .ok_or(::pdf::Error::from(::pdf::ErrorKind::EntryNotFound { key: #key }))?
                .to_name()?;
            if ty != #val {
                bail!("[Dict entry /{}] != /{}", #key, #val);
            }
        }
    ).collect();

    let ty_check = match (attrs.type_name, attrs.type_required) {
        (Some(ty), true) => quote! {
            let ty = dict.remove("Type")
                .ok_or(::pdf::Error::from(::pdf::ErrorKind::EntryNotFound { key: "Type" }))?
                .to_name()?;
            if ty != #ty {
                bail!("[Dict entry /{}] != /{}", "Type", #ty);
            }
        },
        (Some(ty), false) => quote! {
            match dict.remove("Type") {
                Some(ty) => if ty.to_name()? != #ty {
                    bail!("[Dict entry /{}] != /{}", "Type", #ty);
                },
                None => {}
            }
        },
        (None, _) => quote!{}
    };
        
    
    quote! {
        let mut dict = Dictionary::from_primitive(p, resolve)?;
        #ty_check
        #( #checks )*
        #( #let_parts )*
        Ok(#name {
            #( #field_parts )*
        })
    }
}
