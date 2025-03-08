use pdf::file::FileOptions;
use pdf::object::*;
use pdf::parser::{parse, ParseFlags};
use std::path::{Path, PathBuf};
use std::str;

macro_rules! run {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => {
                panic!("{}", e);
            }
        }
    };
}

fn files() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("files")
}
fn file_path(s: &str) -> PathBuf {
    files().join(s)
}
fn dir_pdfs(path: PathBuf) -> impl Iterator<Item = PathBuf> {
    path.read_dir()
        .unwrap()
        .filter_map(|r| r.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "pdf").unwrap_or(false))
}

#[test]
fn open_file() {
    let _ = run!(FileOptions::uncached().open(file_path("example.pdf")));
    #[cfg(all(feature = "mmap", feature = "cache"))]
    let _ = run!({
        use memmap2::Mmap;
        let file = std::fs::File::open(file_path!("example.pdf")).expect("can't open file");
        let mmap = unsafe { Mmap::map(&file).expect("can't mmap file") };
        FileOptions::cached().load(mmap)
    });
}

#[cfg(feature = "cache")]
#[test]
fn read_pages() {
    for path in dir_pdfs(files()) {
        println!("\n == Now testing `{}` ==", path.to_str().unwrap());

        let path = path.to_str().unwrap();
        let file = run!(FileOptions::cached().open(path));
        for i in 0..file.num_pages() {
            println!("Read page {}", i);
            let _ = file.get_page(i);
        }
    }
}

#[test]
fn user_password() {
    for path in dir_pdfs(file_path("password_protected")) {
        println!("\n\n == Now testing `{}` ==\n", path.to_str().unwrap());

        let path = path.to_str().unwrap();
        let file = run!(FileOptions::uncached().password(b"userpassword").open(path));
        for i in 0..file.num_pages() {
            println!("\nRead page {}", i);
            let _ = file.get_page(i);
        }
    }
}

#[test]
fn owner_password() {
    for path in dir_pdfs(file_path("password_protected")) {
        println!("\n\n == Now testing `{}` ==\n", path.to_str().unwrap());

        let path = path.to_str().unwrap();
        let file = run!(FileOptions::uncached()
            .password(b"ownerpassword")
            .open(path));
        for i in 0..file.num_pages() {
            println!("\nRead page {}", i);
            let _ = file.get_page(i);
        }
    }
}

// Test for invalid PDFs found by fuzzing.
// We don't care if they give an Err or Ok, as long as they don't panic.
#[cfg(feature = "cache")]
#[test]
fn invalid_pdfs() {
    for path in dir_pdfs(file_path("invalid")) {
        let path = path.to_str().unwrap();
        println!("\n\n == Now testing `{}` ==\n", path);

        match FileOptions::cached().open(path) {
            Ok(file) => {
                for i in 0..file.num_pages() {
                    let _ = file.get_page(i);
                }
            }
            Err(_) => {
                continue;
            }
        }
    }
}

#[cfg(feature = "cache")]
#[test]
fn parse_objects_from_stream() {
    use pdf::object::NoResolve;
    let file = run!(FileOptions::cached().open(file_path("xelatex.pdf")));
    let resolver = file.resolver();

    // .. we know that object 13 of that file is an ObjectStream
    let obj_stream: RcRef<ObjectStream> = run!(resolver.get(Ref::new(PlainRef { id: 13, gen: 0 })));
    for i in 0..obj_stream.n_objects() {
        let (data, range) = run!(obj_stream.get_object_slice(i, &resolver));
        let slice = &data[range];
        println!("Object slice #{}: {}\n", i, str::from_utf8(slice).unwrap());
        run!(parse(slice, &NoResolve, ParseFlags::ANY));
    }
}

// TODO test decoding
