use fire_ui::Element;
use fire_ui_native::{run, WindowOptions};
use fire_ui_widgets::{Label, Padding};

fn main() -> Result<(), String> {
    let fonts = fire_ui_fonts::Fonts::load(&[
        fire_ui_text::system_font().ok_or("Set FIRE_UI_FONT to a readable font file")?
    ])?;
    let content = Padding::new(Element::leaf(Label::new("Hello, Fire UI")), 24.0);
    run(
        content,
        WindowOptions {
            title: "Hello, Fire UI".into(),
            ..WindowOptions::default()
        },
        fire_ui_text::Text::new(fonts.clone())?,
        fire_ui_cairo::Cairo::direct(fonts),
    )
}
