#![recursion_limit="128"]

extern crate proc_macro;
extern crate syn;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream;
use syn::*;

// Debugging:
/*
use std::fs::{File, OpenOptions};
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
    write!(file, "{}", gen);
    */
    // Return the generated impl
    gen.parse().unwrap()
}


/// Returns (key, opt, default)
fn field_attrs(field: &Field) -> (String, bool, Option<String>) {
    field.attrs.iter()
    .filter_map(|attr| match attr.value {
        MetaItem::List(ref ident, ref list) if ident == "pdf" => {
            let (mut key, mut opt, mut default) = (None, false, None);
            for meta in list {
                match *meta {
                    NestedMetaItem::MetaItem(MetaItem::NameValue(ref ident, Lit::Str(ref value, _))) 
                    if ident == "key"
                        => key = Some(value.clone()),
                    NestedMetaItem::MetaItem(MetaItem::NameValue(ref ident, Lit::Bool(value))) 
                    if ident == "opt"
                        => opt = value,
                    NestedMetaItem::MetaItem(MetaItem::NameValue(ref ident, Lit::Str(ref value, _)))
                    if ident == "default"
                        => default = Some(value.clone()),
                    _ => panic!(r##"Derive error - Supported derive attributes: `key="Key"`, `opt=[true|false]`, `default="some code"`."##)
                }
            }
            Some(( key.expect("attr `key` missing"), opt, default))
        },
        _ => None
    }).next().expect("no pdf meta attribute")
}




/// Just the attributes for the whole struct
#[derive(Default)]
struct GlobalAttrs {
    pdf_type: Option<String>,
    is_stream: bool,
}
impl GlobalAttrs {
    fn default(ast: &DeriveInput) -> GlobalAttrs {
        GlobalAttrs {
            pdf_type: Some(String::from(ast.ident.as_ref())),
            is_stream: false,
        }
    }

    /// The PDF type may be explicitly specified as an attribute with type "Type". Else, it is the name
    /// of the struct.
    fn from_ast(ast: &DeriveInput) -> GlobalAttrs {
        let mut attrs = GlobalAttrs::default(ast);
        for attr in &ast.attrs {
            match attr.value {
                MetaItem::List(ref ident, ref list) if ident == "pdf" => {
                    // Loop through list of attributes
                    for meta in list {
                        match *meta {
                            NestedMetaItem::MetaItem(MetaItem::NameValue(ref ident, ref value))
                            if ident == "Type" => match *value {
                                Lit::Str(ref value, _) => attrs.pdf_type = Some(value.clone()),
                                Lit::Bool(false) => attrs.pdf_type = None,
                                _ => {}
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
    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
    let attrs = GlobalAttrs::from_ast(&ast);
    
    let fields = match ast.body {
        Body::Struct(ref data) => data.fields(),
        Body::Enum(_) => panic!("#[derive(Object)] can only be used with structs"),
    };
    
    
    let parts: Vec<_> = fields.iter()
    .map(|field| {
        let (key, opt, default) = field_attrs(field);
        (field.ident.clone(), key, opt, default)
    }).collect();
    
    // Implement serialize()
    let fields_ser = parts.iter()
    .map( |&(ref field, ref key, opt, ref default)|
         if opt {
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
        }
    );
    
    let type_code = match attrs.pdf_type {
        Some(type_name) => quote! {
            writeln!(out, "/Type /{}", #type_name)?;
        },
        None => quote! {}
    };

    // Implement from_primitive()
    let from_primitive_code = match attrs.is_stream {
        false => impl_from_dict(&ast),
        true => impl_from_stream(&ast),
    };

    // Implement view()
    let fields_view = parts.iter()
    .map( |&(ref field, ref key, opt, ref default)| {
        quote! {
            viewer.attr(#key, |viewer| self.#field.view(viewer));
        }
    });

    quote! {
        impl #impl_generics ::pdf::object::Object for #name #ty_generics #where_clause {
            fn serialize<W: ::std::io::Write>(&self, out: &mut W) -> ::std::io::Result<()> {
                writeln!(out, "<<")?;
                #type_code
                #(#fields_ser)*
                writeln!(out, ">>")?;
                Ok(())
            }
            fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<Self> {
                #from_primitive_code
            }
            fn view<V: Viewer>(&self, viewer: &mut V) {
                #(#fields_view)*
            }

        }
    }

}

fn get_type(field: &Field) -> Ty {
    let (_name, opt, _default) = field_attrs(field);

    match opt {
        false => field.ty.clone(),
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
            data.types[0].clone()
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
        let (key, opt, default) = field_attrs(field);
        let ref name = field.ident;

        let ty = get_type(field);
        
        if opt {
            quote! {
                let #name = match dict.remove(#key) {
                    Some(p) => Some(
                        {let x: #ty = <#ty as Object>::from_primitive(p, resolve).chain_err(|| #key)?; x},
                    ),
                    None => None
                };
            }
        } else if let Some(default) = default {
            let default = syn::parse_token_trees(&default).unwrap();
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
                    // TODO: perhaps it's better to handle the case that #ty is Vec here, rather
                    // than allowing Vec<T> to be created from Primitive::Null?
                    let primitive: ::pdf::primitive::Primitive
                        = dict.remove(#key).or(Some(Primitive::Null)).unwrap(); // unwrap - we know it's not None
                    let x: #ty = <#ty as Object>::from_primitive(primitive, resolve)
                        .chain_err( || stringify!(#name) )?;
                        // TODO: figure out why the following gives "unexpected token"
                        // .chain_err( || Err(::pdf::err:ErrorKind::EntryNotFound {key: stringify!(#name)}) );
                    x
                };
            }
        }
    }).collect(),
    fields.iter().map(|field| {
        let ref name = field.ident;
        quote! { #name: #name, }
    }).collect())
}


fn impl_from_dict(ast: &syn::DeriveInput) -> quote::Tokens {
    let name = &ast.ident;
    let attrs = GlobalAttrs::from_ast(&ast);
    
    let fields = match ast.body {
        Body::Struct(ref data) => data.fields(),
        Body::Enum(_) => panic!("#[derive(Object)] can only be used with structs"),
    };
    
    
    let (let_parts, field_parts) = impl_parts(&fields);

    
    let type_check = match attrs.pdf_type {
        Some(type_name) => quote! {
            // Type check
            let result_p: ::pdf::err::Result<::pdf::primitive::Primitive> = dict.remove("Type").ok_or(
                ::pdf::err::ErrorKind::EntryNotFound { key: "Type" }.into()
            );
            assert_eq!(result_p?.to_name().chain_err(|| "Type")?, #type_name);
        },
        None => quote! {}
    };
    quote! {
        use ::pdf::err::ResultExt;
        let mut dict = p.to_dictionary(resolve).chain_err(|| stringify!(#name))?;
        #type_check
        #( #let_parts )*
        Ok(#name {
            #( #field_parts )*
        })
    }
}


fn impl_from_stream(ast: &syn::DeriveInput) -> quote::Tokens {
    let name = &ast.ident;
    let attrs = GlobalAttrs::from_ast(&ast);
    
    let fields = match ast.body {
        Body::Struct(ref data) => data.fields(),
        Body::Enum(_) => panic!("#[derive(Object)] can only be used with structs"),
    };
    
    let (let_parts, field_parts) = impl_parts(&fields);

    let type_check = match attrs.pdf_type {
        Some(type_name) => quote! {
            // Type check
            //println!("check for {}", stringify!(#name));
            let result_p: ::pdf::err::Result<::pdf::primitive::Primitive> = dict.remove("Type").ok_or(
                ::pdf::err::ErrorKind::EntryNotFound { key: "Type" }.into()
            );
            assert_eq!(result_p?.to_name().chain_err(|| "Type")?, #type_name);
        },
        None => quote! {}
    };
    quote! {
        use ::pdf::err::ResultExt;
        let mut dict = p.to_stream(resolve).chain_err(|| stringify!(#name))?.info;
        #type_check
        #( #let_parts )*
        Ok(#name {
            #( #field_parts )*
        })
    }

}
