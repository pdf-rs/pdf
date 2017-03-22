#![recursion_limit="128"]

extern crate proc_macro;
extern crate syn;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream;
use syn::*;

#[proc_macro_derive(Object, attributes(pdf))]
pub fn object(input: TokenStream) -> TokenStream {
    // Construct a string representation of the type definition
    let s = input.to_string();
    
    // Parse the string representation
    let ast = syn::parse_derive_input(&s).unwrap();

    // Build the impl
    let gen = impl_object(&ast);
    
    // Return the generated impl
    gen.parse().unwrap()
}

fn get_attrs(list: &[NestedMetaItem]) -> (String, bool) {
    let (mut key, mut opt) = (None, false);
    for meta in list {
        match *meta {
            NestedMetaItem::MetaItem(MetaItem::NameValue(ref ident, Lit::Str(ref value, _))) 
            if ident == "key" => {
                key = Some(value.clone());
            },
            NestedMetaItem::MetaItem(MetaItem::NameValue(ref ident, Lit::Bool(value))) 
            if ident == "opt" => {
                opt = value;
            }
            _ => panic!(r##"only `key="Key"` and `opt=[true|false]` are supported."##)
        }
    }
    (key.expect("attr `key` missing"), opt)
}

fn pdf_attr(field: &Field) -> (String, bool) {
    field.attrs.iter()
    .filter_map(|attr| match attr.value {
        MetaItem::List(ref ident, ref list) if ident == "pdf" => {
            Some(get_attrs(&list))
        },
        _ => None
    }).next().expect("no pdf meta attribute")
}

fn pdf_type(ast: &DeriveInput) -> String {
    ast.attrs.iter()
    .filter_map(|attr| match attr.value {
        MetaItem::List(ref ident, ref list) if ident == "pdf" => {
            list.iter().filter_map(|meta| {
                match *meta {
                    NestedMetaItem::MetaItem(
                        MetaItem::NameValue(ref ident, Lit::Str(ref value, _))) 
                    if ident == "type" =>
                        Some(value.clone()),
                    _ => None
                }
            }).next()
        },
        _ => None
    })
    .next()
    .unwrap_or_else(|| String::from(ast.ident.as_ref()))
}

fn impl_object(ast: &DeriveInput) -> quote::Tokens {
    let name = &ast.ident;
    
    let fields = match ast.body {
        Body::Struct(ref data) => data.fields(),
        Body::Enum(_) => panic!("#[derive(Object)] can only be used with structs"),
    };
    
    
    let parts: Vec<_> = fields.iter()
    .map(|field| {
        let (key, opt) = pdf_attr(field);
        (field.ident.clone(), key, opt)
    }).collect();
    
    let fields_ser = parts.iter()
    .map(|&(ref field, ref key, opt)| if opt {
        quote! {
            if let Some(ref field) = self.#field {
                write!(out, "{} ", #key)?;
                field.serialize(out)?;
                writeln!(out, "")?;
            }
        }
    } else {
        quote! {
            write!(out, "{} ", #key)?;
            self.#field.serialize(out)?;
            writeln!(out, "")?;
        }
    });
    
    let type_name = pdf_type(&ast);
    quote! {
        impl ::pdf::object::Object for #name {
            fn serialize<W: ::std::io::Write>(&self, out: &mut W) -> ::std::io::Result<()> {
                writeln!(out, "<<")?;
                writeln!(out, "/Type /{}", stringify!(#type_name))?;
                #(#fields_ser)*
                writeln!(out, ">>")?;
                Ok(())
            }
        }
    }
}


#[proc_macro_derive(FromDict, attributes(pdf))]
pub fn from_dict(input: TokenStream) -> TokenStream {
    // Construct a string representation of the type definition
    let s = input.to_string();
    
    // Parse the string representation
    let ast = syn::parse_derive_input(&s).unwrap();

    // Build the impl
    let gen = impl_from_dict(&ast);
    
    // Return the generated impl
    gen.parse().unwrap()
}

fn make_aliases(fields: &[Field]) -> Vec<Ty> {
    fields.iter().enumerate().map(|(i, field)| {
        let alias = format!("ty_{}", i);
        Ty::Path(None, Path {
            global: false,
            segments: vec![Ident::from(alias).into()]
        })
    })
    .collect()
}

fn impl_from_dict(ast: &syn::DeriveInput) -> quote::Tokens {
    let name = &ast.ident;
    
    let fields = match ast.body {
        Body::Struct(ref data) => data.fields(),
        Body::Enum(_) => panic!("#[derive(FromDict)] can only be used with structs"),
    };
    
    let aliases = make_aliases(&fields);
    
    let parts = fields.iter().zip(aliases.iter()).map(|(field, alias)| {
        let (key, opt) = pdf_attr(field);
        let ref name = field.ident;
        
        if opt {
            quote! {
                #name: #alias::from_primitive(dict.get(#key), r)?,
            }
        } else {
            quote! {
                #name: #alias::from_primitive(
                    dict.get(#key)
                    .ok_or(::pdf::err::ErrorKind::EntryNotFound { key: #key }.into())?,
                    r
                )?,
            }
        }
    });
    
    let aliases = fields.iter().zip(aliases.iter()).map(|(field, alias)| {
        let ref ty = field.ty;
        
        quote! {
            type #alias = #ty;
        }
    });
    
    let type_name = pdf_type(&ast);
    quote! {
        impl ::pdf::object::FromDict for #name {
            fn from_dict(dict: &::pdf::primitive::Dictionary, r: &::pdf::object::Resolve) -> ::std::result::Result<#name, ::pdf::err::Error> {
                use ::pdf::object::PrimitiveConv;
                #( #aliases )*
                assert_eq!(
                    dict.get("Type")
                    .ok_or(::pdf::err::ErrorKind::EntryNotFound { key:"Type" }.into())?
                    .as_name()?,
                    stringify!(#type_name)
                );
                Ok(#name {
                    #( #parts )*
                })
            }
        }
    }
}

#[proc_macro_derive(FromStream, attributes(pdf))]
pub fn from_stream(input: TokenStream) -> TokenStream {
    // Construct a string representation of the type definition
    let s = input.to_string();
    
    // Parse the string representation
    let ast = syn::parse_derive_input(&s).unwrap();

    // Build the impl
    let gen = impl_from_stream(&ast);
    
    // Return the generated impl
    gen.parse().unwrap()
}

fn impl_from_stream(ast: &syn::DeriveInput) -> quote::Tokens {
    let name = &ast.ident;
    
    let fields = match ast.body {
        Body::Struct(ref data) => data.fields(),
        Body::Enum(_) => panic!("#[derive(FromStream)] can only be used with structs"),
    };
    
    
    let aliases = make_aliases(&fields);
    
    let parts = fields.iter().zip(aliases.iter()).map(|(field, alias)| {
        let (key, opt) = pdf_attr(field);
        let ref name = field.ident;
        
        if opt {
            quote! {
                #name: #alias::from_primitive(dict.get(#key), r)?,
            }
        } else {
            quote! {
                #name: #alias::from_primitive(
                    dict.get(#key)
                    .ok_or(::pdf::err::ErrorKind::EntryNotFound { key: #key }.into())?,
                    r
                )?,
            }
        }
    });
    
    let aliases = fields.iter().zip(aliases.iter()).map(|(field, alias)| {
        let ref ty = field.ty;
        
        quote! {
            type #alias = #ty;
        }
    });
    
    let type_name = pdf_type(&ast);
    quote! {
        impl ::pdf::object::FromStream for #name {
            fn from_stream(dict: &::pdf::primitive::Stream, r: &::pdf::object::Resolve) -> ::std::result::Result<#name, ::pdf::err::Error> {
                use ::pdf::object::PrimitiveConv;
                #( #aliases )*
                let dict = &stream.info;
                assert_eq!(
                    dict.get("Type")
                    .ok_or(::pdf::err::ErrorKind::EntryNotFound { key:"Type" }.into())?
                    .as_name()?,
                    stringify!(#type_name)
                );
                Ok(#name {
                    #( #parts )*
                })
            }
        }
    }
}
