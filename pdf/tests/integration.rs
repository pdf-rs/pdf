use std::str;
use pdf::file::{File, FileOptions};
use pdf::object::*;
use pdf::parser::{parse, ParseFlags};
use glob::glob;

macro_rules! file_path {
    ( $subdir:expr ) => { concat!("../files/", $subdir) }
}
macro_rules! run {
    ($e:expr) => (
        match $e {
            Ok(v) => v,
            Err(e) => {
                e.trace();
                panic!("{}", e);
            }
        }
    )
}

#[test]
fn open_file() {
    let _ = run!(FileOptions::uncached().open(file_path!("example.pdf")));
    #[cfg(feature = "mmap")]
    let _ = run!({
        use memmap2::Mmap;
        let file = std::fs::File::open(file_path!("example.pdf")).expect("can't open file");
        let mmap = unsafe { Mmap::map(&file).expect("can't mmap file") };
        FileOptions::cached().load(mmap)
    });
}

#[cfg(feature="cache")]
#[test]
fn read_pages() {
    for entry in glob(file_path!("*.pdf")).expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                println!("\n == Now testing `{}` ==", path.to_str().unwrap());

                let path = path.to_str().unwrap();
                let file = run!(FileOptions::cached().open(path));
                for i in 0 .. file.num_pages() {
                    println!("Read page {}", i);
                    let _ = file.get_page(i);
                }
            }
            Err(e) => println!("{:?}", e)
        }
    }
}

#[test]
fn user_password() {
    for entry in glob(file_path!("password_protected/*.pdf"))
        .expect("Failed to read glob pattern")
    {
        match entry {
            Ok(path) => {
                println!("\n\n == Now testing `{}` ==\n", path.to_str().unwrap());

                let path = path.to_str().unwrap();
                let file = run!(FileOptions::uncached().password(b"userpassword").open(path));
                for i in 0 .. file.num_pages() {
                    println!("\nRead page {}", i);
                    let _ = file.get_page(i);
                }
            }
            Err(e) => println!("{:?}", e)
        }
    }
}

#[test]
fn owner_password() {
    for entry in glob(file_path!("password_protected/*.pdf"))
        .expect("Failed to read glob pattern")
    {
        match entry {
            Ok(path) => {
                println!("\n\n == Now testing `{}` ==\n", path.to_str().unwrap());

                let path = path.to_str().unwrap();
                let file = run!(FileOptions::uncached().password(b"ownerpassword").open(path));
                for i in 0 .. file.num_pages() {
                    println!("\nRead page {}", i);
                    let _ = file.get_page(i);
                }
            }
            Err(e) => println!("{:?}", e)
        }
    }
}

// Test for invalid PDFs found by fuzzing.
// We don't care if they give an Err or Ok, as long as they don't panic.
#[cfg(feature="cache")]
#[test]
fn invalid_pdfs() {
    for entry in glob(file_path!("invalid/*.pdf"))
        .expect("Failed to read glob pattern")
    {
        match entry {
            Ok(path) => {
                let path = path.to_str().unwrap();
                println!("\n\n == Now testing `{}` ==\n", path);

                match FileOptions::cached().open(path) {
                    Ok(file) => {
                        for i in 0 .. file.num_pages() {
                            let _ = file.get_page(i);
                        }
                    }
                    Err(_) => {
                        continue;
                    }
                }
            }
            Err(e) => panic!("error when reading glob patterns: {:?}", e),
        }
    }
}

#[cfg(feature="cache")]
#[test]
fn parse_objects_from_stream() {
    use pdf::object::NoResolve;
    let file = run!(FileOptions::cached().open(file_path!("xelatex.pdf")));
    // .. we know that object 13 of that file is an ObjectStream
    let obj_stream: RcRef<ObjectStream> = run!(file.get(Ref::new(PlainRef {id: 13, gen: 0})));
    for i in 0..obj_stream.n_objects() {
        let (data, range) = run!(obj_stream.get_object_slice(i, &file));
        let slice = &data[range];
        println!("Object slice #{}: {}\n", i, str::from_utf8(slice).unwrap());
        run!(parse(slice, &NoResolve, ParseFlags::ANY));
    }
}

// TODO test decoding
