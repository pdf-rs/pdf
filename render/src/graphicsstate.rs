use pdf::object::ColorSpace;

use pathfinder_geometry::transform2d::Transform2F;
use pathfinder_content::{
    fill::FillRule,
    stroke::{StrokeStyle, OutlineStrokeToFill},
    outline::Outline,
};
use pathfinder_renderer::{
    scene::{DrawPath, ClipPathId, Scene},
    paint::{PaintId},
};

#[derive(Copy, Clone)]
pub struct GraphicsState<'a> {
    pub transform: Transform2F,
    pub stroke_style: StrokeStyle,
    pub fill_paint: PaintId,
    pub stroke_paint: PaintId,
    pub clip_path: Option<ClipPathId>,
    pub fill_color_space: &'a ColorSpace,
    pub stroke_color_space: &'a ColorSpace,
}

#[derive(Copy, Clone)]
pub enum DrawMode {
    Fill,
    Stroke,
    FillStroke,
    StrokeFill,
}

impl<'a> GraphicsState<'a> {
    pub fn draw(&self, scene: &mut Scene, outline: &Outline, mode: DrawMode, fill_rule: FillRule) {
        self.draw_transform(scene, outline, mode, fill_rule, Transform2F::default());
    }
    pub fn draw_transform(&self, scene: &mut Scene, outline: &Outline, mode: DrawMode, fill_rule: FillRule, transform: Transform2F) {
        let tr = self.transform * transform;
        let fill = |scene: &mut Scene| {
            let mut draw_path = DrawPath::new(outline.clone().transformed(&tr), self.fill_paint);
            draw_path.set_clip_path(self.clip_path);
            draw_path.set_fill_rule(fill_rule);
            scene.push_draw_path(draw_path);
        };

        if matches!(mode, DrawMode::Fill | DrawMode::FillStroke) {
            fill(scene);
        }
        if matches!(mode, DrawMode::Stroke | DrawMode::FillStroke) {
            let mut stroke = OutlineStrokeToFill::new(outline, self.stroke_style);
            stroke.offset();
            let mut draw_path = DrawPath::new(stroke.into_outline().transformed(&tr), self.stroke_paint);
            draw_path.set_clip_path(self.clip_path);
            draw_path.set_fill_rule(fill_rule);
            scene.push_draw_path(draw_path);
        }
        if matches!(mode, DrawMode::StrokeFill) {
            fill(scene);
        }
    }
}
