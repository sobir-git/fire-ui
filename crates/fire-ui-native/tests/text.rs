use fire_ui::{TextEngine, TextStyle};
use fire_ui_native::NativeText;
use unicode_segmentation::UnicodeSegmentation;

#[test]
fn native_carets_use_the_same_run_width_and_grapheme_boundaries() {
    let mut text =
        NativeText::new().expect("Install a font or set FIRE_UI_FONT for native font tests");
    for value in [
        "aXb",
        "office",
        "cafe\u{301}",
        "👩‍💻",
        "Привет",
        "العربية",
        "abc العربية office",
    ] {
        let line = text.measure(value, TextStyle::default());
        assert!(line.width > 0.);
        assert_eq!(line.carets.first().unwrap().0, 0);
        assert_eq!(line.carets.last(), Some(&(value.len(), line.width)));
        assert!(line
            .carets
            .iter()
            .all(|(byte, _)| value.is_char_boundary(*byte)));
        assert_eq!(
            line.carets
                .iter()
                .map(|(byte, _)| *byte)
                .collect::<Vec<_>>(),
            value
                .grapheme_indices(true)
                .map(|(byte, _)| byte)
                .chain(std::iter::once(value.len()))
                .collect::<Vec<_>>()
        );
    }
    let line = text.measure("e\u{301}", TextStyle::default());
    assert_eq!(line.carets.len(), 2, "no caret inside a combining grapheme");
    let line = text.measure("👩‍💻", TextStyle::default());
    assert_eq!(line.carets.len(), 2, "no caret inside a joined emoji");
}
