use fire_ui::Element;
use fire_ui_native::{run, WindowOptions};
use fire_ui_widgets::{Label, Padding};

fn main() -> Result<(), String> {
    let content = Padding::new(Element::leaf(Label::new("Hello, Fire UI")), 24.0);
    run(
        content,
        WindowOptions {
            title: "Hello, Fire UI".into(),
            font: Some(fire_ui_native::system_font().ok_or("Choose a font file")?),
            ..WindowOptions::default()
        },
    )
}
