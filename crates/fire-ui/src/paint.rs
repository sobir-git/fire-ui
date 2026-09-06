use crate::{Point, Rect, Transform};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color(pub f32, pub f32, pub f32, pub f32);
impl Color {
    pub const fn hex(n: u32) -> Self {
        Self(
            ((n >> 16) & 255) as f32 / 255.,
            ((n >> 8) & 255) as f32 / 255.,
            (n & 255) as f32 / 255.,
            1.,
        )
    }
    pub fn alpha(self, a: f32) -> Self {
        Self(self.0, self.1, self.2, a)
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Brush {
    Solid(Color),
    Linear {
        start: Point,
        end: Point,
        from: Color,
        to: Color,
    },
}
impl From<Color> for Brush {
    fn from(c: Color) -> Self {
        Self::Solid(c)
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Path {
    Move(Point),
    Line(Point),
    Curve(Point, Point, Point),
    Close,
}
/// Stream drawing into a host backend. Transform composes with the current transform.
pub trait Painter {
    fn save(&mut self);
    fn restore(&mut self);
    fn transform(&mut self, transform: Transform);
    fn clip(&mut self, rect: Rect);
    fn rect(&mut self, rect: Rect, radius: f32, brush: Brush);
    fn stroke(&mut self, rect: Rect, radius: f32, width: f32, color: Color);
    fn path(&mut self, path: &[Path], brush: Brush, width: Option<f32>);
    fn paragraph(&mut self, paragraph: &crate::Paragraph, origin: Point, brush: Brush);
    fn custom(&mut self, _paint: &dyn CustomPaint) -> bool {
        false
    }
}
pub trait CustomPaint {
    fn paint(&self, backend: &mut dyn std::any::Any) -> bool;
}
