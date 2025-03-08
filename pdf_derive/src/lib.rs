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
//! ```ignore
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
//! ```ignore
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
//! ```ignore
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
#![recursion_limit = "128"]

extern crate proc_macro;
extern crate syn;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use syn::*;
type SynStream = TokenStream2;

// Debugging:
/*
use std::fs::{OpenOptions};
use std::io::Write;
*/

#[proc_macro_derive(Object, attributes(pdf))]
pub fn object(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    // Build the impl
    impl_object(&ast)
}

#[proc_macro_derive(ObjectWrite, attributes(pdf))]
pub fn objectwrite(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    // Build the impl
    impl_objectwrite(&ast)
}

#[proc_macro_derive(DeepClone, attributes(pdf))]
pub fn deepclone(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    // Build the impl
    impl_deepclone(&ast)
}

#[derive(Default)]
struct FieldAttrs {
    key: Option<LitStr>,
    default: Option<LitStr>,
    name: Option<LitStr>,
    skip: bool,
    other: bool,
    indirect: bool,
}
impl FieldAttrs {
    fn new() -> FieldAttrs {
        FieldAttrs {
            key: None,
            default: None,
            name: None,
            skip: false,
            other: false,
            indirect: false,
        }
    }
    fn key(&self) -> &LitStr {
        self.key.as_ref().expect("no 'key' in field attributes")
    }
    fn default(&self) -> Option<Expr> {
        self.default
            .as_ref()
            .map(|s| parse_str(&s.value()).expect("can't parse `default` as EXPR"))
    }
    fn parse(list: &[Attribute]) -> FieldAttrs {
        let mut attrs = FieldAttrs::new();
        for attr in list.iter().filter(|attr| attr.path().is_ident("pdf")) {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("key") {
                    let value = meta.value()?;
                    attrs.key = Some(value.parse()?);
                    return Ok(());
                }

                if meta.path.is_ident("default") {
                    let value = meta.value()?;
                    attrs.default = Some(value.parse()?);
                    return Ok(());
                }

                if meta.path.is_ident("name") {
                    let value = meta.value()?;
                    attrs.name = Some(value.parse()?);
                    return Ok(());
                }

                if meta.path.is_ident("skip") {
                    attrs.skip = true;
                    return Ok(());
                }

                if meta.path.is_ident("other") {
                    attrs.other = true;
                    return Ok(());
                }

                if meta.path.is_ident("indirect") {
                    attrs.indirect = true;
                    return Ok(());
                }

                Err(meta.error("unsupported key"))
            })
            .expect("parse error");
        }
        attrs
    }
}

/// Just the attributes for the whole struct
#[derive(Default, Debug)]
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

        for attr in ast.attrs.iter().filter(|attr| attr.path().is_ident("pdf")) {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("Type") {
                    let value = meta.value()?;
                    let lit = value.parse()?;
                    match lit {
                        Lit::Str(ref value) => {
                            let mut value = value.value();
                            attrs.type_required = if value.ends_with('?') {
                                value.pop(); // remove '?'
                                false
                            } else {
                                true
                            };
                            attrs.type_name = Some(value);
                        }
                        _ => panic!("Value of 'Type' attribute must be a String."),
                    };
                    return Ok(());
                }

                if meta.path.is_ident("is_stream") {
                    attrs.is_stream = true;
                    return Ok(());
                }

                if let Ok(value) = meta.value() {
                    let path = &meta.path;
                    let lit = value.parse()?;
                    match lit {
                        Lit::Str(ref value) => {
                            let segments = path
                                .segments
                                .iter()
                                .map(|s| s.ident.to_string())
                                .collect::<Vec<String>>()
                                .join("::");
                            attrs.checks.push((segments, value.value()));
                        }
                        _ => panic!("Other checks must have RHS String."),
                    };
                    return Ok(());
                }

                Ok(())
            })
            .expect("error with global attrs parsing");
        }

        attrs
    }
}

