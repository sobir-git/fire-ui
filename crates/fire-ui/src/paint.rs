use crate::{Point, Rect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color(pub f32, pub f32, pub f32, pub f32);
impl Color {
    pub const fn hex(rgb: u32) -> Self {
        Self(
            ((rgb >> 16) & 255) as f32 / 255.,
            ((rgb >> 8) & 255) as f32 / 255.,
            (rgb & 255) as f32 / 255.,
            1.,
        )
    }
    pub fn alpha(self, alpha: f32) -> Self {
        Self(self.0, self.1, self.2, alpha)
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
pub enum PathCommand {
    Move(Point),
    Line(Point),
    Curve(Point, Point, Point),
    Close,
}

/// A font handle belongs to the host's text service. Zero is the default font.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub font: u32,
}
impl Default for TextStyle {
    fn default() -> Self {
        Self { size: 15., font: 0 }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LineMetrics {
    pub width: f32,
    pub carets: Vec<(usize, f32)>,
}
impl LineMetrics {
    pub fn x(&self, byte: usize) -> f32 {
        self.carets
            .binary_search_by_key(&byte, |(b, _)| *b)
            .map(|i| self.carets[i].1)
            .unwrap_or(self.width)
    }
    pub fn hit(&self, x: f32) -> usize {
        self.carets
            .iter()
            .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
            .map(|(b, _)| *b)
            .unwrap_or(0)
    }
}

/// Shaping and paint must use the same fonts and metrics. Hosts can replace both.
pub trait TextEngine {
    fn measure(&mut self, text: &str, style: TextStyle) -> LineMetrics;
}

/// Low-level drawing protocol. Widgets can implement arbitrary 2D visuals.
/// Calls are streamed to the backend; there is no per-frame owned scene copy.
pub trait Painter {
    fn save(&mut self);
    fn restore(&mut self);
    fn translate(&mut self, offset: Point);
    fn transform(&mut self, matrix: [f32; 6]);
    fn clip(&mut self, rect: Rect);
    fn rect(&mut self, rect: Rect, radius: f32, brush: Brush);
    fn stroke(&mut self, rect: Rect, radius: f32, width: f32, color: Color);
    fn path(&mut self, path: &[PathCommand], brush: Brush, stroke: Option<f32>);
    /// Position is the top-left of a line box, not the baseline.
    fn text(&mut self, text: &str, origin: Point, style: TextStyle, brush: Brush);
    /// Optional backend-specific escape hatch. Unsupported backends return false.
    fn custom(&mut self, _paint: &dyn CustomPaint) -> bool {
        false
    }
}

/// A widget can downcast the backend here without putting GPU types in the core.
pub trait CustomPaint {
    fn paint(&self, backend: &mut dyn std::any::Any) -> bool;
}

/// Deterministic text metrics for headless behavioral tests, not a production font.
pub struct TestText;
impl TextEngine for TestText {
    fn measure(&mut self, text: &str, style: TextStyle) -> LineMetrics {
        let mut carets = Vec::new();
        let mut x = 0.;
        for (byte, _) in text.char_indices() {
            carets.push((byte, x));
            x += style.size * 0.6;
        }
        carets.push((text.len(), x));
        LineMetrics { width: x, carets }
    }
}
