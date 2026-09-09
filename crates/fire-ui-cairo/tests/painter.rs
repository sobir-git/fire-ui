use fire_ui::*;
use fire_ui_cairo::{CairoPainter, FontFaces};
use fire_ui_fonts::Fonts;
use fire_ui_text::Text;
use std::sync::Arc;

fn pixels(draw: impl FnOnce(&mut CairoPainter<'_>)) -> Vec<u32> {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 160, 160).unwrap();
    let fonts = FontFaces::new(&Fonts::default()).unwrap();
    {
        let context = cairo::Context::new(&surface).unwrap();
        let mut painter = CairoPainter::new(context, &fonts);
        draw(&mut painter);
        painter.status().unwrap();
    }
    let bytes = surface.data().unwrap();
    bytes
        .chunks_exact(4)
        .map(|v| u32::from_ne_bytes(v.try_into().unwrap()))
        .collect()
}

#[test]
fn transforms_compose_and_clip_is_restored() {
    let image = pixels(|p| {
        p.save();
        p.transform(Transform([1., 0., 0., 1., 20., 30.]));
        p.transform(Transform([2., 0., 0., 2., 0., 0.]));
        p.clip(Rect::new(0., 0., 5., 5.));
        p.rect(
            Rect::new(-20., -20., 50., 50.),
            0.,
            Color::hex(0xff0000).into(),
        );
        p.restore();
        p.rect(Rect::new(1., 1., 3., 3.), 0., Color::hex(0x00ff00).into());
    });
    assert_eq!(image[35 * 160 + 25], 0xffff0000);
    assert_eq!(image[35 * 160 + 31], 0);
    assert_eq!(image[29 * 160 + 25], 0);
    assert_eq!(image[2 * 160 + 2], 0xff00ff00);
}

#[test]
fn tiled_box_fills_every_tile_and_preserves_path_and_clip() {
    let image = pixels(|p| {
        p.clip(Rect::new(4., 4., 148., 148.));
        p.rect(
            Rect::new(8., 8., 136., 136.),
            0.,
            Brush::Box {
                rect: Rect::new(20., 20., 100., 100.),
                radius: 0.,
                feather: 8.,
                from: Color::hex(0xff0000),
                to: Color::hex(0x0000ff),
            },
        );
    });
    for y in 24..116 {
        for x in 24..116 {
            assert_eq!(image[y * 160 + x], 0xffff0000, "{x},{y}");
        }
    }
    assert_eq!(image[130 * 160 + 130], 0xff0000ff);
    assert_eq!(image[7 * 160 + 30], 0);
}

#[test]
fn missing_font_is_reported_by_status() {
    let fonts = Fonts::default();
    let faces = FontFaces::new(&fonts).unwrap();
    let paragraph = Text::new(fonts).unwrap().layout(TextRequest {
        previous: None,
        text: Arc::from("Visible text"),
        style: TextStyle::default(),
        width: None,
        revision: 0,
    });
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 160, 160).unwrap();
    let mut painter = CairoPainter::new(cairo::Context::new(&surface).unwrap(), &faces);
    painter.paragraph(&paragraph, Point::default(), Color::hex(0xffffff).into());
    assert!(painter.status().unwrap_err().contains("font"));
}

#[test]
fn box_paint_reports_invalid_context() {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 10, 10).unwrap();
    let faces = FontFaces::new(&Fonts::default()).unwrap();
    let mut painter = CairoPainter::new(cairo::Context::new(&surface).unwrap(), &faces);
    painter.transform(Transform([0.; 6]));
    painter.rect(
        Rect::new(0., 0., 10., 10.),
        0.,
        Brush::Box {
            rect: Rect::new(0., 0., 10., 10.),
            radius: 1.,
            feather: 2.,
            from: Color::hex(0xffffff),
            to: Color::hex(0),
        },
    );
    assert!(painter.status().is_err());
}

#[test]
fn transformed_box_tiles_have_no_transparent_seams() {
    let image = pixels(|p| {
        p.transform(Transform([1.3, 0.2, 0., 1.3, 0.3, 0.7]));
        p.rect(
            Rect::new(-100., -100., 400., 400.),
            0.,
            Brush::Box {
                rect: Rect::new(0., 0., 100., 100.),
                radius: 4.,
                feather: 5.,
                from: Color::hex(0xffffff),
                to: Color::hex(0xffffff),
            },
        );
    });
    assert!(image.iter().all(|pixel| *pixel == 0xffffffff));
}

