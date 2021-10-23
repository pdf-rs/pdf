use std::str;

use glob::glob;
use pdf::file::File;
use pdf::object::*;
use pdf::parser::parse;

macro_rules! file_path {
    ( $subdir:expr ) => {
        concat!("../files/", $subdir)
    };
}
macro_rules! run {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => {
                e.trace();
                panic!("{}", e);
            }
        }
    };
}

#[test]
fn open_file() {
    let _ = run!(File::open(file_path!("example.pdf")));
    #[cfg(feature = "mmap")]
    let _ = run!({
        use memmap::Mmap;
        let file = std::fs::File::open(file_path!("example.pdf")).expect("can't open file");
        let mmap = unsafe { Mmap::map(&file).expect("can't mmap file") };
        File::from_data(mmap)
    });
}

#[test]
fn read_pages() {
    for entry in glob(file_path!("*.pdf")).expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                println!("\n == Now testing `{}` ==", path.to_str().unwrap());

                let path = path.to_str().unwrap();
                let file = run!(File::<Vec<u8>>::open(path));
                for i in 0..file.num_pages() {
                    println!("Read page {}", i);
                    let _ = file.get_page(i);
                }
            }
            Err(e) => println!("{:?}", e),
        }
    }
}

#[test]
fn user_password() {
    for entry in glob(file_path!("password_protected/*.pdf")).expect("Failed to read glob pattern")
    {
        match entry {
            Ok(path) => {
                println!("\n\n == Now testing `{}` ==\n", path.to_str().unwrap());

                let path = path.to_str().unwrap();
                let file = run!(File::<Vec<u8>>::open_password(path, b"userpassword"));
                for i in 0..file.num_pages() {
                    println!("\nRead page {}", i);
                    let _ = file.get_page(i);
                }
            }
            Err(e) => println!("{:?}", e),
        }
    }
}

#[test]
fn owner_password() {
    for entry in glob(file_path!("password_protected/*.pdf")).expect("Failed to read glob pattern")
    {
        match entry {
            Ok(path) => {
                println!("\n\n == Now testing `{}` ==\n", path.to_str().unwrap());

                let path = path.to_str().unwrap();
                let file = run!(File::<Vec<u8>>::open_password(path, b"ownerpassword"));
                for i in 0..file.num_pages() {
                    println!("\nRead page {}", i);
                    let _ = file.get_page(i);
                }
            }
            Err(e) => println!("{:?}", e),
        }
    }
}

#[test]
fn parse_objects_from_stream() {
    use pdf::object::NoResolve;
    let file = run!(File::<Vec<u8>>::open(file_path!("xelatex.pdf")));
    // .. we know that object 13 of that file is an ObjectStream
    let obj_stream: RcRef<ObjectStream> = run!(file.get(Ref::new(PlainRef { id: 13, gen: 0 })));
    for i in 0..obj_stream.n_objects() {
        let slice = run!(obj_stream.get_object_slice(i));
        println!("Object slice #{}: {}\n", i, str::from_utf8(slice).unwrap());
        run!(parse(slice, &NoResolve));
    }
}

// TODO test decoding