fn impl_object(ast: &DeriveInput) -> TokenStream {
    let attrs = GlobalAttrs::from_ast(ast);
    match (attrs.is_stream, &ast.data) {
        (true, Data::Struct(ref data)) => impl_object_for_stream(ast, &data.fields).into(),
        (false, Data::Struct(ref data)) => impl_object_for_struct(ast, &data.fields).into(),
        (true, Data::Enum(ref variants)) => impl_enum_from_stream(ast, variants, &attrs).into(),
        (false, Data::Enum(ref variants)) => impl_object_for_enum(ast, variants).into(),
        (_, _) => unimplemented!(),
    }
}
fn impl_objectwrite(ast: &DeriveInput) -> TokenStream {
    let attrs = GlobalAttrs::from_ast(ast);
    match (attrs.is_stream, &ast.data) {
        (false, Data::Struct(ref data)) => impl_objectwrite_for_struct(ast, &data.fields).into(),
        (false, Data::Enum(ref variants)) => impl_objectwrite_for_enum(ast, variants).into(),
        (_, _) => unimplemented!(),
    }
}
fn impl_deepclone(ast: &DeriveInput) -> TokenStream {
    let _attrs = GlobalAttrs::from_ast(ast);
    match &ast.data {
        Data::Struct(ref data) => impl_deepclone_for_struct(ast, &data.fields).into(),
        Data::Enum(ref variants) => impl_deepclone_for_enum(ast, variants).into(),
        _ => unimplemented!(),
    }
}

