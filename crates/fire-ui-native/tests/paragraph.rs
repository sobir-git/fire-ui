use fire_ui::*;
use fire_ui_native::NativeText;
use std::sync::Arc;
#[test]
fn native_shaping_wraps_and_returns_caret_geometry_for_the_same_text() {
    let mut text = NativeText::new().expect("Install a TrueType font or set FIRE_UI_FONT");
    let source: Arc<str> = Arc::from("Café Привет e\u{301} office\nsecond line");
    let p = text.layout(TextRequest {
        text: source.clone(),
        style: TextStyle::default(),
        width: Some(90.),
        revision: 7,
    });
    assert!(Arc::ptr_eq(&source, &p.text));
    assert_eq!(p.revision, 7);
    assert!(p.lines.len() > 2);
    for line in &p.lines {
        assert!(line.width.is_finite() && line.width > 0.);
        for stop in &line.stops {
            assert!(p.text.is_char_boundary(stop.caret.byte));
            let point = p.caret_point(stop.caret);
            assert!((point.x - stop.x).abs() < 0.1);
            assert!((point.y - line.y).abs() < 0.1);
        }
    }
    assert!(!p.selection(0..5).is_empty());
}
#[test]
fn empty_native_paragraph_has_a_caret_and_finite_extent() {
    let mut text = NativeText::new().unwrap();
    let p = text.layout(TextRequest {
        text: Arc::from(""),
        style: TextStyle::default(),
        width: Some(0.),
        revision: 0,
    });
    assert!(p.size.valid());
    assert_eq!(p.lines.len(), 1);
    assert_eq!(p.hit(Point::default()).byte, 0);
    assert_eq!(p.caret_point(Caret::at(0)), Point::default());
}

#[test]
fn wrapping_prefers_words_and_keeps_every_byte() {
    let mut text = NativeText::new().unwrap();
    let p = text.layout(TextRequest {
        text: Arc::from("Small ideas deserve a little room."),
        style: TextStyle::default(),
        width: Some(130.),
        revision: 0,
    });
    assert!(p.lines.len() > 1);
    for line in p.lines.iter().take(p.lines.len() - 1) {
        assert!(p.text[line.range.clone()].ends_with(' '));
    }
    let reconstructed: String = p
        .lines
        .iter()
        .map(|line| &p.text[line.range.clone()])
        .collect();
    assert_eq!(reconstructed, p.text.as_ref());
    let p = text.layout(TextRequest {
        text: Arc::from("supercalifragilistic"),
        style: TextStyle::default(),
        width: Some(12.),
        revision: 0,
    });
    assert!(p.lines.len() > 5);
    assert!(p.lines.iter().all(|line| !line.range.is_empty()));
}
