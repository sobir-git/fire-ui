use femtovg::{renderer::OpenGl, Canvas};
use fire_ui::*;
#[cfg(not(feature = "raster-text"))]
use fire_ui_fonts::Fonts;
pub(crate) struct GlPainter<'a> {
    pub(crate) canvas: &'a mut Canvas<OpenGl>,
    #[cfg(not(feature = "raster-text"))]
    pub(crate) fonts: &'a Fonts,
    #[cfg(feature = "raster-text")]
    pub(crate) font_ids: &'a [femtovg::FontId],
    clip: Rect,
    clips: Vec<Rect>,
    images: Vec<femtovg::ImageId>,
    error: Option<String>,
}
impl<'a> GlPainter<'a> {
    pub(crate) fn status(&self) -> Result<(), String> {
        self.error.clone().map_or(Ok(()), Err)
    }
    pub(crate) fn new(
        canvas: &'a mut Canvas<OpenGl>,
        #[cfg(not(feature = "raster-text"))] fonts: &'a Fonts,
        #[cfg(feature = "raster-text")] font_ids: &'a [femtovg::FontId],
        viewport: Rect,
    ) -> Self {
        Self {
            canvas,
            #[cfg(not(feature = "raster-text"))]
            fonts,
            #[cfg(feature = "raster-text")]
            font_ids,
            clip: viewport,
            clips: vec![],
            images: vec![],
            error: None,
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
        Brush::Radial {
            center,
            inner,
            outer,
            from,
            to,
        } => femtovg::Paint::radial_gradient(
            center.x,
            center.y,
            inner,
            outer.max(inner),
            color(from),
            color(to),
        ),
        Brush::Box {
            rect,
            radius,
            feather,
            from,
            to,
        } => femtovg::Paint::box_gradient(
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            radius,
            feather.max(0.001),
            color(from),
            color(to),
        ),
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
        let paint = brush(b);

        let Some(inverse) = Transform(self.canvas.transform().0).inverse() else {
            return;
        };
        let visible = inverse.rect(self.clip);
        let top = visible.y - origin.y;
        let bottom = top + visible.height;
        let first = p.lines.partition_point(|line| line.y + p.line_height < top);
        for line in p.lines[first..].iter().take_while(|line| line.y < bottom) {
            for run in &line.runs {
                #[cfg(feature = "raster-text")]
                {
                    let Some(font) = self.font_ids.get(run.font as usize).copied() else {
                        self.error = Some(
                            "Paragraph references a font that this painter does not own".into(),
                        );
                        return;
                    };
                    if run.glyphs.iter().any(|glyph| glyph.index > u16::MAX as u32) {
                        self.error = Some("Paragraph references an invalid glyph".into());
                        return;
                    }
                    let y = (origin.y + line.y + p.baseline).round();
                    let glyphs = run.glyphs.iter().map(|glyph| femtovg::PositionedGlyph {
                        x: origin.x + run.x + glyph.offset.x,
                        y: y + glyph.offset.y,
                        glyph_id: glyph.index as u16,
                    });
                    let mut paint = paint.clone();
                    paint.set_font_size(p.style.size);
                    if let Err(error) = self.canvas.fill_glyph_run(font, &[], glyphs, &paint) {
                        self.error = Some(error.to_string());
                        return;
                    }
                    continue;
                }
                #[cfg(not(feature = "raster-text"))]
                {
                    let Some(face) = self.fonts.get(run.font as usize).and_then(|font| {
                        ttf_parser::Face::parse(font.bytes.as_ref(), font.face_index).ok()
                    }) else {
                        self.error = Some(
                            "Paragraph references a font that this painter does not own".into(),
                        );
                        return;
                    };
                    let scale = p.style.size / face.units_per_em() as f32;
                    for glyph in run.glyphs.iter() {
                        if glyph.index >= face.number_of_glyphs() as u32 {
                            self.error = Some("Paragraph references an invalid glyph".into());
                            return;
                        }
                        let mut outline = Outline {
                            path: femtovg::Path::new(),
                            scale,
                            x: origin.x + run.x + glyph.offset.x,
                            y: (origin.y + line.y + p.baseline).round() + glyph.offset.y,
                        };
                        if face
                            .outline_glyph(ttf_parser::GlyphId(glyph.index as u16), &mut outline)
                            .is_some()
                        {
                            self.canvas.fill_path(&outline.path, &paint);
                            continue;
                        }
                        #[cfg(not(feature = "bitmap-fonts"))]
                        if face
                            .glyph_raster_image(
                                ttf_parser::GlyphId(glyph.index as u16),
                                p.style.size.ceil() as u16,
                            )
                            .is_some()
                        {
                            self.error = Some(
                                "Bitmap glyph requires the bitmap-fonts renderer feature".into(),
                            );
                            return;
                        }
                        #[cfg(feature = "bitmap-fonts")]
                        if let Some(bitmap) = face.glyph_raster_image(
                            ttf_parser::GlyphId(glyph.index as u16),
                            p.style.size.ceil() as u16,
                        ) {
                            if bitmap.format != ttf_parser::RasterImageFormat::PNG {
                                self.error = Some("Unsupported bitmap glyph encoding".into());
                                return;
                            }
                            {
                                if bitmap.pixels_per_em == 0 {
                                    self.error = Some("Bitmap glyph has zero pixels per em".into());
                                    return;
                                }
                                match self
                                    .canvas
                                    .load_image_mem(bitmap.data, femtovg::ImageFlags::empty())
                                {
                                    Ok(image) => {
                                        let scale = p.style.size / bitmap.pixels_per_em as f32;
                                        let x = outline.x + bitmap.x as f32 * scale;
                                        let y = outline.y - bitmap.y as f32 * scale;
                                        let w = bitmap.width as f32 * scale;
                                        let h = bitmap.height as f32 * scale;
                                        let mut path = femtovg::Path::new();
                                        path.rect(x, y, w, h);
                                        self.canvas.fill_path(
                                            &path,
                                            &femtovg::Paint::image(image, x, y, w, h, 0., 1.),
                                        );
                                        self.images.push(image);
                                    }
                                    Err(error) => {
                                        self.error = Some(error.to_string());
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    fn custom(&mut self, paint: &dyn CustomPaint) -> bool {
        paint.paint(self.canvas)
    }
}

#[cfg(not(feature = "raster-text"))]
struct Outline {
    path: femtovg::Path,
    scale: f32,
    x: f32,
    y: f32,
}
#[cfg(not(feature = "raster-text"))]
impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path
            .move_to(self.x + x * self.scale, self.y - y * self.scale)
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.path
            .line_to(self.x + x * self.scale, self.y - y * self.scale)
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.path.quad_to(
            self.x + x1 * self.scale,
            self.y - y1 * self.scale,
            self.x + x * self.scale,
            self.y - y * self.scale,
        )
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path.bezier_to(
            self.x + x1 * self.scale,
            self.y - y1 * self.scale,
            self.x + x2 * self.scale,
            self.y - y2 * self.scale,
            self.x + x * self.scale,
            self.y - y * self.scale,
        )
    }
    fn close(&mut self) {
        self.path.close()
    }
}

impl Drop for GlPainter<'_> {
    fn drop(&mut self) {
        if !self.images.is_empty() {
            self.canvas.flush();
            for image in self.images.drain(..) {
                self.canvas.delete_image(image);
            }
        }
    }
}
