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
#[test]
fn clicks_count_close_presses_of_one_button_and_releases_repeat_them() {
    use std::time::Duration;
    let ms = Duration::from_millis;
    let p = Point::new(10., 10.);
    let mut c = ClickCounter::new(ms(300), 4.);
    let primary = PointerButton::Primary;
    assert_eq!(c.count(0, primary, true, p, ms(0)), 1);
    assert_eq!(c.count(0, primary, false, p, ms(50)), 1);
    assert_eq!(c.count(0, primary, true, Point::new(13., 7.), ms(300)), 2);
    assert_eq!(c.count(0, primary, false, p, ms(320)), 2);
    assert_eq!(c.count(0, primary, true, p, ms(500)), 3);
    // Too far, too late, or another button starts over.
    assert_eq!(c.count(0, primary, true, Point::new(15., 10.), ms(600)), 1);
    assert_eq!(c.count(0, primary, true, Point::new(15., 10.), ms(901)), 1);
    assert_eq!(c.count(0, PointerButton::Secondary, true, p, ms(950)), 1);
    assert_eq!(c.count(0, primary, true, p, ms(1000)), 1);
}
