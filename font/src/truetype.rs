use std::error::Error;
use pathfinder_canvas::Path2D;
use pathfinder_geometry::vector::Vector2F;
use pathfinder_geometry::transform2d::Transform2F;
use stb_truetype::FontInfo;
use stb_truetype::VertexType;
use crate::{Font, Glyph};

pub struct TrueTypeFont<'a> {
    pub info: FontInfo<&'a [u8]>
}
impl<'a> TrueTypeFont<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, Box<dyn Error>> {
        let info = FontInfo::new(data, 0).expect("can't pase font");
        Ok(TrueTypeFont { info })
    }
}
impl<'a> Font for TrueTypeFont<'a> {
    fn num_glyphs(&self) -> u32 {
        self.info.get_num_glyphs()
    }
    fn font_matrix(&self) -> Transform2F {
        let scale = 1.0 / self.info.units_per_em() as f32;
        Transform2F::row_major(scale, 0., 0., scale, 0., 0.)
    }
    fn glyph(&self, id: u32) -> Result<Glyph, Box<dyn Error>> {
        let mut path = Path2D::new();
        
        if let Some(shape) = self.info.get_glyph_shape(id) {
            for vertex in shape {
                let p = Vector2F::new(vertex.x as _, vertex.y as _);
                
                match vertex.vertex_type() {
                    VertexType::MoveTo => path.move_to(p),
                    VertexType::LineTo => path.line_to(p),
                    VertexType::CurveTo => {
                        let c = Vector2F::new(vertex.cx as _, vertex.cy as _);
                        path.quadratic_curve_to(c, p);
                    }
                }
            }
            path.close_path();
        }
        let width = self.info.get_glyph_h_metrics(id).advance_width as f32 / self.info.units_per_em() as f32;
        
        Ok(Glyph {
            width,
            path
        })
    }
}

