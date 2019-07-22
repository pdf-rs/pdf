use std::fs::{self, File};
use std::io::BufWriter;
use std::env;
use std::error::Error;
use pathfinder_canvas::{CanvasFontContext, CanvasRenderingContext2D};
use pathfinder_geometry::vector::Vector2F;
use pathfinder_geometry::transform2d::Transform2F;
use pathfinder_export::{Export, FileFormat};
use font::{Font, TrueTypeFont, CffFont};

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let font_data = fs::read(args[1].as_str())?;
    let font: Box<dyn Font> = match args[2].as_str() {
        "cff" => Box::new(CffFont::parse(&font_data, 0)?) as _,
        "otf" => Box::new(CffFont::parse_opentype(&font_data, 0)?) as _,
        "tt" => Box::new(TrueTypeFont::parse(&font_data)?) as _,
        _ => panic!("unsupported format")
    };
    let gid = args[3].parse().expect("not a number");
    
    let font_context = CanvasFontContext::from_system_source();
    let mut canvas = CanvasRenderingContext2D::new(font_context, Vector2F::new(1000.0, 1000.0));
    
    let transform = Transform2F::from_translation(Vector2F::new(0., 1000.)) * Transform2F::from_scale(Vector2F::new(1.0, -1.0));
    canvas.set_current_transform(&transform);
    canvas.fill_path(font.glyph(gid)?.path);
    canvas.into_scene().export(&mut BufWriter::new(File::create("glyph.svg")?), FileFormat::SVG)?;
    
    Ok(())
}
