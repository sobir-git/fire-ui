use crate::NativeText;
use femtovg::{renderer::OpenGl, Canvas};
use fire_ui::*;
pub(crate) struct GlPainter<'a> {
    pub(crate) canvas: &'a mut Canvas<OpenGl>,
    pub(crate) text: &'a NativeText,
    clip: Rect,
    clips: Vec<Rect>,
}
impl<'a> GlPainter<'a> {
    pub(crate) fn new(
        canvas: &'a mut Canvas<OpenGl>,
        text: &'a NativeText,
        viewport: Rect,
    ) -> Self {
        Self {
            canvas,
            text,
            clip: viewport,
            clips: vec![],
        }
    }
}
fn color(c: Color) -> femtovg::Color {
    femtovg::Color::rgbaf(c.0, c.1, c.2, c.3)
}
fn brush(b: Brush) -> femtovg::Paint {
    match b {
        Brush::Solid(c) => femtovg::Paint::color(color(c)),
        Brush::Linear {
            start,
            end,
            from,
            to,
        } => {
            femtovg::Paint::linear_gradient(start.x, start.y, end.x, end.y, color(from), color(to))
        }
    }
}
impl Painter for GlPainter<'_> {
    fn save(&mut self) {
        self.clips.push(self.clip);
        self.canvas.save()
    }
    fn restore(&mut self) {
        if let Some(clip) = self.clips.pop() {
            self.clip = clip;
        }
        self.canvas.restore()
    }
    fn transform(&mut self, t: Transform) {
        self.canvas.set_transform(&femtovg::Transform2D(t.0))
    }
    fn clip(&mut self, r: Rect) {
        self.clip = self
            .clip
            .intersect(Transform(self.canvas.transform().0).rect(r));
        self.canvas.intersect_scissor(r.x, r.y, r.width, r.height)
    }
    fn rect(&mut self, r: Rect, radius: f32, b: Brush) {
        let mut path = femtovg::Path::new();
        path.rounded_rect(r.x, r.y, r.width, r.height, radius);
        self.canvas.fill_path(&path, &brush(b));
    }
    fn stroke(&mut self, r: Rect, radius: f32, width: f32, c: Color) {
        let mut path = femtovg::Path::new();
        path.rounded_rect(r.x, r.y, r.width, r.height, radius);
        let mut paint = femtovg::Paint::color(color(c));
        paint.set_line_width(width);
        self.canvas.stroke_path(&path, &paint);
    }
    fn path(&mut self, commands: &[Path], b: Brush, width: Option<f32>) {
        let mut path = femtovg::Path::new();
        for command in commands {
            match command {
                Path::Move(p) => path.move_to(p.x, p.y),
                Path::Line(p) => path.line_to(p.x, p.y),
                Path::Curve(a, b, c) => path.bezier_to(a.x, a.y, b.x, b.y, c.x, c.y),
                Path::Close => path.close(),
            }
        }
        let mut paint = brush(b);
        if let Some(width) = width {
            paint.set_line_width(width);
            self.canvas.stroke_path(&path, &paint)
        } else {
            self.canvas.fill_path(&path, &paint)
        }
    }
    fn paragraph(&mut self, p: &Paragraph, origin: Point, b: Brush) {
        let mut paint = brush(b);
        self.text.configure(&mut paint, p.style);
        let Some(inverse) = Transform(self.canvas.transform().0).inverse() else {
            return;
        };
        let visible = inverse.rect(self.clip);
        let top = visible.y - origin.y;
        let bottom = top + visible.height;
        let first = p.lines.partition_point(|line| line.y + p.line_height < top);
        for line in p.lines[first..].iter().take_while(|line| line.y < bottom) {
            let _ = self.canvas.fill_text(
                origin.x,
                origin.y + line.y,
                &p.text[line.range.clone()],
                &paint,
            );
        }
    }
    fn custom(&mut self, paint: &dyn CustomPaint) -> bool {
        paint.paint(self.canvas)
    }
}
