#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}
impl Size {
    pub const ZERO: Self = Self::new(0., 0.);
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
    pub fn valid(self) -> bool {
        self.width.is_finite() && self.height.is_finite() && self.width >= 0. && self.height >= 0.
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
    pub fn from_size(size: Size) -> Self {
        Self::new(0., 0., size.width, size.height)
    }
    pub fn size(self) -> Size {
        Size::new(self.width, self.height)
    }
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.x + self.width && p.y < self.y + self.height
    }
    pub fn inset(self, n: f32) -> Self {
        Self::new(
            self.x + n,
            self.y + n,
            (self.width - 2. * n).max(0.),
            (self.height - 2. * n).max(0.),
        )
    }
    pub fn intersect(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Self::new(
            x,
            y,
            (self.x + self.width).min(other.x + other.width).max(x) - x,
            (self.y + self.height).min(other.y + other.height).max(y) - y,
        )
    }
    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self::new(
            x,
            y,
            (self.x + self.width).max(other.x + other.width) - x,
            (self.y + self.height).max(other.y + other.height) - y,
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform(pub [f32; 6]);
impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}
impl Transform {
    pub const IDENTITY: Self = Self([1., 0., 0., 1., 0., 0.]);
    pub fn translate(x: f32, y: f32) -> Self {
        Self([1., 0., 0., 1., x, y])
    }
    pub fn scale(x: f32, y: f32) -> Self {
        Self([x, 0., 0., y, 0., 0.])
    }
    pub fn rotate(r: f32) -> Self {
        let (s, c) = r.sin_cos();
        Self([c, s, -s, c, 0., 0.])
    }
    /// Composition: apply rhs locally, then self.
    pub fn compose(self, rhs: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [g, h, i, j, k, l] = rhs.0;
        Self([
            a * g + c * h,
            b * g + d * h,
            a * i + c * j,
            b * i + d * j,
            a * k + c * l + e,
            b * k + d * l + f,
        ])
    }
    pub fn point(self, p: Point) -> Point {
        let [a, b, c, d, e, f] = self.0;
        Point::new(a * p.x + c * p.y + e, b * p.x + d * p.y + f)
    }
    pub fn inverse(self) -> Option<Self> {
        let [a, b, c, d, e, f] = self.0;
        let det = a * d - b * c;
        if !self.0.iter().all(|v| v.is_finite()) || !det.is_finite() || det.abs() < 1e-8 {
            return None;
        }
        {
            let inverse = Self([
                d / det,
                -b / det,
                -c / det,
                a / det,
                (c * f - d * e) / det,
                (b * e - a * f) / det,
            ]);
            inverse.0.iter().all(|v| v.is_finite()).then_some(inverse)
        }
    }
    pub fn rect(self, r: Rect) -> Rect {
        let points = [
            self.point(Point::new(r.x, r.y)),
            self.point(Point::new(r.x + r.width, r.y)),
            self.point(Point::new(r.x, r.y + r.height)),
            self.point(Point::new(r.x + r.width, r.y + r.height)),
        ];
        let x = points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let y = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        Rect::new(
            x,
            y,
            points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max) - x,
            points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max) - y,
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Constraints {
    pub min: Size,
    pub max: Size,
}
impl Constraints {
    pub fn tight(size: Size) -> Self {
        assert!(size.valid());
        Self {
            min: size,
            max: size,
        }
    }
    pub fn loose(max: Size) -> Self {
        assert!(max.width >= 0. && max.height >= 0.);
        Self {
            min: Size::ZERO,
            max,
        }
    }
    pub fn constrain(self, s: Size) -> Size {
        Size::new(
            s.width.max(self.min.width).min(self.max.width),
            s.height.max(self.min.height).min(self.max.height),
        )
    }
    pub fn inset(self, n: f32) -> Self {
        Self {
            min: Size::new(
                (self.min.width - 2. * n).max(0.),
                (self.min.height - 2. * n).max(0.),
            ),
            max: Size::new(
                (self.max.width - 2. * n).max(0.),
                (self.max.height - 2. * n).max(0.),
            ),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    pub size: Size,
    pub baseline: Option<f32>,
}
impl Metrics {
    pub fn new(size: Size) -> Self {
        Self {
            size,
            baseline: None,
        }
    }
}
