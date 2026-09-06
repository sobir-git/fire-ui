use femtovg::{renderer::OpenGl, Canvas, FontId, Paint, Path, TextContext};
use fire_ui::*;
use unicode_segmentation::UnicodeSegmentation;

pub struct NativeText {
    pub context: TextContext,
    fonts: Vec<FontId>,
}
impl NativeText {
    pub fn new() -> Result<Self, String> {
        let context = TextContext::default();
        let mut fonts = vec![];
        let candidates = [
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "C:\\Windows\\Fonts\\segoeui.ttf",
        ];
        if let Ok(path) = std::env::var("FIRE_UI_FONT") {
            fonts.push(context.add_font_file(path).map_err(|e| e.to_string())?);
        }
        if fonts.is_empty() {
            for path in candidates {
                if let Ok(id) = context.add_font_file(path) {
                    fonts.push(id);
                    break;
                }
            }
        }
        if fonts.is_empty() {
            return Err("No font found. Set FIRE_UI_FONT to a TrueType font path.".into());
        }
        Ok(Self { context, fonts })
    }
    pub fn add_font(&mut self, path: impl AsRef<std::path::Path>) -> Result<u32, String> {
        let id = self
            .context
            .add_font_file(path)
            .map_err(|e| e.to_string())?;
        self.fonts.push(id);
        Ok((self.fonts.len() - 1) as u32)
    }
    fn configure(&self, paint: &mut Paint, style: TextStyle) {
        let index = (style.font as usize).min(self.fonts.len() - 1);
        paint.set_font(&self.fonts[index..]);
        paint.set_font_size(style.size);
        paint.set_text_baseline(femtovg::Baseline::Top);
    }
}
impl TextEngine for NativeText {
    fn measure(&mut self, text: &str, style: TextStyle) -> LineMetrics {
        let mut paint = Paint::default();
        self.configure(&mut paint, style);
        let Ok(metrics) = self.context.measure_text(0., 0., text, &paint) else {
            return LineMetrics::default();
        };
        let width = metrics.width();
        let mut clusters: Vec<(usize, f32)> = metrics
            .glyphs
            .iter()
            .map(|g| (g.byte_index, g.x - g.bearing_x - g.offset_x))
            .collect();
        // Shaped LTR runs are already ordered. Sort only reordered runs, such as RTL.
        if !clusters.windows(2).all(|pair| pair[0].0 <= pair[1].0) {
            clusters.sort_by_key(|(byte, _)| *byte);
        }
        clusters.dedup_by_key(|(byte, _)| *byte);
        if clusters.first().is_none_or(|(b, _)| *b != 0) {
            clusters.insert(0, (0, 0.));
        }
        clusters.push((text.len(), width));
        let ascii = text.is_ascii();
        let mut carets = Vec::with_capacity(clusters.len());
        let mut at = 0;
        let mut push_caret = |byte| {
            while at + 1 < clusters.len() && clusters[at].0 < byte {
                at += 1;
            }
            let x = if clusters.get(at).is_some_and(|(b, _)| *b == byte) {
                clusters[at].1
            } else {
                let (before, x1) = clusters[at.saturating_sub(1)];
                let (after, x2) = clusters.get(at).copied().unwrap_or((text.len(), width));
                let portion = if ascii {
                    (byte - before) as f32 / (after - before).max(1) as f32
                } else {
                    let total = text[before..after].graphemes(true).count().max(1) as f32;
                    text[before..byte].graphemes(true).count() as f32 / total
                };
                x1 + (x2 - x1) * portion
            };
            carets.push((byte, x));
        };
        if ascii && !text.contains("\r\n") {
            for byte in 0..=text.len() {
                push_caret(byte);
            }
        } else {
            for byte in text
                .grapheme_indices(true)
                .map(|(byte, _)| byte)
                .chain(std::iter::once(text.len()))
            {
                push_caret(byte);
            }
        }
        LineMetrics { width, carets }
    }
}

