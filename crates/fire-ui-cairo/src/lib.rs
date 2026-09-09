#![doc = include_str!("../README.md")]
use fire_ui::*;
use fire_ui_fonts::{FontBytes, Fonts};
pub struct CairoPainter<'a> {
    pub context: cairo::Context,
    fonts: &'a FontFaces,
    error: std::cell::RefCell<Option<String>>,
}
/// Cairo font objects own the selected byte storage and remain independent of windows.
pub struct FontFaces(Vec<cairo::FontFace>);
impl FontFaces {
    pub fn new(fonts: &Fonts) -> Result<Self, String> {
        load_faces(fonts).map(Self)
    }
}
impl<'a> CairoPainter<'a> {
    pub fn new(context: cairo::Context, fonts: &'a FontFaces) -> Self {
        Self {
            context,
            fonts,
            error: Default::default(),
        }
    }
    pub fn status(&self) -> Result<(), String> {
        if let Some(e) = &*self.error.borrow() {
            Err(e.clone())
        } else {
            self.context.status().map_err(|e| e.to_string())
        }
    }

    fn source(&self, b: Brush) {
        let c = &self.context;
        let stops = |p: &cairo::Gradient, from: Color, to: Color| {
            p.add_color_stop_rgba(
                0.,
                from.0 as f64,
                from.1 as f64,
                from.2 as f64,
                from.3 as f64,
            );
            p.add_color_stop_rgba(1., to.0 as f64, to.1 as f64, to.2 as f64, to.3 as f64);
        };
        match b {
            Brush::Solid(v) => c.set_source_rgba(v.0 as f64, v.1 as f64, v.2 as f64, v.3 as f64),
            Brush::Linear {
                start,
                end,
                from,
                to,
            } => {
                let p = cairo::LinearGradient::new(
                    start.x as f64,
                    start.y as f64,
                    end.x as f64,
                    end.y as f64,
                );
                stops(&p, from, to);
                let _ = c.set_source(p);
            }
            Brush::Radial {
                center,
                inner,
                outer,
                from,
                to,
            } => {
                let p = cairo::RadialGradient::new(
                    center.x as f64,
                    center.y as f64,
                    inner as f64,
                    center.x as f64,
                    center.y as f64,
                    outer.max(inner + 0.001) as f64,
                );
                stops(&p, from, to);
                let _ = c.set_source(p);
            }
            Brush::Box { .. } => unreachable!("box brushes are drawn in bounded tiles"),
        }
    }
    fn with_brush(
        &self,
        b: Brush,
        bounds: impl FnOnce(&cairo::Context) -> Result<(f64, f64, f64, f64), cairo::Error>,
        draw: impl Fn(&cairo::Context) -> Result<(), cairo::Error>,
    ) {
        let result = (|| -> Result<(), String> {
            let c = &self.context;
            let Brush::Box {
                rect,
                radius,
                feather,
                from,
                to,
            } = b
            else {
                self.source(b);
                return draw(c).map_err(|e| e.to_string());
            };
            let matrix = c.matrix();
            let inverse = matrix.try_invert().map_err(|e| e.to_string())?;
            c.save().map_err(|e| e.to_string())?;
            c.identity_matrix();
            let extents = c.clip_extents();
            c.restore().map_err(|e| e.to_string())?;
            let (left, top, right, bottom) = extents.map_err(|e| e.to_string())?;
            let (x0, y0, x1, y1) = bounds(c).map_err(|e| e.to_string())?;
            if x1 <= x0 || y1 <= y0 {
                return Ok(());
            }
            // Bounds are in user coordinates; tiles and antialias padding are in
            // device pixels. Transform every corner for rotation and shear.
            let corners =
                [(x0, y0), (x1, y0), (x0, y1), (x1, y1)].map(|(x, y)| matrix.transform_point(x, y));
            let left = left.max(corners.iter().map(|p| p.0).fold(f64::INFINITY, f64::min) - 1.);
            let top = top.max(corners.iter().map(|p| p.1).fold(f64::INFINITY, f64::min) - 1.);
            let right = right.min(
                corners
                    .iter()
                    .map(|p| p.0)
                    .fold(f64::NEG_INFINITY, f64::max)
                    + 1.,
            );
            let bottom = bottom.min(
                corners
                    .iter()
                    .map(|p| p.1)
                    .fold(f64::NEG_INFINITY, f64::max)
                    + 1.,
            );
            let path = c.copy_path().map_err(|e| e.to_string())?;
            let f = feather.max(0.001);
            for y in (top.floor() as i32..bottom.ceil() as i32).step_by(64) {
                for x in (left.floor() as i32..right.ceil() as i32).step_by(64) {
                    let w = 64.min(right.ceil() as i32 - x);
                    let h = 64.min(bottom.ceil() as i32 - y);
                    if w <= 0 || h <= 0 {
                        continue;
                    }
                    let mut image = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h)
                        .map_err(|e| e.to_string())?;
                    let stride = image.stride() as usize;
                    {
                        let mut data = image.data().map_err(|e| e.to_string())?;
                        for iy in 0..h as usize {
                            for ix in 0..w as usize {
                                let (px, py) = inverse.transform_point(
                                    x as f64 + ix as f64 + 0.5,
                                    y as f64 + iy as f64 + 0.5,
                                );
                                let qx = (px as f32 - rect.x - rect.width / 2.).abs()
                                    - (rect.width / 2. - radius);
                                let qy = (py as f32 - rect.y - rect.height / 2.).abs()
                                    - (rect.height / 2. - radius);
                                let distance =
                                    qx.max(0.).hypot(qy.max(0.)) + qx.max(qy).min(0.) - radius;
                                let t = ((distance + f / 2.) / f).clamp(0., 1.);
                                let a = from.3 + (to.3 - from.3) * t;
                                let channel =
                                    |v: f32, z: f32| ((v + (z - v) * t) * a * 255.).round() as u32;
                                let pixel = ((a * 255.).round() as u32) << 24
                                    | channel(from.0, to.0) << 16
                                    | channel(from.1, to.1) << 8
                                    | channel(from.2, to.2);
                                data[iy * stride + ix * 4..iy * stride + ix * 4 + 4]
                                    .copy_from_slice(&pixel.to_ne_bytes());
                            }
                        }
                    }
                    c.save().map_err(|e| e.to_string())?;
                    c.new_path();
                    c.identity_matrix();
                    c.rectangle(x as f64, y as f64, w as f64, h as f64);
                    c.clip();
                    c.set_matrix(matrix);
                    c.append_path(&path);
                    // Pad avoids sampling transparent pixels at tile boundaries under scaling.
                    let pattern = cairo::SurfacePattern::create(image);
                    pattern.set_extend(cairo::Extend::Pad);
                    pattern.set_matrix(cairo::Matrix::new(
                        matrix.xx(),
                        matrix.yx(),
                        matrix.xy(),
                        matrix.yy(),
                        matrix.x0() - x as f64,
                        matrix.y0() - y as f64,
                    ));
                    let painted = c.set_source(pattern).and_then(|_| draw(c));
                    let restored = c.restore();
                    painted.and(restored).map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            self.error.borrow_mut().get_or_insert(e);
        }
    }
    fn rounded(&self, r: Rect, radius: f32) {
        let c = &self.context;
        let k = radius.max(0.).min(r.width / 2.).min(r.height / 2.) as f64;
        let (x, y, w, h) = (r.x as f64, r.y as f64, r.width as f64, r.height as f64);
        c.new_sub_path();
        c.arc(x + w - k, y + k, k, -std::f64::consts::FRAC_PI_2, 0.);
        c.arc(x + w - k, y + h - k, k, 0., std::f64::consts::FRAC_PI_2);
        c.arc(
            x + k,
            y + h - k,
            k,
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::PI,
        );
        c.arc(
            x + k,
            y + k,
            k,
            std::f64::consts::PI,
            3. * std::f64::consts::FRAC_PI_2,
        );
        c.close_path();
    }
}
impl Painter for CairoPainter<'_> {
    fn save(&mut self) {
        let _ = self.context.save();
    }
    fn restore(&mut self) {
        let _ = self.context.restore();
    }
    fn transform(&mut self, t: Transform) {
        let a = t.0;
        self.context.transform(cairo::Matrix::new(
            a[0] as f64,
            a[1] as f64,
            a[2] as f64,
            a[3] as f64,
            a[4] as f64,
            a[5] as f64,
        ));
    }
    fn clip(&mut self, r: Rect) {
        self.context
            .rectangle(r.x as f64, r.y as f64, r.width as f64, r.height as f64);
        self.context.clip();
    }
    fn rect(&mut self, r: Rect, radius: f32, b: Brush) {
        self.rounded(r, radius);
        self.with_brush(b, |c| c.fill_extents(), |c| c.fill_preserve());
        self.context.new_path();
    }
    fn stroke(&mut self, r: Rect, radius: f32, width: f32, c: Color) {
        self.rounded(r, radius);
        self.context.set_line_width(width as f64);
        self.with_brush(c.into(), |c| c.stroke_extents(), |c| c.stroke_preserve());
        self.context.new_path();
    }
    fn path(&mut self, path: &[Path], b: Brush, width: Option<f32>) {
        let c = &self.context;
        c.new_path();
        for p in path {
            match p {
                Path::Move(p) => c.move_to(p.x as f64, p.y as f64),
                Path::Line(p) => c.line_to(p.x as f64, p.y as f64),
                Path::Curve(a, b, d) => c.curve_to(
                    a.x as f64, a.y as f64, b.x as f64, b.y as f64, d.x as f64, d.y as f64,
                ),
                Path::Close => c.close_path(),
            }
        }
        if let Some(width) = width {
            c.set_line_width(width as f64);
            self.with_brush(b, |c| c.stroke_extents(), |c| c.stroke_preserve());
        } else {
            self.with_brush(b, |c| c.fill_extents(), |c| c.fill_preserve());
        }
        c.new_path();
    }
    fn paragraph(&mut self, p: &Paragraph, origin: Point, b: Brush) {
        let c = &self.context;
        c.set_font_size(p.style.size as f64);
        let (_, top, _, bottom) = c.clip_extents().unwrap_or((0., 0., f64::MAX, f64::MAX));
        let first = p
            .lines
            .partition_point(|l| origin.y as f64 + l.y as f64 + (p.line_height as f64) < top);
        for line in p.lines[first..]
            .iter()
            .take_while(|l| ((origin.y + l.y) as f64) <= bottom)
        {
            for run in &line.runs {
                if run.font as usize >= self.fonts.0.len() {
                    self.error.borrow_mut().get_or_insert(
                        "Paragraph references a font that this painter does not own".into(),
                    );
                    return;
                }
                if let Some(face) = self.fonts.0.get(run.font as usize) {
                    c.set_font_face(face);
                    let glyphs: Vec<_> = run
                        .glyphs
                        .iter()
                        .map(|g| {
                            cairo::Glyph::new(
                                g.index as _,
                                (origin.x + run.x + g.offset.x) as f64,
                                (origin.y + line.y + p.baseline).round() as f64 + g.offset.y as f64,
                            )
                        })
                        .collect();
                    self.with_brush(
                        b,
                        |c| {
                            let extents = c.glyph_extents(&glyphs)?;
                            // Cairo bearings are relative to the first glyph position.
                            let x = glyphs.first().map_or(0., |g| g.x()) + extents.x_bearing();
                            let y = glyphs.first().map_or(0., |g| g.y()) + extents.y_bearing();
                            Ok((x, y, x + extents.width(), y + extents.height()))
                        },
                        |c| c.show_glyphs(&glyphs),
                    );
                }
            }
        }
    }
    fn custom(&mut self, paint: &dyn CustomPaint) -> bool {
        paint.paint(&mut self.context)
    }
}

fn load_faces(fonts: &Fonts) -> Result<Vec<cairo::FontFace>, String> {
    if fonts.is_empty() {
        return Ok(vec![]);
    }
    let library = cairo::freetype::Library::init().map_err(|e| e.to_string())?;
    static KEY: cairo::UserDataKey<cairo::freetype::Face<FontBytes>> = cairo::UserDataKey::new();
    fonts
        .iter()
        .enumerate()
        .map(|(entry, font)| {
            // FreeType reserves bits 16..30 for variation instances; our index
            // names only a face in the file, never a packed FreeType instance.
            if font.face_index > u16::MAX as u32 {
                return Err(format!(
                    "Cairo font entry {entry}, face {} exceeds FreeType's face-index limit",
                    font.face_index
                ));
            }
            let ft = library
                .new_memory_face2(font.bytes.clone(), font.face_index as isize)
                .map_err(|error| {
                    format!(
                        "Cairo font entry {entry}, face {}: {error}",
                        font.face_index
                    )
                })?;
            // Cairo may retain the face after this frame; user data owns FreeType and its font bytes.
            let face = unsafe {
                cairo::FontFace::from_raw_full(cairo::ffi::cairo_ft_font_face_create_for_ft_face(
                    ft.raw() as *const _ as *mut _,
                    0,
                ))
            };
            face.set_user_data(&KEY, std::rc::Rc::new(ft))
                .map_err(|e| e.to_string())?;
            face.status().map_err(|e| e.to_string())?;
            Ok(face)
        })
        .collect()
}

#[cfg(feature = "x11")]
mod native;
#[cfg(feature = "x11")]
pub use native::Cairo;

#[cfg(test)]
mod box_work_tests {
    use super::*;
    #[test]
    fn box_work_is_bounded_by_ink_not_the_whole_surface_clip() {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 2400, 1600).unwrap();
        let faces = FontFaces::new(&Fonts::default()).unwrap();
        let context = cairo::Context::new(&surface).unwrap();
        context.rectangle(100., 100., 40., 20.);
        let painter = CairoPainter::new(context, &faces);
        let draws = std::cell::Cell::new(0);
        painter.with_brush(
            Brush::Box {
                rect: Rect::new(100., 100., 40., 20.),
                radius: 3.,
                feather: 6.,
                from: Color::hex(0xffffff),
                to: Color::hex(0),
            },
            |c| c.fill_extents(),
            |c| {
                draws.set(draws.get() + 1);
                c.fill_preserve()
            },
        );
        painter.status().unwrap();
        assert_eq!(
            draws.get(),
            1,
            "a small shape must not repaint hundreds of clip tiles"
        );
    }
}
