extern crate pdf;

use std::env::args;
use std::collections::HashMap;
use std::convert::TryInto;

use pdf::file::File;
use pdf::content::*;
use pdf::font::*;
use pdf::object::{Resolve, RcRef};
use pdf::encoding::BaseEncoding;
use pdf::error::PdfError;

struct FontInfo {
    font: RcRef<Font>,
    cmap: ToUnicodeMap,
}
struct Cache {
    fonts: HashMap<String, FontInfo>
}
impl Cache {
    fn new() -> Self {
        Cache {
            fonts: HashMap::new()
        }
    }
    fn add_font(&mut self, name: impl Into<String>, font: RcRef<Font>) {
        println!("add_font({:?})", font);
        if let Some(to_unicode) = font.to_unicode() {
            self.fonts.insert(name.into(), FontInfo { font, cmap: to_unicode.unwrap() });
        }
    }
    fn get_font(&self, name: &str) -> Option<&FontInfo> {
        self.fonts.get(name)
    }
}

fn add_string(data: &[u8], out: &mut String, info: &FontInfo) {
    if let Some(encoding) = info.font.encoding() {
        match encoding.base {
            BaseEncoding::IdentityH => {
                for w in data.windows(2) {
                    let cp = u16::from_be_bytes(w.try_into().unwrap());
                    if let Some(s) = info.cmap.get(cp) {
                        out.push_str(s);
                    }
                }
            }
            _ => {
                for &b in data {
                    if let Some(s) = info.cmap.get(b as u16) {
                        out.push_str(s);
                    } else {
                        out.push(b as char);
                    }
                }
            }
        };
    }
}

fn main() -> Result<(), PdfError> {
    let path = args().nth(1).expect("no file given");
    println!("read: {}", path);
    let file = File::<Vec<u8>>::open(&path).unwrap();
    
    let mut out = String::new();
    for page in file.pages() {
        let page = page?;
        let resources = page.resources.as_ref().unwrap();
        let mut cache = Cache::new();
        
        // make sure all fonts are in the cache, so we can reference them
        for (name, &font) in &resources.fonts {
            cache.add_font(name, file.get(font)?);
        }
        for gs in resources.graphics_states.values() {
            if let Some((font, _)) = gs.font {
                let font = file.get(font)?;
                if let Some(font_name) = &font.name {
                    cache.add_font(font_name.clone(), font);
                }
            }
        }
        let mut current_font = None;
        let contents = page.contents.as_ref().unwrap();
        for op in contents.operations(&file)?.iter() {
            match op {
                Op::GraphicsState { name } => {
                    let gs = resources.graphics_states.get(name).unwrap();
                    
                    if let Some((font, _)) = gs.font {
                        let font = file.get(font)?;
                        if let Some(font_name) = &font.name{
                            current_font = cache.get_font(font_name.as_str());
                        }
                    }
                }
                // text font
                Op::TextFont { name, .. } => {
                    current_font = cache.get_font(name);
                }
                Op::TextDraw { text } => if let Some(font) = current_font {
                    add_string(&text.data, &mut out, font);
                }
                Op::TextDrawAdjusted { array } =>  if let Some(font) = current_font {
                    for data in array {
                        if let TextDrawAdjusted::Text(text) = data {
                            add_string(&text.data, &mut out, font);
                        }
                    }
                }
                Op::TextNewline => {
                    out.push('\n');
                }
                _ => {}
            }
        }
    }
    println!("{}", out);

    Ok(())
}
