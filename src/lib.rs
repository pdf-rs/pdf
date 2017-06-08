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

fn pdf_type(ast: &DeriveInput) -> Option<String> {
    ast.attrs.iter()
    .filter_map(|attr| match attr.value {
        MetaItem::List(ref ident, ref list) if ident == "pdf" => {
            list.iter().filter_map(|meta| {
                match *meta {
                    NestedMetaItem::MetaItem(MetaItem::NameValue(ref ident, ref value))
                    if ident == "Type" => match *value {
                        Lit::Str(ref value, _) => Some(Some(value.clone())),
                        Lit::Bool(false) => Some(None),
                        _ => None
                    },
                    _ => None
                }
            }).next()
        },
        _ => None
    })
    .next()
    .unwrap_or_else(|| Some(String::from(ast.ident.as_ref())))
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
    
    let type_code = match pdf_type(&ast) {
        Some(type_name) => quote! {
            writeln!(out, "/Type /{}", #type_name)?;
        },
        None => quote! {}
    };
    quote! {
        impl ::pdf::object::Object for #name {
            fn serialize<W: ::std::io::Write>(&self, out: &mut W) -> ::std::io::Result<()> {
                writeln!(out, "<<")?;
                #type_code
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
    fields.iter().enumerate().map(|(i, _field)| {
        let alias = format!("Ty{}", i);
        Ty::Path(None, Path {
            global: false,
            segments: vec![Ident::from(alias).into()]
        })
    })
    .collect()
}

fn define_alises(fields: &[Field], aliases: &[Ty]) -> Vec<quote::Tokens> {
    fields.iter().zip(aliases.iter()).map(|(field, alias)| {
        let (_name, opt) = pdf_attr(field);
        let ty = match opt {
            false => &field.ty,
            true => {
                let path = match field.ty {
                    Ty::Path(_, ref path) => path,
                    _ => panic!()
                };
                assert_eq!(1, path.segments.len());
                let data = match path.segments[0].parameters {
                    PathParameters::AngleBracketed(ref data) => data,
                    _ => panic!()
                };
                assert_eq!(1, data.types.len());
                &data.types[0]
            }
        };
        
        quote! {
            type #alias = #ty;
        }
    })
    .collect()
}

fn impl_parts(fields: &[Field], aliases: &[Ty]) -> Vec<quote::Tokens> {
    fields.iter().zip(aliases.iter()).map(|(field, alias)| {
        let (key, opt) = pdf_attr(field);
        let ref name = field.ident;
        
        if opt {
            quote! {
                #name: match dict.remove(#key) {
                    Some(p) => Some(#alias::from_primitive(p, r).chain_err(|| #key)?),
                    None => None
                },
            }
        } else {
            quote! {
                #name: {
                    let result_p: ::pdf::err::Result<::pdf::primitive::Primitive> = dict.remove(#key).ok_or(
                        ::pdf::err::ErrorKind::EntryNotFound { key: #key }.into()
                    );
                    #alias::from_primitive(result_p?, r).chain_err(|| stringify!(#name))?
                },
            }
        }
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
    
    let parts = impl_parts(&fields, &aliases);
    
    let impl_aliases = define_alises(&fields, &aliases);
    
    let type_check = match pdf_type(&ast) {
        Some(type_name) => quote! {
            // Type check
            //println!("check for {}", stringify!(#name));
            let result_p: ::pdf::err::Result<::pdf::primitive::Primitive> = dict.remove("Type").ok_or(
                ::pdf::err::ErrorKind::EntryNotFound { key: "Type" }.into()
            );
            assert_eq!(result_p?.as_name().chain_err(|| "Type")?, #type_name);
        },
        None => quote! {}
    };
    quote! {
        impl ::pdf::object::FromDict for #name {
            fn from_dict(
                mut dict: ::pdf::primitive::Dictionary,
                r:        &::pdf::object::Resolve
            ) -> ::pdf::err::Result<#name>
            {
                use ::pdf::object::FromPrimitive;
                use ::pdf::err::ResultExt;
                #( #impl_aliases )*
                #type_check
                Ok(#name {
                    #( #parts )*
                })
            }
        }
        impl ::pdf::object::FromPrimitive for #name {
            fn from_primitive(
                p:  ::pdf::primitive::Primitive,
                r:  &::pdf::object::Resolve
            ) -> ::pdf::err::Result<#name>
            {
                use ::pdf::object::FromDict;
                use ::pdf::err::ResultExt;
                #name::from_dict(p.as_dictionary(r).chain_err(|| stringify!(#name))?, r)
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
    
    let parts = impl_parts(&fields, &aliases);
    
    let impl_aliases = define_alises(&fields, &aliases);
    
    let type_check = match pdf_type(&ast) {
        Some(type_name) => quote! {
            // Type check
            //println!("check for {}", stringify!(#name));
            let result_p: ::pdf::err::Result<::pdf::primitive::Primitive> = dict.remove("Type").ok_or(
                ::pdf::err::ErrorKind::EntryNotFound { key: "Type" }.into()
            );
            assert_eq!(result_p?.as_name().chain_err(|| "Type")?, #type_name);
        },
        None => quote! {}
    };
    quote! {
        impl ::pdf::object::FromStream for #name {
            fn from_stream(
                mut dict: ::pdf::primitive::Stream,
                r:        &::pdf::object::Resolve
            ) -> ::pdf::err::Result<#name>
            {
                use ::pdf::object::FromPrimitive;
                use ::pdf::err::ResultExt;
                #( #impl_aliases )*
                let dict = &stream.info;
                #type_check
                Ok(#name {
                    #( #parts )*
                })
            }
        }
        impl ::pdf::object::FromPrimitive for #name {
            fn from_primitive(
                p: ::pdf::primitive::Primitive,
                r: &::pdf::object::Resolve
            ) -> ::pdf::err::Result<#name>
            {
                use ::pdf::object::FromStream;
                use ::pdf::err::ResultExt;
                #name::from_stream(p.as_stream(r).chain_err(|| stringify!(#name))?, r)
            }
        }
    }
}
