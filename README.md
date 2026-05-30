# pdf-rs [![test](https://github.com/pdf-rs/pdf/actions/workflows/test.yml/badge.svg)](https://github.com/pdf-rs/pdf/actions/workflows/test.yml)

A comprehensive Rust library for reading, altering, and writing PDF files with extensive API support for PDF manipulation, content extraction, and document creation.

Modifying and writing PDFs is still experimental.

One easy way you can contribute is to add different PDF files to [`tests/files`](files/) and see if they pass the tests (`cargo test`).

We welcome contributions in the form of ideas, issues, or code! Please join [us on Zulip](https://type.zulipchat.com/#narrow/stream/209232-pdf) if you have any questions or problems.

## Workspace
This repository uses a Cargo Workspace with default members. By default, only the `pdf` library is built.
To build additional parts, such as the `read` example, pass the `--package` flag with the subcrate name (e.g., `cargo build --package=read`).

## Examples
Examples are located in the [`pdf/examples/`](pdf/examples/) directory and can be executed using:

```bash
cargo run --example {content|metadata|names|read|text} -- <path/to/your.pdf>
```

## Renderer and Viewer
A library for rendering PDFs using [Pathfinder](https://github.com/servo/pathfinder), along with a minimal viewer, can be found at [pdf-rs/pdf_render](https://github.com/pdf-rs/pdf_render).

## Inspect
An interactive tool for visualizing PDF file structure as a hierarchy of primitives is available at [pdf-rs/inspect-prim](https://github.com/pdf-rs/inspect-prim). To use it, clone the repository and run `cargo run`.


## Table of Contents

- [Introduction](#introduction)
- [Installation](#installation)
  - [Available Features](#available-features)
- [Getting Started](#getting-started)
  - [Reading a PDF File](#reading-a-pdf-file)
  - [Loading PDF from Memory Buffer](#loading-pdf-from-memory-buffer)
  - [Creating a Simple PDF](#creating-a-simple-pdf)
- [Features](#features)
  - [Core Capabilities](#core-capabilities)
  - [Advanced Features](#advanced-features)
- [API Reference](#api-reference)
  - [File Operations](#file-operations)
    - [Opening PDF Files](#opening-pdf-files)
        - [Opening PDF Files from local disk](#opening-pdf-files-from-local-disk)
        - [Opening PDF Files from in-memory buffer](#opening-pdf-files-from-in-memory-buffer)
    - [File Structure Access](#file-structure-access)
  - [PDF Creation](#pdf-creation)
    - [Using PdfBuilder](#using-pdfbuilder)
    - [Advanced Page Creation](#advanced-page-creation)
  - [PDF Modification](#pdf-modification)
    - [Updating Existing PDFs](#updating-existing-pdfs)
    - [Adding Images to PDFs](#adding-images-to-pdfs)
  - [Content Extraction](#content-extraction)
    - [Extracting Images](#extracting-images)
    - [Extracting Fonts](#extracting-fonts)
    - [Text Extraction](#text-extraction)
  - [Metadata Handling](#metadata-handling)
    - [Reading Document Information](#reading-document-information)
    - [Setting Document Metadata](#setting-document-metadata)
  - [Interactive Features](#interactive-features)
    - [Working with Forms (AcroForms)](#working-with-forms-acroforms)
    - [Handling Annotations](#handling-annotations)
  - [Navigation Features](#navigation-features)
    - [Working with Outlines (Bookmarks)](#working-with-outlines-bookmarks)
    - [Named Destinations](#named-destinations)
  - [Low-level Operations](#low-level-operations)
    - [Working with Primitives](#working-with-primitives)
    - [Stream Processing](#stream-processing)
    - [Custom Content Operations](#custom-content-operations)
- [Examples](#examples)
  - [Core Examples (pdf/examples/)](#core-examples-(pdf/examples/))
  - [Extended Examples (examples/src/bin/)](#extended-examples-(examples/src/bin/))
  - [Running Examples](#running-examples)
- [Workspace Structure](#workspace-structure)
  - [Building Specific Components](#building-specific-components)
- [Contributing](#contributing)
  - [Easy Contributions](#easy-contributions)
  - [Development Contributions](#development-contributions)
  - [Getting Help](#getting-help)
  - [Development Setup](#development-setup)
- [Related Projects](#related-projects)
  - [Renderer and Viewer](#renderer-and-viewer)
  - [PDF Inspector](#pdf-inspector)
  - [Community Projects](#community-projects)
- [License](#license)

## Introduction

pdf-rs is a powerful Rust crate designed for comprehensive PDF file manipulation. It provides a rich set of APIs for reading existing PDFs, extracting content (text, images, fonts), modifying documents, and creating new PDFs from scratch. The library supports both high-level operations for common tasks and low-level primitives for advanced PDF manipulation.

**Note:** Modifying and writing PDFs is still experimental, but the library provides robust reading capabilities and is actively developed with community contributions.

## Installation

Add pdf-rs to your `Cargo.toml`:

```toml
[dependencies]
pdf = "0.9.0"

# Optional features
pdf = { version = "0.9.0", features = ["cache", "mmap", "threads"] }
```

### Available Features

- `cache` (default): Enable object caching for better performance
- `sync` (default): Thread-safe operations
- `mmap`: Memory-mapped file access for large PDFs
- `threads`: Multi-threaded JPEG decoding
- `dump`: Temporary file support for debugging

## Getting Started

### Reading a PDF File

```rust
use pdf::file::FileOptions;
use pdf::error::PdfError;

fn main() -> Result<(), PdfError> {
    // Open a PDF file with caching enabled
    let file = FileOptions::cached().open("document.pdf")?;
    
    // Access document metadata
    if let Some(info) = &file.trailer.info_dict {
        if let Some(title) = &info.title {
            println!("Title: {}", title.to_string_lossy());
        }
        if let Some(author) = &info.author {
            println!("Author: {}", author.to_string_lossy());
        }
    }
    
    // Iterate through pages
    for (page_num, page) in file.pages().enumerate() {
        let page = page?;
        println!("Page {}: {:?}", page_num + 1, page.media_box);
    }
    
    Ok(())
}
```

### Loading PDF from Memory Buffer

For scenarios where you have PDF data in memory (from network requests, embedded resources, or other sources), you can load PDFs directly from byte buffers without writing to disk:

```rust
use pdf::file::FileOptions;
use pdf::error::PdfError;

fn load_from_buffer() -> Result<(), PdfError> {
    // From a byte slice (&[u8])
    let pdf_data: &[u8] = &your_pdf_bytes;
    let file = FileOptions::cached().load(pdf_data)?;
    
    // From a Vec<u8>
    let pdf_buffer: Vec<u8> = get_pdf_data_from_somewhere();
    let file = FileOptions::cached().load(pdf_buffer)?;
    
    // From any type that can be dereferenced to [u8]
    let boxed_data: Box<[u8]> = pdf_data.into();
    let file = FileOptions::cached().load(boxed_data)?;
    
    // Process the file same as file-based loading
    println!("Loaded PDF with {} pages", file.num_pages());
    
    // Access pages and content
    for (page_num, page) in file.pages().enumerate() {
        let page = page?;
        println!("Page {}: {:?}", page_num + 1, page.media_box);
    }
    
    Ok(())
}

// Example: Loading from HTTP response
async fn load_from_http() -> Result<(), Box<dyn std::error::Error>> {
    // Simulate getting PDF data from HTTP (replace with your HTTP client)
    let pdf_bytes = download_pdf_from_url("https://example.com/document.pdf").await?;
    
    // Load directly from the downloaded bytes
    let file = FileOptions::cached().load(&pdf_bytes)?;
    
    println!("Downloaded and loaded PDF with {} pages", file.num_pages());
    
    Ok(())
}

// Example: Loading embedded PDF resource
fn load_embedded_pdf() -> Result<(), PdfError> {
    // PDF data embedded in the binary
    const EMBEDDED_PDF: &[u8] = include_bytes!("../resources/embedded.pdf");
    
    let file = FileOptions::cached().load(EMBEDDED_PDF)?;
    println!("Loaded embedded PDF with {} pages", file.num_pages());
    
    Ok(())
}
```

The `load()` method accepts any type that implements `Deref<Target=[u8]>`, including:
- `&[u8]` - Byte slices
- `Vec<u8>` - Owned byte vectors  
- `Box<[u8]>` - Boxed byte arrays
- `Arc<[u8]>` - Reference-counted byte arrays
- Any custom type that dereferences to `[u8]`

### Creating a Simple PDF

```rust
use pdf::build::*;
use pdf::content::*;
use pdf::object::*;
use pdf::primitive::PdfString;
use pdf::file::FileOptions;

fn create_pdf() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = PdfBuilder::new(FileOptions::cached());
    
    // Create content with drawing operations
    let content = Content::from_ops(vec![
        Op::MoveTo { p: Point { x: 100., y: 100. } },
        Op::LineTo { p: Point { x: 200., y: 200. } },
        Op::LineTo { p: Point { x: 300., y: 100. } },
        Op::Close,
        Op::Stroke,
    ]);
    
    // Build page with content
    let mut page = PageBuilder::from_content(content, &NoResolve)?;
    page.media_box = Some(Rectangle {
        left: 0.0, bottom: 0.0,
        right: 400.0, top: 400.0
    });
    page.resources = Resources::default();
    
    // Create catalog and document info
    let catalog = CatalogBuilder::from_pages(vec![page]);
    let mut info = InfoDict::default();
    info.title = Some(PdfString::from("My PDF Document"));
    
    // Build and save PDF
    let data = builder.info(info).build(catalog)?;
    std::fs::write("output.pdf", data)?;
    
    Ok(())
}
```

## Features

### Core Capabilities
- **PDF Reading**: Complete PDF parsing with support for all PDF versions
- **Content Extraction**: Extract text, images, fonts, and metadata
- **PDF Creation**: Build new PDFs from scratch with full control
- **PDF Modification**: Update existing PDFs, add content, modify pages
- **Form Handling**: Read and manipulate AcroForms and form fields
- **Annotation Support**: Access and modify PDF annotations
- **Encryption Support**: Handle password-protected and encrypted PDFs
- **Stream Processing**: Decode and encode various PDF stream filters
- **Font Management**: Extract embedded fonts and font information
- **Image Processing**: Extract images in various formats (JPEG, PNG, JBIG2, etc.)

### Advanced Features
- **Memory Mapping**: Efficient handling of large PDF files
- **Caching System**: Object caching for improved performance
- **Multi-threading**: Parallel processing support
- **Error Recovery**: Robust parsing with repair capabilities
- **Logging**: Comprehensive logging for debugging and monitoring

## API Reference

### File Operations

#### Opening PDF Files

##### Opening PDF Files from local disk

The [`FileOptions`](pdf/src/file.rs:1) struct provides flexible options for opening PDF files from disk or memory:

```rust
use pdf::file::{FileOptions, Log};

// Basic file opening from disk
let file = FileOptions::cached().open("document.pdf")?;

// Loading from memory buffer
let pdf_data: &[u8] = &your_pdf_bytes;
let file = FileOptions::cached().load(pdf_data)?;

// Loading from Vec<u8>
let pdf_buffer: Vec<u8> = get_pdf_data();
let file = FileOptions::cached().load(pdf_buffer)?;

// Custom logging (works with both open() and load())
struct MyLog;
impl Log for MyLog {
    fn load_object(&self, r: pdf::object::PlainRef) {
        println!("Loading object: {:?}", r);
    }
}

let file = FileOptions::cached()
    .log(MyLog)
    .open("document.pdf")?;

// Or with buffer loading
let file = FileOptions::cached()
    .log(MyLog)
    .load(&pdf_data)?;

// Memory-mapped access for large files (disk only)
#[cfg(feature = "mmap")]
let file = FileOptions::cached()
    .mmap()
    .open("large_document.pdf")?;
```

##### Opening PDF Files from in-memory buffer

The `load()` method works with any type implementing `Deref<Target=[u8]>`:

```rust
// Different buffer types supported
let file1 = FileOptions::cached().load(&pdf_bytes[..])?;     // &[u8]
let file2 = FileOptions::cached().load(pdf_vec)?;           // Vec<u8>
let file3 = FileOptions::cached().load(Box::from(pdf_vec))?; // Box<[u8]>
let file4 = FileOptions::cached().load(Arc::from(pdf_vec))?; // Arc<[u8]>

// Example: Reading PDF from network and loading directly
async fn load_remote_pdf() -> Result<(), Box<dyn std::error::Error>> {
    let response = reqwest::get("https://example.com/document.pdf").await?;
    let pdf_bytes = response.bytes().await?;
    
    // Load directly from downloaded bytes
    let file = FileOptions::cached().load(&pdf_bytes[..])?;
    println!("Loaded remote PDF with {} pages", file.num_pages());
    
    Ok(())
}
```

#### File Structure Access

```rust
// Access document trailer and cross-reference table
let trailer = &file.trailer;
let root = file.get_root();

// Get document version
let version = file.version;
println!("PDF Version: {}.{}", version.major, version.minor);

// Access resolver for object references
let resolver = file.resolver();
```

### PDF Creation

#### Using PdfBuilder

The [`PdfBuilder`](pdf/src/build.rs:1) provides a high-level interface for creating PDFs:

```rust
use pdf::build::*;
use pdf::content::*;
use pdf::object::*;

let mut builder = PdfBuilder::new(FileOptions::cached());

// Create multiple pages
let mut pages = Vec::new();

// Page 1: Simple drawing
let content1 = Content::from_ops(vec![
    Op::MoveTo { p: Point { x: 50., y: 50. } },
    Op::LineTo { p: Point { x: 150., y: 150. } },
    Op::Stroke,
]);
let page1 = PageBuilder::from_content(content1, &NoResolve)?;
pages.push(page1);

// Page 2: Rectangle with fill
let content2 = Content::from_ops(vec![
    Op::Rectangle { 
        rect: pdf::content::Rect {
            x: 100., y: 100.,
            width: 200., height: 100.
        }
    },
    Op::Fill,
]);
let page2 = PageBuilder::from_content(content2, &NoResolve)?;
pages.push(page2);

// Build catalog and create PDF
let catalog = CatalogBuilder::from_pages(pages);
let data = builder.build(catalog)?;
```

#### Advanced Page Creation

```rust
use pdf::object::*;
use pdf::content::*;

// Create page with custom properties
let mut page_builder = PageBuilder::new();

// Set page dimensions (A4 size)
page_builder.media_box = Some(Rectangle {
    left: 0.0, bottom: 0.0,
    right: 595.0, top: 842.0  // A4 in points
});

// Add resources (fonts, images, etc.)
let mut resources = Resources::default();
// resources.fonts.insert("F1".into(), font_ref);
page_builder.resources = resources;

// Create content stream
let content = Content::from_ops(vec![
    Op::Save,
    Op::Transform { 
        matrix: Matrix {
            a: 1.0, b: 0.0, c: 0.0, 
            d: 1.0, e: 100.0, f: 700.0
        }
    },
    Op::Restore,
]);

page_builder.contents = Some(content);
```

### PDF Modification

#### Updating Existing PDFs

```rust
use pdf::file::FileOptions;
use pdf::object::*;
use pdf::content::*;

// Open existing PDF for modification
let mut file = FileOptions::cached().open("input.pdf")?;

// Get a specific page
let page = file.get_page(0)?; // First page
let mut page_copy: Page = (*page).clone();

// Modify page content
if let Some(existing_content) = &page.contents {
    let mut ops = existing_content.operations(&file.resolver())?;
    
    // Add new drawing operations
    ops.extend(vec![
        Op::Save,
        Op::MoveTo { p: Point { x: 300., y: 300. } },
        Op::LineTo { p: Point { x: 400., y: 400. } },
        Op::Stroke,
        Op::Restore,
    ]);
    
    // Update page content
    page_copy.contents = Some(Content::from_ops(ops));
}

// Update the page in the file
file.update(page.get_ref().get_inner(), page_copy)?;

// Save modified PDF
file.save_to("output.pdf")?;
```

#### Adding Images to PDFs

Based on the [`add_image.rs`](examples/src/bin/add_image.rs:1) example:

```rust
use pdf::object::*;
use pdf::content::*;
use pdf::enc::{StreamFilter, DCTDecodeParams};

// Load image data
let img_data = std::fs::read("image.jpg")?;

// Create image dictionary
let image_dict = ImageDict {
    width: 800,
    height: 600,
    color_space: Some(ColorSpace::DeviceRGB),
    bits_per_component: Some(8),
    ..Default::default()
};

// Create image stream
let image_stream = Stream::new_with_filters(
    image_dict, 
    img_data, 
    vec![StreamFilter::DCTDecode(DCTDecodeParams { 
        color_transform: None 
    })]
);

// Create XObject and add to file
let image_obj = XObject::Image(ImageXObject { inner: image_stream });
let image_ref = file.create(image_obj)?;

// Add image to page resources
let mut resources: Resources = (*page.resources()?).clone();
resources.xobjects.insert("MyImage".into(), image_ref.get_ref());

// Add image to page content
let mut ops = page.contents.as_ref().unwrap().operations(&file.resolver())?;
ops.extend(vec![
    Op::Save,
    Op::Transform { 
        matrix: Matrix {
            a: 200.0, d: 150.0,  // Scale
            b: 0.0, c: 0.0,      // No skew
            e: 100.0, f: 100.0,  // Position
        }
    },
    Op::XObject { name: "MyImage".into() },
    Op::Restore,
]);
```

### Content Extraction

#### Extracting Images

Based on the [`read.rs`](pdf/examples/read.rs:1) example:

```rust
use pdf::object::*;
use pdf::enc::StreamFilter;

let mut images = Vec::new();
let resolver = file.resolver();

// Collect images from all pages
for page in file.pages() {
    let page = page?;
    let resources = page.resources()?;
    
    // Extract XObject images
    images.extend(
        resources.xobjects
            .iter()
            .map(|(_name, &r)| resolver.get(r).unwrap())
            .filter(|o| matches!(**o, XObject::Image(_)))
    );
}

// Process and save images
for (i, obj) in images.iter().enumerate() {
    if let XObject::Image(img) = &**obj {
        let (data, filter) = img.raw_image_data(&resolver)?;
        
        let (extension, processed_data) = match filter {
            Some(StreamFilter::DCTDecode(_)) => ("jpg", data),
            Some(StreamFilter::FlateDecode(_)) => ("png", data),
            Some(StreamFilter::CCITTFaxDecode(_)) => {
                let tiff_data = fax::tiff::wrap(&data, img.width, img.height);
                ("tiff", tiff_data.into())
            },
            _ => continue,
        };
        
        let filename = format!("image_{}.{}", i, extension);
        std::fs::write(&filename, processed_data)?;
        println!("Extracted: {}", filename);
    }
}
```

#### Extracting Fonts

```rust
use std::collections::HashMap;
use pdf::object::*;

let mut fonts = HashMap::new();
let resolver = file.resolver();

// Collect fonts from all pages
for page in file.pages() {
    let page = page?;
    let resources = page.resources()?;
    
    for (name, font_ref) in resources.fonts.iter() {
        let font = font_ref.load(&resolver)?;
        fonts.insert(name.clone(), font);
    }
}

// Extract embedded font data
for (name, font) in fonts.iter() {
    if let Some(Ok(font_data)) = font.embedded_data(&resolver) {
        let filename = format!("font_{}.ttf", name);
        std::fs::write(&filename, font_data)?;
        println!("Extracted font: {}", filename);
    }
}
```

#### Text Extraction

```rust
use pdf::content::*;
use pdf::object::*;

for page in file.pages() {
    let page = page?;
    
    if let Some(content) = &page.contents {
        let operations = content.operations(&file.resolver())?;
        
        for op in operations {
            match op {
                Op::TextDraw { text } => {
                    println!("Text: {}", text.to_string_lossy());
                },
                Op::TextDrawAdjusted { array } => {
                    for item in array {
                        if let TextDrawAdjusted::Text(text) = item {
                            println!("Text: {}", text.to_string_lossy());
                        }
                    }
                },
                _ => {}
            }
        }
    }
}
```

### Metadata Handling

#### Reading Document Information

```rust
use pdf::object::InfoDict;

// Access document information dictionary
if let Some(info) = &file.trailer.info_dict {
    // Basic metadata
    if let Some(title) = &info.title {
        println!("Title: {}", title.to_string_lossy());
    }
    if let Some(author) = &info.author {
        println!("Author: {}", author.to_string_lossy());
    }
    if let Some(subject) = &info.subject {
        println!("Subject: {}", subject.to_string_lossy());
    }
    if let Some(keywords) = &info.keywords {
        println!("Keywords: {}", keywords.to_string_lossy());
    }
    if let Some(creator) = &info.creator {
        println!("Creator: {}", creator.to_string_lossy());
    }
    if let Some(producer) = &info.producer {
        println!("Producer: {}", producer.to_string_lossy());
    }
    
    // Dates
    if let Some(creation_date) = &info.creation_date {
        println!("Created: {}", creation_date.to_string_lossy());
    }
    if let Some(mod_date) = &info.modification_date {
        println!("Modified: {}", mod_date.to_string_lossy());
    }
}
```

#### Setting Document Metadata

```rust
use pdf::object::InfoDict;
use pdf::primitive::PdfString;

let mut info = InfoDict::default();
info.title = Some(PdfString::from("My Document"));
info.author = Some(PdfString::from("John Doe"));
info.subject = Some(PdfString::from("PDF Processing"));
info.keywords = Some(PdfString::from("rust, pdf, library"));
info.creator = Some(PdfString::from("pdf-rs"));

// Use with PdfBuilder
let data = builder.info(info).build(catalog)?;
```

### Interactive Features

#### Working with Forms (AcroForms)

```rust
use pdf::object::*;
use pdf::primitive::Primitive;

// Access form fields
if let Some(forms) = &file.get_root().forms {
    println!("Form fields:");
    
    for field in forms.fields.iter() {
        print!("Field '{}': ", field.name);
        
        match &field.value {
            Primitive::String(s) => println!("{}", s.to_string_lossy()),
            Primitive::Integer(i) => println!("{}", i),
            Primitive::Name(name) => println!("{}", name),
            Primitive::Boolean(b) => println!("{}", b),
            _ => println!("{:?}", field.value),
        }
        
        // Field properties
        if let Some(field_type) = &field.field_type {
            println!("  Type: {:?}", field_type);
        }
        if let Some(flags) = field.field_flags {
            println!("  Flags: {:?}", flags);
        }
    }
}
```

#### Handling Annotations

```rust
use pdf::object::*;

for page in file.pages() {
    let page = page?;
    
    if let Some(annotations) = &page.annotations {
        for annotation_ref in annotations {
            if let Ok(annotation) = file.resolver().get(*annotation_ref) {
                match &**annotation {
                    Annotation::Text(text_annot) => {
                        println!("Text annotation: {:?}", text_annot.contents);
                    },
                    Annotation::Link(link_annot) => {
                        println!("Link annotation: {:?}", link_annot.action);
                    },
                    Annotation::Highlight(highlight) => {
                        println!("Highlight annotation");
                    },
                    _ => println!("Other annotation type"),
                }
            }
        }
    }
}
```

### Navigation Features

#### Working with Outlines (Bookmarks)

```rust
use pdf::object::*;

if let Some(outlines) = &file.get_root().outlines {
    fn print_outline(outline: &Outline, level: usize, resolver: &impl pdf::object::Resolve) {
        let indent = "  ".repeat(level);
        
        if let Some(title) = &outline.title {
            println!("{}Outline: {}", indent, title.to_string_lossy());
        }
        
        // Process destination
        if let Some(dest) = &outline.dest {
            println!("{}  Destination: {:?}", indent, dest);
        }
        
        // Process child outlines
        if let Some(first_child) = &outline.first {
            if let Ok(child) = resolver.get(*first_child) {
                print_outline(&child, level + 1, resolver);
            }
        }
        
        // Process sibling outlines
        if let Some(next_sibling) = &outline.next {
            if let Ok(sibling) = resolver.get(*next_sibling) {
                print_outline(&sibling, level, resolver);
            }
        }
    }
    
    if let Some(first_outline) = &outlines.first {
        if let Ok(outline) = file.resolver().get(*first_outline) {
            print_outline(&outline, 0, &file.resolver());
        }
    }
}
```

#### Named Destinations

```rust
use pdf::object::*;

if let Some(names) = &file.get_root().names {
    if let Some(dests) = &names.dests {
        println!("Named destinations:");
        
        // Process destination names
        for (name, dest) in dests.iter() {
            println!("  {}: {:?}", name, dest);
        }
    }
}
```

### Low-level Operations

#### Working with Primitives

```rust
use pdf::primitive::*;
use pdf::object::*;

// Create and manipulate PDF primitives
let name = Name::from("MyName");
let string = PdfString::from("Hello, PDF!");
let array = vec![
    Primitive::Integer(42),
    Primitive::Real(3.14),
    Primitive::String(string),
];

// Dictionary operations
let mut dict = Dictionary::new();
dict.insert("Type", Primitive::Name("Page".into()));
dict.insert("MediaBox", Primitive::Array(vec![
    Primitive::Integer(0),
    Primitive::Integer(0),
    Primitive::Integer(612),
    Primitive::Integer(792),
]));
```

#### Stream Processing

```rust
use pdf::object::*;
use pdf::enc::StreamFilter;

// Create a stream with compression
let data = b"Hello, PDF Stream!";
let mut stream_dict = StreamDict::default();
stream_dict.filter = Some(StreamFilter::FlateDecode(Default::default()));

let stream = Stream::new(stream_dict, data.to_vec());

// Decode stream data
let decoded_data = stream.decode()?;
println!("Decoded: {}", String::from_utf8_lossy(&decoded_data));
```

#### Custom Content Operations

```rust
use pdf::content::*;

// Create complex drawing operations
let operations = vec![
    Op::Save,
    Op::SetLineWidth { width: 2.0 },
    Op::SetStrokeColor { color: Color::Gray(0.5) },
    Op::MoveTo { p: Point { x: 100., y: 100. } },
    Op::CurveTo { 
        c1: Point { x: 150., y: 150. },
        c2: Point { x: 200., y: 150. },
        p: Point { x: 250., y: 100. }
    },
    Op::Stroke,
    Op::Restore,
];

let content = Content::from_ops(operations);
```

## Examples

The repository includes comprehensive examples demonstrating various use cases:

### Core Examples (pdf/examples/)

- [`read.rs`](pdf/examples/read.rs:1) - Complete PDF reading with image and font extraction
- [`content.rs`](pdf/examples/content.rs:1) - Creating PDFs with drawing operations
- [`metadata.rs`](pdf/examples/metadata.rs:1) - Working with document metadata
- [`names.rs`](pdf/examples/names.rs:1) - Handling named destinations and navigation
- [`other_page_content.rs`](pdf/examples/other_page_content.rs:1) - Advanced page content manipulation

### Extended Examples (examples/src/bin/)

- [`add_image.rs`](examples/src/bin/add_image.rs:1) - Adding images to existing PDFs
- [`extract_page.rs`](examples/src/bin/extract_page.rs:1) - Extracting individual pages
- [`form.rs`](examples/src/bin/form.rs:1) - Working with PDF forms and fields

### Running Examples

```bash
# Core examples
cargo run --example read -- files/example.pdf
cargo run --example content -- output.pdf
cargo run --example metadata -- files/example.pdf

# Extended examples  
cargo run --package examples --bin add_image -- --input files/example.pdf --image image.jpg --output result.pdf
cargo run --package examples --bin extract_page -- --input files/example.pdf --page 0 --output page.pdf
```

## Workspace Structure

This repository uses a Cargo Workspace with the following structure:

```
pdf-rs/
├── pdf/                    # Main PDF library crate
│   ├── src/               # Core library source code
│   ├── examples/          # Basic usage examples
│   └── tests/             # Integration tests
├── pdf_derive/            # Procedural macros for PDF objects
├── examples/              # Extended examples and utilities
│   └── src/bin/          # Command-line example applications
└── files/                 # Test PDF files
```

### Building Specific Components

By default, only the `pdf` library is built. To build additional components:

```bash
# Build specific package
cargo build --package pdf
cargo build --package examples
cargo build --package pdf_derive

# Run tests for specific package
cargo test --package pdf
```

## Contributing

We welcome contributions! Here are several ways you can help:

### Easy Contributions
- Add different PDF files to `files/` directory and test with `cargo test`
- Report issues with specific PDF files that don't parse correctly
- Improve documentation and examples

### Development Contributions
- Implement missing PDF features
- Improve error handling and recovery
- Optimize performance for large files
- Add support for newer PDF specifications

### Getting Help
Feel free to contribute with ideas, issues, or code! Please join [us on Zulip](https://type.zulipchat.com/#narrow/stream/209232-pdf) if you have any questions or problems.

### Development Setup

```bash
# Clone the repository
git clone https://github.com/pdf-rs/pdf.git
cd pdf

# Run tests
cargo test

# Run examples
cargo run --example read -- files/example.pdf

# Check formatting
cargo fmt --all

# Run clippy
cargo clippy --all-targets --all-features
```

## Related Projects

### Renderer and Viewer
A library for rendering PDFs via [Pathfinder](https://github.com/servo/pathfinder) and minimal viewer can be found at [pdf_render](https://github.com/pdf-rs/pdf_render).

### PDF Inspector
There is a tool for visualizing a PDF file as an interactive hierarchy of primitives at [inspect-prim](https://github.com/pdf-rs/inspect-prim). Just clone and `cargo run`.

### Community Projects
- PDF manipulation tools built with pdf-rs
- Integration examples with web frameworks
- Command-line utilities for PDF processing

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

---

**Note**: PDF modification and writing features are still experimental. While the library provides robust reading capabilities, use caution when modifying important documents and always keep backups.

For the latest updates and detailed API documentation, visit [docs.rs/pdf](https://docs.rs/pdf).