fn enum_pairs(
    ast: &DeriveInput,
    data: &DataEnum,
) -> (Vec<(String, TokenStream2)>, Option<TokenStream2>) {
    let id = &ast.ident;

    let mut pairs = Vec::with_capacity(data.variants.len());
    let mut other = None;

    for var in data.variants.iter() {
        let attrs = FieldAttrs::parse(&var.attrs);
        let var_ident = &var.ident;
        let name = attrs
            .name
            .map(|lit| lit.value())
            .unwrap_or_else(|| var_ident.to_string());
        if attrs.other {
            assert!(
                other.is_none(),
                "only one 'other' variant is allowed in a name enum"
            );

            match &var.fields {
                Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {}
                _ => {
                    panic!(
                        "the 'other' variant in a name enum should have exactly one unnamed field",
                    );
                }
            }
            other = Some(quote! { #id::#var_ident });
        } else {
            pairs.push((name, quote! { #id::#var_ident }));
        }
    }

    (pairs, other)
}

/// Accepts Name to construct enum
fn impl_object_for_enum(ast: &DeriveInput, data: &DataEnum) -> SynStream {
    let id = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let int_count = data
        .variants
        .iter()
        .filter(|var| var.discriminant.is_some())
        .count();
    if int_count > 0 {
        assert_eq!(
            int_count,
            data.variants.len(),
            "either none or all variants can have a descriminant"
        );

        let parts = data.variants.iter().map(|var| {
            if let Some((_, Expr::Lit(ref lit_expr))) = var.discriminant {
                let var_ident = &var.ident;
                let pat = Pat::from(lit_expr.clone());
                quote! {
                    #pat => Ok(#id::#var_ident)
                }
            } else {
                panic!()
            }
        });

        quote! {
            impl #impl_generics pdf::object::Object for #id #ty_generics #where_clause {
                fn from_primitive(p: pdf::primitive::Primitive, _resolve: &impl pdf::object::Resolve) -> pdf::error::Result<Self> {
                    match p {
                        pdf::primitive::Primitive::Integer(i) => {
                            match i {
                                #( #parts, )*
                                _ => Err(pdf::error::PdfError::UnknownVariant { id: stringify!(#id), name: i.to_string() })
                            }
                        }
                        _ => Err(pdf::error::PdfError::UnexpectedPrimitive { expected: "Integer", found: p.get_debug_name() }),
                    }
                }
            }
        }
    } else {
        let (pairs, other) = enum_pairs(ast, data);

        let mut parts: Vec<_> = pairs
            .iter()
            .map(|(name, var)| {
                quote! {
                    #name => Ok(#var)
                }
            })
            .collect();

        if let Some(other_tokens) = other {
            parts.push(quote! {
                s => Ok(#other_tokens(s.to_string()))
            });
        } else {
            parts.push(quote! {
                s => Err(pdf::error::PdfError::UnknownVariant { id: stringify!(#id), name: s.to_string() })
            });
        }

        quote! {
            impl #impl_generics pdf::object::Object for #id #ty_generics #where_clause {
                fn from_primitive(p: pdf::primitive::Primitive, _resolve: &impl pdf::object::Resolve) -> pdf::error::Result<Self> {
                    match p {
                        pdf::primitive::Primitive::Name(name) => {
                            match name.as_str() {
                                #( #parts, )*
                            }
                        }
                        _ => Err(pdf::error::PdfError::UnexpectedPrimitive { expected: "Name", found: p.get_debug_name() }),
                    }
                }
            }
        }
    }
}
/// Accepts Name to construct enum
fn impl_objectwrite_for_enum(ast: &DeriveInput, data: &DataEnum) -> SynStream {
    let id = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let int_count = data
        .variants
        .iter()
        .filter(|var| var.discriminant.is_some())
        .count();
    if int_count > 0 {
        assert_eq!(
            int_count,
            data.variants.len(),
            "either none or all variants can have a descriminant"
        );

        let parts = data.variants.iter().map(|var| {
            if let Some((_, ref expr)) = var.discriminant {
                let var_ident = &var.ident;
                quote! {
                    #id::#var_ident => Ok(Primitive::Integer(#expr))
                }
            } else {
                panic!()
            }
        });

        quote! {
            impl #impl_generics pdf::object::ObjectWrite for #id #ty_generics #where_clause {
                fn to_primitive(&self, update: &mut impl pdf::object::Updater) -> Result<Primitive> {
                    match *self {
                        #( #parts, )*
                    }
                }
            }
        }
    } else {
        let (pairs, other) = enum_pairs(ast, data);

        let mut ser_code: Vec<_> = pairs
            .iter()
            .map(|(name, var)| {
                quote! {
                    #var => #name
                }
            })
            .collect();

        if let Some(other_tokens) = other {
            ser_code.push(quote! {
                #other_tokens(ref name) => name.as_str()
            });
        }

        quote! {
            impl #impl_generics pdf::object::ObjectWrite for #id #ty_generics #where_clause {
                fn to_primitive(&self, update: &mut impl pdf::object::Updater) -> Result<Primitive> {
                    let name = match *self {
                        #( #ser_code, )*
                    };

                    Ok(Primitive::Name(name.into()))
                }
            }
        }
    }
}
fn impl_deepclone_for_enum(ast: &DeriveInput, data: &DataEnum) -> SynStream {
    let id = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let parts = data.variants.iter().map(|var| {
        let var_ident = &var.ident;
        match var.fields {
            Fields::Unnamed(ref fields) => {
                let labels: Vec<Ident> = fields.unnamed.iter().enumerate().map(|(i, _f)| {
                    Ident::new(&format!("f_{i}"), Span::mixed_site())
                }).collect();
                quote! {
                    #id::#var_ident( #( ref #labels, )* ) => Ok(#id::#var_ident( #( #labels.deep_clone(cloner)? ),* ))
                }
            }
            Fields::Named(ref fields) => {
                let names: Vec<_> = fields.named.iter().map(|f| f.ident.as_ref().unwrap()).collect();
                quote! {
                    #id::#var_ident { #( ref #names ),* } => Ok(#id::#var_ident { #( #names: #names.deep_clone(cloner)? ),* })
                }
            }
            Fields::Unit => {
                quote! {
                    #id::#var_ident => Ok(#id::#var_ident)
                }
            }
        }
    });

    quote! {
        impl #impl_generics pdf::object::DeepClone for #id #ty_generics #where_clause {
            fn deep_clone(&self, cloner: &mut impl pdf::object::Cloner) -> Result<Self> {
                match *self {
                    #( #parts, )*
                }
            }
        }
    }
}

fn impl_enum_from_stream(ast: &DeriveInput, data: &DataEnum, attrs: &GlobalAttrs) -> SynStream {
    let id = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let ty_check = match (&attrs.type_name, attrs.type_required) {
        (Some(ref ty), required) => quote! {
            stream.info.expect(stringify!(#id), "Type", #ty, #required)?;
        },
        (None, _) => quote! {},
    };

    let variants_code: Vec<_> = data.variants.iter().map(|var| {
        let attrs = FieldAttrs::parse(&var.attrs);
        let inner_ty = match var.fields {
            Fields::Unnamed(ref fields) => {
                assert_eq!(fields.unnamed.len(), 1, "all variants in a stream enum have to have exactly one unnamed field");
                fields.unnamed.first().unwrap().ty.clone()
            },
            _ => panic!("all variants in a stream enum have to have exactly one unnamed field")
        };
        let name = attrs.name.map(|lit| lit.value()).unwrap_or_else(|| var.ident.to_string());
        let variant_ident = &var.ident;
        quote! {
            #name => Ok(#id::#variant_ident ( #inner_ty::from_primitive(pdf::primitive::Primitive::Stream(stream), resolve)?))
        }
    }).collect();

    quote! {
        impl #impl_generics pdf::object::Object for #id #ty_generics #where_clause {
            fn from_primitive(p: pdf::primitive::Primitive, resolve: &impl pdf::object::Resolve) -> pdf::error::Result<Self> {
                let mut stream = PdfStream::from_primitive(p, resolve)?;
                #ty_check

                let subty = stream.info.get("Subtype")
                    .ok_or(pdf::error::PdfError::MissingEntry { typ: stringify!(#id), field: "Subtype".into()})?
                    .as_name()?;

                match subty {
                    #( #variants_code, )*
                    s => Err(pdf::error::PdfError::UnknownVariant { id: stringify!(#id), name: s.into() })
                }
            }
        }
    }
}

fn is_option(f: &Field) -> Option<Type> {
    match f.ty {
        Type::Path(ref p) => {
            let first = p.path.segments.first().unwrap();
            match first {
                PathSegment {
                    ident,
                    arguments: PathArguments::AngleBracketed(args),
                } if ident == "Option" => match args.args.first().unwrap() {
                    GenericArgument::Type(t) => Some(t.clone()),
                    _ => panic!(),
                },
                _ => None,
            }
        }
        _ => None,
    }
}

/// Accepts Dictionary to construct a struct
fn impl_object_for_struct(ast: &DeriveInput, fields: &Fields) -> SynStream {
    let id = &ast.ident;
    let mut generics = ast.generics.clone();
    for g in generics.params.iter_mut() {
        if let GenericParam::Type(p) = g {
            p.bounds.push(parse_quote!(pdf::object::Object));
        }
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let attrs = GlobalAttrs::from_ast(ast);

    ///////////////////////
    let typ = id.to_string();
    let let_parts = fields.iter().map(|field| {

        let name = &field.ident;
        let attrs = FieldAttrs::parse(&field.attrs);
        if attrs.skip {
            return quote! {}
        }
        if attrs.other {
            return quote! {
                let #name = dict;
            };
        }

        let key = attrs.key();

        let ty = field.ty.clone();
        if let Some(ref default) = attrs.default() {
            quote! {
                let #name = {
                    let primitive: Option<pdf::primitive::Primitive>
                        = dict.remove(#key);
                    let x: #ty = match primitive {
                        Some(primitive) => <#ty as pdf::object::Object>::from_primitive(primitive, resolve).map_err(|e|
                            pdf::error::PdfError::FromPrimitive {
                                typ: #typ,
                                field: stringify!(#name),
                                source: Box::new(e)
                            })?,
                        None => #default,
                    };
                    x
                };
            }
        } else {
            quote! {
                let #name = {
                    match dict.remove(#key) {
                        Some(primitive) =>
                            match <#ty as pdf::object::Object>::from_primitive(primitive, resolve) {
                                Ok(obj) => obj,
                                Err(e) => return Err(pdf::error::PdfError::FromPrimitive {
                                    typ: stringify!(#ty),
                                    field: stringify!(#name),
                                    source: Box::new(e)
                                })
                            }
                        None =>  // Try to construct T from Primitive::Null
                            match <#ty as pdf::object::Object>::from_primitive(pdf::primitive::Primitive::Null, resolve) {
                                Ok(obj) => obj,
                                Err(_) => return Err(pdf::error::PdfError::MissingEntry {
                                    typ: #typ,
                                    field: String::from(stringify!(#name)),
                                })
                            },
                    }
                    // ^ By using Primitive::Null when we don't find the key, we allow 'optional'
                    // types like Option and Vec to be constructed from non-existing values
                };
            }
        }
    });

    let field_parts = fields.iter().map(|field| {
        let name = &field.ident;
        quote! { #name: #name, }
    });

    let checks: Vec<_> = attrs
        .checks
        .iter()
        .map(|(key, val)| {
            quote! {
                dict.expect(#typ, #key, #val, true)?;
            }
        })
        .collect();

    let ty_check = match (&attrs.type_name, attrs.type_required) {
        (Some(ref ty), required) => quote! {
            dict.expect(#typ, "Type", #ty, #required)?;
        },
        (None, _) => quote! {},
    };

    quote! {
        impl #impl_generics pdf::object::FromDict for #id #ty_generics #where_clause {
            fn from_dict(mut dict: pdf::primitive::Dictionary, resolve: &impl pdf::object::Resolve) -> pdf::error::Result<Self> {
                #ty_check
                #( #checks )*
                #( #let_parts )*
                Ok(#id {
                    #( #field_parts )*
                })
            }
        }
        impl #impl_generics pdf::object::Object for #id #ty_generics #where_clause {
            fn from_primitive(p: pdf::primitive::Primitive, resolve: &impl pdf::object::Resolve) -> pdf::error::Result<Self> {
                let dict = pdf::primitive::Dictionary::from_primitive(p, resolve)?;
                <Self as pdf::object::FromDict>::from_dict(dict, resolve)
            }
        }
    }
}

fn impl_objectwrite_for_struct(ast: &DeriveInput, fields: &Fields) -> SynStream {
    let id = &ast.ident;
    let mut generics = ast.generics.clone();
    for g in generics.params.iter_mut() {
        if let GenericParam::Type(p) = g {
            p.bounds.push(parse_quote!(pdf::object::ObjectWrite));
        }
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let attrs = GlobalAttrs::from_ast(ast);

    let parts: Vec<_> = fields
        .iter()
        .map(|field| {
            (
                field.ident.clone(),
                FieldAttrs::parse(&field.attrs),
                is_option(field),
            )
        })
        .collect();

    let fields_ser = parts.iter().map(|(field, attrs, _opt)| {
        if attrs.skip | attrs.other {
            quote!()
        } else {
            let key = attrs.key();
            let tr = if attrs.indirect {
                quote! {
                    match val {
                       pdf::primitive::Primitive::Reference(r) => val,
                       p => updater.create(p)?.into(),
                    }
                }
            } else {
                quote! { val }
            };

            quote! {
                let val = pdf::object::ObjectWrite::to_primitive(&self.#field, updater)?;
                if !matches!(val, pdf::primitive::Primitive::Null) {
                    let val2 = #tr;
                    dict.insert(#key, val2);
                }
            }
        }
    });
    let checks_code = attrs.checks.iter().map(|(key, val)| {
        quote! {
            dict.insert(#key, pdf::primitive::Primitive::Name(#val.into()));
        }
    });
    let pdf_type = match attrs.type_name {
        Some(ref name) => quote! {
            dict.insert("Type", pdf::primitive::Primitive::Name(#name.into()));
        },
        None => quote! {},
    };

    let other = parts
        .iter()
        .filter(|(_field, attrs, _)| attrs.other)
        .flat_map(|(field, _, _)| field)
        .next();
    let init_dict = if let Some(other) = other {
        quote! {
            let mut dict = self.#other.clone();
        }
    } else {
        quote! {
            let mut dict = pdf::primitive::Dictionary::new();
        }
    };

    quote! {
        impl #impl_generics pdf::object::ObjectWrite for #id #ty_generics #where_clause {
            fn to_primitive(&self, update: &mut impl pdf::object::Updater) -> Result<pdf::primitive::Primitive> {
                pdf::object::ToDict::to_dict(self, update).map(pdf::primitive::Primitive::Dictionary)
            }
        }
        impl #impl_generics pdf::object::ToDict for #id #ty_generics #where_clause {
            fn to_dict(&self, updater: &mut impl pdf::object::Updater) -> Result<pdf::primitive::Dictionary> {
                #init_dict
                #pdf_type
                #( #checks_code )*
                #(#fields_ser)*
                Ok(dict)
            }
        }
    }
}
fn impl_deepclone_for_struct(ast: &DeriveInput, fields: &Fields) -> SynStream {
    let id = &ast.ident;
    let mut generics = ast.generics.clone();
    for g in generics.params.iter_mut() {
        if let GenericParam::Type(p) = g {
            p.bounds.push(parse_quote!(pdf::object::DeepClone));
        }
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let parts: Vec<_> = fields
        .iter()
        .map(|field| (field.ident.clone(), is_option(field)))
        .collect();

    let field_parts = parts.iter().map(|(field, _opt)| {
        quote! {
            #field: self.#field.deep_clone(cloner)?,
        }
    });

    quote! {
        impl #impl_generics pdf::object::DeepClone for #id #ty_generics #where_clause {
            fn deep_clone(&self, cloner: &mut impl pdf::object::Cloner) -> Result<Self> {
                Ok(#id {
                    #( #field_parts )*
                })
            }
        }
    }
}

/// Note: must have info and dict (TODO explain in docs)
fn impl_object_for_stream(ast: &DeriveInput, fields: &Fields) -> SynStream {
    let id = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let info_ty = fields
        .iter()
        .filter_map(|field| {
            if let Some(ident) = field.ident.as_ref() {
                if ident == "info" {
                    Some(field.ty.clone())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .next()
        .unwrap();

    quote! {
        impl #impl_generics pdf::object::Object for #id #ty_generics #where_clause {
            fn from_primitive(p: pdf::primitive::Primitive, resolve: &impl pdf::object::Resolve) -> pdf::error::Result<Self> {
                let pdf::primitive::PdfStream {info, data}
                    = p.to_stream(resolve)?;

                Ok(#id {
                    info: <#info_ty as pdf::object::Object>::from_primitive(pdf::primitive::Primitive::Dictionary (info), resolve)?,
                    data: data,
                })
            }
        }
    }
}
