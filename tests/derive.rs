#[macro_use]
extern crate pdf_derive;


use std::io;
pub mod pdf {
    pub mod file {
        pub struct File<B>(B);
    }
    pub mod primitive {
        pub struct Primitive();
    }
    pub mod err {
        pub type Error = String;
    }
    pub mod object {
        use std::io;
        use super::file;
        use super::primitive;
        use super::err;
        
        pub trait Object {
            fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>;
        }
        impl Object for String {
            fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
                unimplemented!()
            }
        }
        pub trait PrimitiveConv: Sized {
            fn from_primitive<B>(p: &primitive::Primitive, reader: &file::File<B>) -> Result<Self, err::Error>;
        }
        
        impl PrimitiveConv for String {
            fn from_primitive<B>(p: &primitive::Primitive, reader: &file::File<B>) -> Result<Self, err::Error> {
                unimplemented!()
            }
        }
    }
}

mod test {
    use super::pdf;
    
    #[derive(Object, PrimitiveConv)]
    #[pdf(Type="X")]
    struct Test {
        #[pdf(key="Foo")]
        a:  String
    }
}