#[cfg(target_os = "linux")]
#[test]
fn font_resources_validate_with_freetype_and_keep_selected_collection_face_alive() {
    use fire_ui_fonts::{Font, FontBytes};
    assert!(FontFaces::new(&Fonts::new([Font {
        bytes: FontBytes::from_static(b"not a font"),
        face_index: 0
    }]))
    .is_err());
    let path = "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc";
    let bytes = FontBytes::load(path).unwrap();
    let fonts = Fonts::new([Font {
        bytes,
        face_index: 1,
    }]);
    let faces = FontFaces::new(&fonts).unwrap();
    let mut text = Text::new(fonts.clone()).unwrap();
    let paragraph = text.layout(TextRequest {
        previous: None,
        text: "火 UI".into(),
        style: TextStyle::default(),
        width: None,
        revision: 0,
    });
    drop(text);
    drop(fonts);
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 100, 100).unwrap();
    let context = cairo::Context::new(&surface).unwrap();
    let mut painter = CairoPainter::new(context, &faces);
    painter.paragraph(&paragraph, Point::new(2., 2.), Color::hex(0xffffff).into());
    assert!(painter.status().is_ok());
    drop(painter);
    surface.flush();
    assert!(surface.data().unwrap().iter().any(|byte| *byte != 0));
    assert!(FontFaces::new(&Fonts::new([Font {
        bytes: FontBytes::load(path).unwrap(),
        face_index: 999
    }]))
    .is_err());
}

#[test]
fn cairo_rejects_face_indices_that_freetype_packs_as_variation_instances() {
    use fire_ui_fonts::{Font, FontBytes};
    let fonts = Fonts::new([Font {
        bytes: FontBytes::from_static(b"unused"),
        face_index: 65536,
    }]);
    let error = FontFaces::new(&fonts).err().unwrap();
    assert!(error.contains("entry 0, face 65536"));
    assert!(error.contains("face-index limit"));
}

#[test]
fn box_geometry_bounds_preserve_transformed_fill_and_stroke_coverage() {
    for transform in [
        Transform([1., 0., 0., 1., 0.3, 0.7]),
        Transform([1.2, 0.3, -0.2, 1.1, 40., 10.]),
        Transform([-0.9, 0.2, 0.3, 1.2, 120., 5.]),
    ] {
        for kind in 0..3 {
            let draw = |p: &mut CairoPainter<'_>, brush| {
                p.transform(transform);
                match kind {
                    0 => p.rect(Rect::new(14.2, 17.4, 38.3, 24.7), 7., brush),
                    _ => p.path(
                        &[
                            Path::Move(Point::new(10., 20.)),
                            Path::Curve(
                                Point::new(80., -4.),
                                Point::new(-8., 90.),
                                Point::new(55., 55.),
                            ),
                            Path::Line(Point::new(25., 40.)),
                            Path::Close,
                        ],
                        brush,
                        (kind == 2).then_some(9.),
                    ),
                }
            };
            let solid = pixels(|p| draw(p, Color::hex(0xffffff).into()));
            let boxed = pixels(|p| {
                draw(
                    p,
                    Brush::Box {
                        rect: Rect::new(0., 0., 10., 10.),
                        radius: 0.,
                        feather: 3.,
                        from: Color::hex(0xffffff),
                        to: Color::hex(0xffffff),
                    },
                )
            });
            assert_eq!(solid, boxed, "kind {kind}, transform {transform:?}");
        }
    }
}

#[test]
fn bounded_box_preserves_gradient_coordinates() {
    let brush = Brush::Box {
        rect: Rect::new(43., 38., 6., 6.),
        radius: 2.,
        feather: 20.,
        from: Color::hex(0xff5500),
        to: Color::hex(0x2255ff),
    };
    let full = pixels(|p| p.rect(Rect::new(0., 0., 160., 160.), 0., brush));
    let small = pixels(|p| p.rect(Rect::new(35., 30., 25., 25.), 0., brush));
    for y in 0..160 {
        for x in 0..160 {
            assert_eq!(
                small[y * 160 + x],
                if (35..60).contains(&x) && (30..55).contains(&y) {
                    full[y * 160 + x]
                } else {
                    0
                }
            );
        }
    }
}

#[test]
fn box_glyph_bounds_include_origins_overhangs_and_transforms() {
    let fonts = Fonts::load(&["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"]).unwrap();
    let faces = FontFaces::new(&fonts).unwrap();
    let paragraph = Text::new(fonts).unwrap().layout(TextRequest {
        previous: None,
        text: Arc::from("jÁé العربية"),
        style: TextStyle { size: 22., font: 0 },
        width: Some(110.),
        revision: 0,
    });
    for transform in [
        Transform::default(),
        Transform([1.1, 0.2, -0.1, 1., 8., 3.]),
    ] {
        let render = |brush| {
            let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 160, 160).unwrap();
            {
                let mut painter = CairoPainter::new(cairo::Context::new(&surface).unwrap(), &faces);
                painter.transform(transform);
                painter.paragraph(&paragraph, Point::new(22.3, 38.7), brush);
                painter.status().unwrap();
            }
            let pixels = surface.data().unwrap().to_vec();
            pixels
        };
        let solid = render(Color::hex(0xffffff).into());
        let boxed = render(Brush::Box {
            rect: Rect::new(0., 0., 10., 10.),
            radius: 0.,
            feather: 3.,
            from: Color::hex(0xffffff),
            to: Color::hex(0xffffff),
        });
        assert_eq!(solid, boxed);
    }
}
