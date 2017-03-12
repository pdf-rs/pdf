extern crate proc_macro;
extern crate syn;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream;
use syn::{MetaItem, Lit, Body};

#[proc_macro_derive(Object, attributes(key, opt))]
pub fn object(input: TokenStream) -> TokenStream {
    // Construct a string representation of the type definition
    let s = input.to_string();
    
    // Parse the string representation
    let ast = syn::parse_macro_input(&s).unwrap();

    // Build the impl
    let gen = impl_object(&ast);
    
    // Return the generated impl
    gen.parse().unwrap()
}

fn impl_object(ast: &syn::MacroInput) -> quote::Tokens {
    let name = &ast.ident;
    
    let fields = match ast.body {
        Body::Struct(ref data) => data.fields(),
        Body::Enum(_) => panic!("#[derive(Object)] can only be used with structs"),
    };
    
    
    let parts: Vec<_> = fields.iter()
    .map(|field| {
        let (mut key, mut opt) = (None, false);
        for attr in field.attrs.iter() {
            match attr.value {
                MetaItem::NameValue(ref ident, Lit::Str(ref val, _))
                if ident == "key" => {
                    key = Some(val.clone());
                },
                MetaItem::Word(ref ident) if ident == "opt" => {
                    opt = true;
                },
                _ => {}
            }
        }
        
        (field.ident.clone(), key.expect("key attr missing").clone(), opt)
    }).collect();
    
    let fields_ser: Vec<_> = parts.iter()
    .map(|&(ref field, ref key, opt)| if opt {
        quote! {
            if let Some(field) = self.#field {
                write!(out, "{} ", #key)?;
                field.serialize(out);
                writeln!($out, "")?;
            }
        }
    } else {
        quote! {
            write!(out, "{} ", #key)?;
            self.#field.serialize(out);
            writeln!(out, "")?;
        }
    }).collect();
    
    let r = quote! {
        impl Object for #name {
            fn serialize<W: Write>(&self, out: &mut W) -> io::Result<()> {
                writeln!(out, "<<")?;
                writeln!(out, "/Type /{}", stringify!(#name))?;
                #(#fields_ser)*
                writeln!(out, ">>")?;
                Ok(())
            }
        }
    };
    println!("{}", r.as_str());
    r
}
