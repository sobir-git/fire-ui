//! A draw-only window: no widget crate, font files, or optional native services.
use fire_ui::{Color, Constraints, Element, Layout, Metrics, Paint, Size, Widget};
use fire_ui_native::{run, WindowOptions};
use std::convert::Infallible;

struct Canvas;

impl Widget for Canvas {
    type Command = Infallible;
    type Output = Infallible;

    fn layout(&mut self, _: &mut Layout<'_>, constraints: Constraints) -> Metrics {
        Metrics::new(constraints.max)
    }

    fn paint(&self, cx: &mut Paint<'_>) {
        cx.painter
            .rect(cx.bounds.inset(32.0), 12.0, Color::hex(0xff6b18).into());
    }
}

fn main() -> Result<(), String> {
    let fonts = fire_ui_fonts::Fonts::default();
    run(
        Element::leaf(Canvas),
        WindowOptions {
            title: "Fire UI Minimal".into(),
            size: Size::new(640.0, 480.0),
            background: Color::hex(0x181818),
            ..WindowOptions::default()
        },
        fire_ui::TestText,
        fire_ui_cairo::Cairo::direct(fonts),
    )
}
