use std::fs::{self, File};
use std::io::BufWriter;
use std::env;
use std::error::Error;
use pathfinder_canvas::{CanvasFontContext, CanvasRenderingContext2D};
use pathfinder_geometry::vector::Vector2F;
use pathfinder_geometry::transform2d::Transform2F;
use pathfinder_export::{Export, FileFormat};
use font::{Font, TrueTypeFont, CffFont, Type1Font};

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let font_data = fs::read(args[1].as_str())?;
    let font: Box<dyn Font> = match args[2].as_str() {
        "cff" => Box::new(CffFont::parse(&font_data, 0)?) as _,
        "otf" => Box::new(CffFont::parse_opentype(&font_data, 0)?) as _,
        "tt" => Box::new(TrueTypeFont::parse(&font_data)?) as _,
        "type1" => Box::new(Type1Font::parse(&font_data)?) as _,
        _ => panic!("unsupported format")
    };
    
    let num_glyphs = font.num_glyphs();
    let aspect_ratio = 4. / 3.; // width to height
    let glyphs_x = (num_glyphs as f32 * aspect_ratio).sqrt().ceil() as u32;
    let glyphs_y = (num_glyphs + glyphs_x - 1) / glyphs_x;
    let size = Vector2F::new(glyphs_x as f32, glyphs_y as f32);
    
    println!("{} glyphs in {} by {}", num_glyphs, glyphs_x, glyphs_y);
    
    let font_context = CanvasFontContext::from_system_source();
    let mut canvas = CanvasRenderingContext2D::new(font_context, size);
    
    for gid in 0 .. num_glyphs {
        let y = (gid as u32 / glyphs_x);
        let x = (gid as u32 % glyphs_x);
        let offset = Vector2F::new(x as f32, (y + 1) as f32);
        let transform = Transform2F::from_translation(offset) * Transform2F::from_scale(Vector2F::new(1.0, -1.0)) * font.font_matrix();
        canvas.set_current_transform(&transform);
    
        canvas.fill_path(font.glyph(gid)?.path);
    }
    canvas.into_scene().export(&mut BufWriter::new(File::create("font.svg")?), FileFormat::SVG)?;
    
    Ok(())
}
