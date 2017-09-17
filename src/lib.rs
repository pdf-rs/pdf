#![recursion_limit="128"]

extern crate proc_macro;
extern crate syn;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream;
use syn::*;

// Debugging:
use std::fs::{File, OpenOptions};
use std::io::Write;



#[proc_macro_derive(Object, attributes(pdf))]
pub fn object(input: TokenStream) -> TokenStream {
    // Construct a string representation of the type definition
    let s = input.to_string();
    
    // Parse the string representation
    let ast = syn::parse_derive_input(&s).unwrap();

    // Build the impl
    let gen = impl_object(&ast);
    
    // Debugging
    let mut file = OpenOptions::new()
        .write(true)
        .append(true)
        .open("/tmp/proj/src/main.rs")
        .unwrap();
    write!(file, "{}", gen);
    // Return the generated impl
    gen.parse().unwrap()
}


/// Returns (key, default)
fn field_attrs(field: &Field) -> (String, Option<String>) {
    field.attrs.iter()
    .filter_map(|attr| match attr.value {
        MetaItem::List(ref ident, ref list) if ident == "pdf" => {
            let (mut key, mut default) = (None, None);
            for meta in list {
                match *meta {
                    NestedMetaItem::MetaItem(MetaItem::NameValue(ref ident, Lit::Str(ref value, _))) 
                    if ident == "key"
                        => key = Some(value.clone()),
                    NestedMetaItem::MetaItem(MetaItem::NameValue(ref ident, Lit::Str(ref value, _)))
                    if ident == "default"
                        => default = Some(value.clone()),
                    _ => panic!(r##"Derive error - Supported derive attributes: `key="Key"`, `default="some code"`."##)
                }
            }
            Some(( key.expect("attr `key` missing"), default))
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
    let attrs = GlobalAttrs::from_ast(&ast);
    if attrs.is_stream {
        match ast.body {
            Body::Struct(ref data) => impl_object_for_stream(ast, data.fields()),
            Body::Enum(ref variants) => panic!("Enum can't be a PDF stream"),
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
    let attrs = GlobalAttrs::from_ast(&ast);

    let ser_code: Vec<_> = variants.iter().map(|var| {
        quote! {
            #id::#var => stringify!(#id::#var),
        }
    }).collect();

    let from_primitive_code = impl_from_name(ast, variants);
    quote! {
        impl #impl_generics ::pdf::object::Object for #id #ty_generics #where_clause {
            fn serialize<W: ::std::io::Write>(&self, out: &mut W) -> ::std::io::Result<()> {
                unimplemented!();
                /*
                writeln!(out, "/{}",
                    match *self {
                        #( #ser_code )*
                    }
                );
                Ok(())
                */
            }
            fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<Self> {
                #from_primitive_code
            }
            fn view<V: Viewer>(&self, viewer: &mut V) {
                unimplemented!();
            }

        }
    }
}
// All we need for from_prim is.. 
// match &name {
// "Var1" => Var1,
// "Var2" => Var2,
// }
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
            _ => bail!(::pdf::err::Error::from(::pdf::err::ErrorKind::UnexpectedPrimitive { expected: "Name", found: p.get_debug_name() })),
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
        let (key, default) = field_attrs(field);
        (field.ident.clone(), key, default)
    }).collect();
    
    // Implement serialize()
    let fields_ser = parts.iter()
    .map( |&(ref field, ref key, ref default)|
        quote! {
            write!(out, "{} ", #key)?;
            self.#field.serialize(out)?;
            writeln!(out, "")?;
        }
    );
    
    let type_code = match attrs.pdf_type {
        Some(type_name) => quote! {
            writeln!(out, "/Type /{}", #type_name)?;
        },
        None => quote! {}
    };

    // Implement from_primitive()
    let from_primitive_code =  impl_from_dict(ast, fields);

    // Implement view()
    let fields_view = parts.iter()
    .map( |&(ref field, ref key, ref default)| {
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

/// Note: must have info and dict (TODO explain in docs)
fn impl_object_for_stream(ast: &DeriveInput, fields: &[Field]) -> quote::Tokens {
    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let mut info_ty = fields.iter()
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
            fn serialize<W: ::std::io::Write>(&self, out: &mut W) -> ::std::io::Result<()> {
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
                let ::pdf::primitive::Stream {info: info, data: data}
                    = p.to_stream(resolve).chain_err(|| stringify!(#name))?;

                Ok(#name {
                    info: <#info_ty as Object>::from_primitive(::pdf::primitive::Primitive::Dictionary (info), resolve)?,
                    data: data,
                })
            }
            fn view<V: Viewer>(&self, viewer: &mut V) {
                unimplemented!();
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
        let (key, default) = field_attrs(field);
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
                                Err(e) => bail!(e.chain_err(|| format!("Object {}, Key {} wrong primitive type", stringify!(#name), #key))),
                                // ^ TODO (??)
                                // Err(_) => bail!("Hello"),
                            }
                        None =>  // Try to construct T from Primitive::Null
                            match <#ty as Object>::from_primitive(::pdf::primitive::Primitive::Null, resolve) {
                                Ok(obj) => obj,
                                Err(e) => bail!("Object {}, Key {} not found", stringify!(#name), #key),
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
