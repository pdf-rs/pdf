#![recursion_limit="128"]

extern crate proc_macro;
extern crate syn;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream;
use syn::*;

// for debugging:
use std::fs::{File, OpenOptions};
use std::io::Write;

/*
    let mut file = OpenOptions::new()
        .write(true)
        .append(true)
        .open("/tmp/proj/src/main.rs")
        .unwrap();
    write!(file, "{}", gen);
    */


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
        let (key, opt) = pdf_attr(field);
        (field.ident.clone(), key, opt)
    }).collect();
    
    // serialize()
    let fields_ser = parts.iter()
    .map( |&(ref field, ref key, opt)|
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

    // from_primitive()
    let from_primitive_code = match attrs.is_stream {
        false => impl_from_dict(&ast),
        true => impl_from_stream(&ast),
    };

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

        }
    }

}

fn get_type(field: &Field) -> Ty {
    let (_name, opt) = pdf_attr(field);

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

fn impl_parts(fields: &[Field]) -> Vec<quote::Tokens> {
    fields.iter().map(|field| {
        let (key, opt) = pdf_attr(field);
        let ref name = field.ident;

        let ty = get_type(field);
        
        if opt {
            quote! {
                #name: match dict.remove(#key) {
                    Some(p) => Some(
                        {let x: #ty = <#ty as Object>::from_primitive(p, resolve).chain_err(|| #key)?; x},
                    ),
                    None => None
                },
            }
        } else {
            quote! {
                #name: {
                    let result_p: ::pdf::err::Result<::pdf::primitive::Primitive> = dict.remove(#key).ok_or(
                        ::pdf::err::ErrorKind::EntryNotFound { key: #key }.into()
                    );
                    let x: #ty = <#ty as Object>::from_primitive(result_p?, resolve).chain_err(|| stringify!(#name))?;
                    x
                },
            }
        }
    })
    .collect()
}


fn impl_from_dict(ast: &syn::DeriveInput) -> quote::Tokens {
    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
    let attrs = GlobalAttrs::from_ast(&ast);
    
    let fields = match ast.body {
        Body::Struct(ref data) => data.fields(),
        Body::Enum(_) => panic!("#[derive(Object)] can only be used with structs"),
    };
    
    
    let parts = impl_parts(&fields);

    
    let type_check = match attrs.pdf_type {
        Some(type_name) => quote! {
            // Type check
            let result_p: ::pdf::err::Result<::pdf::primitive::Primitive> = dict.remove("Type").ok_or(
                ::pdf::err::ErrorKind::EntryNotFound { key: "Type" }.into()
            );
            assert_eq!(result_p?.as_name().chain_err(|| "Type")?, #type_name);
        },
        None => quote! {}
    };
    quote! {
        use ::pdf::err::ResultExt;
        let mut dict = p.as_dictionary(resolve).chain_err(|| stringify!(#name))?;
        #type_check
        Ok(#name {
            #( #parts )*
        })
    }
}


fn impl_from_stream(ast: &syn::DeriveInput) -> quote::Tokens {
    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
    let attrs = GlobalAttrs::from_ast(&ast);
    
    let fields = match ast.body {
        Body::Struct(ref data) => data.fields(),
        Body::Enum(_) => panic!("#[derive(Object)] can only be used with structs"),
    };
    
    let parts = impl_parts(&fields);

    let type_check = match attrs.pdf_type {
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
        use ::pdf::err::ResultExt;
        let mut dict = p.as_stream(resolve).chain_err(|| stringify!(#name))?.info;
        #type_check
        Ok(#name {
            #( #parts )*
        })
    }

}
