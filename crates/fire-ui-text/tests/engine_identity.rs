#![cfg(target_os = "linux")]
use fire_ui::{Element, Limits, Size, TextEngine, Ui};
use fire_ui_fonts::Fonts;
use fire_ui_text::Text;
use fire_ui_widgets::Editor;

fn editor() -> Ui<Editor> {
    let mut ui = Ui::new(
        Element::leaf(Editor::new("iiii WWWW")),
        Size::new(600., 400.),
        Limits::default(),
    )
    .unwrap();
    ui.pump(100, |_| {}, |_| {});
    ui
}

#[test]
fn replacing_font_collection_reflows_existing_ui_and_editor_cache() {
    let mut sans =
        Text::new(Fonts::load(&["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"]).unwrap())
            .unwrap();
    let mut mono =
        Text::new(Fonts::load(&["/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"]).unwrap())
            .unwrap();
    assert_ne!(sans.revision(), mono.revision());
    let mut ui = editor();
    ui.layout(&mut sans);
    let old = ui.root().paragraph().unwrap().clone();
    assert!(ui.layout_pending(&mono));
    ui.layout(&mut mono);
    assert!(mono.layouts > 0);
    let replacement = ui.root().paragraph().unwrap().clone();
    assert_eq!(
        replacement.revision, old.revision,
        "document did not change"
    );
    assert_eq!(replacement.service_revision, mono.revision());
    assert_ne!(replacement.lines[0].width, old.lines[0].width);
    let mut fresh = editor();
    fresh.layout(&mut mono);
    assert_eq!(
        replacement.lines[0].width,
        fresh.root().paragraph().unwrap().lines[0].width
    );
    let calls = mono.layouts;
    ui.layout(&mut mono);
    assert_eq!(mono.layouts, calls, "unchanged engine should reuse layout");
}
