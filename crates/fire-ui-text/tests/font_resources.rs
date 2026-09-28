use fire_ui_fonts::{Font, FontBytes, Fonts};
use fire_ui_text::Text;
#[test]
fn validation_rejects_empty_and_unshapable_fonts() {
    assert_eq!(
        Text::new(Fonts::default()).err().as_deref(),
        Some("At least one shaping font is required")
    );
    assert!(Text::new(Fonts::new([Font {
        bytes: FontBytes::from_static(b"not a font"),
        face_index: 0
    }]))
    .is_err());
}
#[test]
fn selected_collection_face_is_used_by_shaping() {
    use fire_ui::*;
    let bytes = FontBytes::load("/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc").unwrap();
    let font = Font {
        bytes: bytes.clone(),
        face_index: 1,
    };
    let mut text = Text::new(Fonts::new([font])).unwrap();
    let paragraph = text.layout(TextRequest {
        previous: None,
        text: "火 UI".into(),
        style: TextStyle::default(),
        width: None,
        revision: 0,
    });
    assert!(!paragraph.lines[0].runs.is_empty());
    assert!(Text::new(Fonts::new([Font {
        bytes,
        face_index: 999
    }]))
    .is_err());
}
