use fire_ui::*;

#[test]
fn units_contain_the_byte_and_end_at_the_text_end() {
    let text = "one  two,\n\u{65}\u{301}x";
    assert_eq!(unit_at(text, 1, TextUnit::Word), 0..3);
    assert_eq!(unit_at(text, 3, TextUnit::Word), 3..5);
    assert_eq!(unit_at(text, 8, TextUnit::Word), 8..9);
    assert_eq!(unit_at(text, 9, TextUnit::Word), 9..10);
    assert_eq!(unit_at(text, 10, TextUnit::Grapheme), 10..13);
    assert_eq!(unit_at(text, 4, TextUnit::Paragraph), 0..10);
    assert_eq!(unit_at(text, 10, TextUnit::Paragraph), 10..14);
    let end = text.len();
    assert_eq!(unit_at(text, end, TextUnit::Word), end..end);
    assert_eq!(unit_at(text, end, TextUnit::Grapheme), end..end);
    assert_eq!(unit_at(text, end, TextUnit::Paragraph), 10..end);
    assert_eq!(unit_at("a\n", 2, TextUnit::Paragraph), 2..2);
}