pub struct GlPainter<'a> {
    canvas: &'a mut Canvas<OpenGl>,
    text: &'a NativeText,
    clip: Option<Rect>,
    clips: Vec<Option<Rect>>,
}
impl<'a> GlPainter<'a> {
    /// Begin painting after resetting canvas state and selecting the render target.
    pub fn new(canvas: &'a mut Canvas<OpenGl>, text: &'a NativeText) -> Self {
        let clip = Some(Rect::new(
            0.,
            0.,
            canvas.width() as f32,
            canvas.height() as f32,
        ));
        Self {
            canvas,
            text,
            clip,
            clips: Vec::new(),
        }
    }
    fn device_rect(&self, r: Rect) -> Option<Rect> {
        let m = self.canvas.transform().0;
        if m[1] != 0. || m[2] != 0. || m[0] <= 0. || m[3] <= 0. {
            return None;
        }
        Some(Rect::new(
            r.x * m[0] + m[4],
            r.y * m[3] + m[5],
            r.width * m[0],
            r.height * m[3],
        ))
    }
}
fn intersection(a: Rect, b: Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    Rect::new(
        x,
        y,
        (a.x + a.width).min(b.x + b.width).max(x) - x,
        (a.y + a.height).min(b.y + b.height).max(y) - y,
    )
}
fn pixel_aligned(r: Rect) -> bool {
    [r.x, r.y, r.x + r.width, r.y + r.height]
        .iter()
        .all(|v| v.is_finite() && v.fract() == 0.)
}
fn color(c: Color) -> femtovg::Color {
    femtovg::Color::rgbaf(c.0, c.1, c.2, c.3)
}
fn brush(b: Brush) -> Paint {
    match b {
        Brush::Solid(c) => Paint::color(color(c)),
        Brush::Linear {
            start,
            end,
            from,
            to,
        } => Paint::linear_gradient(start.x, start.y, end.x, end.y, color(from), color(to)),
    }
}
impl Painter for GlPainter<'_> {
    fn save(&mut self) {
        self.clips.push(self.clip);
        self.canvas.save();
    }
    fn restore(&mut self) {
        self.clip = self.clips.pop().unwrap_or(None);
        self.canvas.restore();
    }
    fn translate(&mut self, p: Point) {
        self.canvas.translate(p.x, p.y);
    }
    fn transform(&mut self, m: [f32; 6]) {
        self.canvas.set_transform(&femtovg::Transform2D(m));
    }
    fn clip(&mut self, r: Rect) {
        self.clip = self
            .clip
            .zip(self.device_rect(r))
            .map(|(a, b)| intersection(a, b));
        self.canvas.intersect_scissor(r.x, r.y, r.width, r.height);
    }
    fn rect(&mut self, r: Rect, radius: f32, b: Brush) {
        // Opaque, pixel-aligned backgrounds need no blending or edge shader.
        // Keep the path renderer for rounded, translucent, transformed, or fractional edges.
        if radius == 0. {
            if let Brush::Solid(c) = b {
                if c.3 == 1. {
                    if let Some((clip, device)) = self.clip.zip(self.device_rect(r)) {
                        if pixel_aligned(clip) && pixel_aligned(device) {
                            let r = intersection(clip, device);
                            self.canvas.clear_rect(
                                r.x as u32,
                                r.y as u32,
                                r.width as u32,
                                r.height as u32,
                                color(c),
                            );
                            return;
                        }
                    }
                }
            }
        }
        let mut p = Path::new();
        p.rounded_rect(r.x, r.y, r.width, r.height, radius);
        self.canvas.fill_path(&p, &brush(b));
    }
    fn stroke(&mut self, r: Rect, radius: f32, width: f32, c: Color) {
        let mut p = Path::new();
        p.rounded_rect(r.x, r.y, r.width, r.height, radius);
        let mut paint = Paint::color(color(c));
        paint.set_line_width(width);
        self.canvas.stroke_path(&p, &paint);
    }
    fn path(&mut self, commands: &[PathCommand], b: Brush, stroke: Option<f32>) {
        let mut path = Path::new();
        for c in commands {
            match c {
                PathCommand::Move(p) => path.move_to(p.x, p.y),
                PathCommand::Line(p) => path.line_to(p.x, p.y),
                PathCommand::Curve(a, b, c) => path.bezier_to(a.x, a.y, b.x, b.y, c.x, c.y),
                PathCommand::Close => path.close(),
            }
        }
        let mut paint = brush(b);
        if let Some(w) = stroke {
            paint.set_line_width(w);
            self.canvas.stroke_path(&path, &paint);
        } else {
            self.canvas.fill_path(&path, &paint);
        }
    }
    fn text(&mut self, text: &str, origin: Point, style: TextStyle, b: Brush) {
        let mut paint = brush(b);
        self.text.configure(&mut paint, style);
        let _ = self.canvas.fill_text(origin.x, origin.y, text, &paint);
    }
    fn custom(&mut self, paint: &dyn CustomPaint) -> bool {
        self.canvas.save();
        let supported = paint.paint(self.canvas);
        self.canvas.restore();
        // A custom backend operation may alter state outside the portable painter protocol.
        self.clip = None;
        supported
    }
}
