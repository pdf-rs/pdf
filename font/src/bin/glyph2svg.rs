use std::fs::{self, File};
use std::io::BufWriter;
use std::env;
use std::error::Error;
use pathfinder_canvas::{CanvasFontContext, CanvasRenderingContext2D};
use pathfinder_geometry::vector::Vector2F;
use pathfinder_geometry::transform2d::Transform2F;
use pathfinder_export::{Export, FileFormat};
use font::{Font, parse_file};

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let font = parse_file(&args[1]).unwrap();
    let gid = args[2].parse().ok().or_else(||
        font.gid_for_name(&args[2])
    ).expect("not a number or valid glyph name");
    
    let font_context = CanvasFontContext::from_system_source();
    let mut canvas = CanvasRenderingContext2D::new(font_context, Vector2F::new(1000.0, 1000.0));
    
    let transform = Transform2F::from_translation(Vector2F::new(0., 1000.)) * Transform2F::from_scale(Vector2F::new(1.0, -1.0));
    canvas.set_current_transform(&transform);
    canvas.fill_path(font.glyph(gid)?.path);
    canvas.into_scene().export(&mut BufWriter::new(File::create("glyph.svg")?), FileFormat::SVG)?;
    
    Ok(())
}
