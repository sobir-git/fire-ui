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
    /// Concentric fade from `from` at `inner` to `to` at `outer`. Round glows and orbs.
    Radial {
        center: Point,
        inner: f32,
        outer: f32,
        from: Color,
        to: Color,
    },
    /// A rounded rectangle fading outwards over `feather`. Shadows, glows and edge light.
    Box {
        rect: Rect,
        radius: f32,
        feather: f32,
        from: Color,
        to: Color,
    },
}
impl Brush {
    /// The same brush with every colour's alpha multiplied by `alpha`.
    pub fn faded(self, alpha: f32) -> Self {
        let f = |c: Color| Color(c.0, c.1, c.2, c.3 * alpha);
        match self {
            Self::Solid(c) => Self::Solid(f(c)),
            Self::Linear {
                start,
                end,
                from,
                to,
            } => Self::Linear {
                start,
                end,
                from: f(from),
                to: f(to),
            },
            Self::Radial {
                center,
                inner,
                outer,
                from,
                to,
            } => Self::Radial {
                center,
                inner,
                outer,
                from: f(from),
                to: f(to),
            },
            Self::Box {
                rect,
                radius,
                feather,
                from,
                to,
            } => Self::Box {
                rect,
                radius,
                feather,
                from: f(from),
                to: f(to),
            },
        }
    }
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
    /// Multiply the alpha of everything drawn until the matching `restore`.
    /// Nested calls compose multiplicatively. Each draw is faded on its own, so
    /// overlapping shapes inside a faded group show through one another.
    fn opacity(&mut self, alpha: f32);
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
