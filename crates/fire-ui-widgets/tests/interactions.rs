use fire_ui::*;
use fire_ui_widgets::*;
use std::{cell::Cell, rc::Rc, time::Duration};
const NOW: Duration = Duration::ZERO;
fn key(ui: &mut Ui, key: Key, shift: bool, control: bool) {
    ui.dispatch(
        Event::Key {
            key,
            modifiers: Modifiers {
                shift,
                control,
                ..Modifiers::default()
            },
        },
        NOW,
        &mut TestText,
    );
}
fn input(ui: &Ui, id: WidgetId) -> &TextInput {
    ui.root.find(id).unwrap().widget::<TextInput>().unwrap()
}
fn click(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch(Event::PointerDown(Point::new(x, y)), NOW, &mut TestText);
    ui.dispatch(Event::PointerUp(Point::new(x, y)), NOW, &mut TestText);
}
fn two_inputs() -> (Ui, WidgetId, WidgetId) {
    let a = Node::new(TextInput::new("ab", Theme::default()));
    let aid = a.id;
    let b = Node::new(TextInput::new("second", Theme::default()));
    let bid = b.id;
    let root = Node::new(
        Flex::column(10.)
            .padding(10.)
            .lengths([Length::Fixed(50.), Length::Fixed(50.)]),
    )
    .children([a, b]);
    (Ui::new(root, Size::new(400., 160.)), aid, bid)
}

#[test]
fn caret_movement_persists_across_input_events() {
    let (mut ui, a, _) = two_inputs();
    key(&mut ui, Key::Tab, false, false);
    key(&mut ui, Key::End, false, false);
    key(&mut ui, Key::Left, false, false);
    ui.dispatch(Event::Text("X".into()), NOW, &mut TestText);
    assert_eq!(input(&ui, a).text(), "aXb");
}
#[test]
fn focus_routes_to_second_input_and_returns_to_first() {
    let (mut ui, a, b) = two_inputs();
    key(&mut ui, Key::Tab, false, false);
    key(&mut ui, Key::Tab, false, false);
    assert_eq!(ui.focused(), Some(b));
    ui.dispatch(Event::Text("Z".into()), NOW, &mut TestText);
    assert_eq!(input(&ui, b).text(), "Zsecond");
    assert_eq!(input(&ui, a).text(), "ab");
    key(&mut ui, Key::Tab, true, false);
    assert_eq!(ui.focused(), Some(a));
}
#[test]
fn pointer_coordinates_include_nested_layout_offsets_and_capture() {
    let (mut ui, a, _) = two_inputs();
    click(&mut ui, 31., 35.);
    assert_eq!(input(&ui, a).cursor(), 1);
    ui.dispatch(Event::PointerDown(Point::new(22., 35.)), NOW, &mut TestText);
    assert_eq!(ui.captured(), Some(a));
    ui.dispatch(
        Event::PointerMove(Point::new(600., 35.)),
        NOW,
        &mut TestText,
    );
    assert_eq!(input(&ui, a).selection(), Some(0..2));
    ui.dispatch(Event::PointerUp(Point::new(600., 35.)), NOW, &mut TestText);
    assert_eq!(ui.captured(), None);
}
#[test]
fn selection_replacement_is_one_undo_operation() {
    let (mut ui, a, _) = two_inputs();
    key(&mut ui, Key::Tab, false, false);
    key(&mut ui, Key::Character('a'), false, true);
    ui.dispatch(Event::Text("replacement".into()), NOW, &mut TestText);
    assert_eq!(input(&ui, a).text(), "replacement");
    key(&mut ui, Key::Character('z'), false, true);
    assert_eq!(input(&ui, a).text(), "ab");
    key(&mut ui, Key::Character('z'), true, true);
    assert_eq!(input(&ui, a).text(), "replacement");
}
#[test]
fn backspace_removes_a_grapheme_not_part_of_an_emoji() {
    let n = Node::new(TextInput::new("a👩‍💻e\u{301}", Theme::default()));
    let id = n.id;
    let mut ui = Ui::new(n, Size::new(400., 50.));
    key(&mut ui, Key::Tab, false, false);
    key(&mut ui, Key::End, false, false);
    key(&mut ui, Key::Backspace, false, false);
    assert_eq!(input(&ui, id).text(), "a👩‍💻");
    key(&mut ui, Key::Backspace, false, false);
    assert_eq!(input(&ui, id).text(), "a");
}
#[test]
fn large_content_stays_complete() {
    let text = "a long note\n".repeat(1000);
    let n = Node::new(TextInput::editor(text.clone(), Theme::default()));
    let id = n.id;
    let mut ui = Ui::new(n, Size::new(500., 300.));
    key(&mut ui, Key::Tab, false, false);
    key(&mut ui, Key::End, false, true);
    ui.dispatch(Event::Text("tail".into()), NOW, &mut TestText);
    assert_eq!(input(&ui, id).text(), text + "tail");
}
#[test]
fn wrapping_uses_visual_rows_for_caret_and_hit_testing() {
    let n = Node::new(TextInput::editor("abcdefghij", Theme::default()));
    let id = n.id;
    let mut ui = Ui::new(n, Size::new(69., 140.));
    ui.layout(&mut TestText);
    assert!(input(&ui, id).lines().len() > 1);
    let start = input(&ui, id).lines()[1].range.start;
    click(&mut ui, 12., 39.);
    assert_eq!(input(&ui, id).cursor(), start);
    key(&mut ui, Key::Home, false, false);
    assert_eq!(input(&ui, id).cursor(), start);
}
#[test]
fn list_navigation_scrolls_and_confirms_actual_selection() {
    let n = Node::new(List::new((0..100).map(|i| i.to_string()), Theme::default()));
    let id = n.id;
    let mut ui = Ui::new(n, Size::new(300., 90.));
    key(&mut ui, Key::Tab, false, false);
    for _ in 0..10 {
        key(&mut ui, Key::Down, false, false);
    }
    key(&mut ui, Key::Enter, false, false);
    assert!(ui.root.widget::<List>().unwrap().scroll > 0.);
    assert!(ui.messages().any(|m| m.source == id
        && m.value
            .downcast_ref::<Activated>()
            .is_some_and(|v| v.0 == 10)));
}
#[test]
fn modal_traps_focus_then_restores_previous_input() {
    let (mut base, a, b) = two_inputs();
    key(&mut base, Key::Tab, false, false);
    let inner = Node::new(TextInput::new("modal", Theme::default()));
    let inner_id = inner.id;
    let modal = Node::new(Modal::new(Size::new(200., 100.), Theme::default())).child(inner);
    let modal_id = modal.id;
    base.root.children.push(modal);
    base.invalidate();
    base.push_modal(modal_id, NOW, &mut TestText);
    assert_eq!(base.focused(), Some(inner_id));
    key(&mut base, Key::Tab, false, false);
    assert_eq!(base.focused(), Some(inner_id));
    base.pop_modal(NOW, &mut TestText);
    assert_eq!(base.focused(), Some(a));
    assert_ne!(base.focused(), Some(b));
}
#[test]
fn focus_loss_cancels_caret_timer() {
    let (mut ui, _, _) = two_inputs();
    assert_eq!(ui.next_deadline(), None);
    key(&mut ui, Key::Tab, false, false);
    assert!(ui.next_deadline().is_some());
    ui.window_focus_lost(NOW, &mut TestText);
    assert_eq!(ui.next_deadline(), None);
}
struct Timer {
    count: Rc<Cell<usize>>,
}
impl Widget for Timer {
    fn layout(&mut self, _: &mut [Node], c: Constraints, _: &mut dyn TextEngine) -> Size {
        c.max
    }
    fn event(&mut self, ctx: &mut EventCtx<'_>, event: &Event) {
        match event {
            Event::PointerDown(_) => ctx.request_frame_after(Duration::from_millis(20)),
            Event::Frame => {
                self.count.set(self.count.get() + 1);
                ctx.repaint();
            }
            _ => {}
        }
    }
}
#[test]
fn requested_frames_fire_once_and_idle_has_no_deadline() {
    let count = Rc::new(Cell::new(0));
    let mut ui = Ui::new(
        Node::new(Timer {
            count: count.clone(),
        }),
        Size::new(100., 100.),
    );
    click(&mut ui, 10., 10.);
    ui.advance(Duration::from_millis(19), &mut TestText);
    assert_eq!(count.get(), 0);
    ui.advance(Duration::from_millis(20), &mut TestText);
    assert_eq!(count.get(), 1);
    assert_eq!(ui.next_deadline(), None);
    ui.advance(Duration::from_secs(60), &mut TestText);
    assert_eq!(count.get(), 1);
}
#[test]
fn removing_a_widget_cancels_its_scheduled_work() {
    let count = Rc::new(Cell::new(0));
    let child = Node::new(Timer {
        count: count.clone(),
    });
    let root = Node::new(Stack).child(child);
    let mut ui = Ui::new(root, Size::new(100., 100.));
    click(&mut ui, 10., 10.);
    ui.root.children.clear();
    ui.invalidate();
    ui.advance(Duration::from_secs(1), &mut TestText);
    assert_eq!(ui.next_deadline(), None);
    assert_eq!(count.get(), 0);
}

#[test]
fn wrapped_layout_does_not_measure_quadratic_suffixes() {
    struct CountingText(usize);
    impl TextEngine for CountingText {
        fn measure(&mut self, text: &str, style: TextStyle) -> LineMetrics {
            self.0 += text.len();
            TestText.measure(text, style)
        }
    }
    let content = "abcdefghij ".repeat(10000);
    let mut ui = Ui::new(
        Node::new(TextInput::editor(&content, Theme::default())),
        Size::new(200., 200.),
    );
    let mut text = CountingText(0);
    ui.layout(&mut text);
    assert!(
        text.0 <= content.len() * 2,
        "measured {} bytes for {} bytes of text",
        text.0,
        content.len()
    );
    let measured = text.0;
    ui.layout(&mut text);
    assert_eq!(text.0, measured, "unchanged layout must stay cached");
}

#[test]
fn hiding_captured_input_releases_focus_and_drag_state() {
    let (mut ui, a, _) = two_inputs();
    ui.dispatch(Event::PointerDown(Point::new(22., 35.)), NOW, &mut TestText);
    ui.root.find_mut(a).unwrap().hidden = true;
    ui.invalidate();
    ui.layout(&mut TestText);
    assert_eq!(ui.captured(), None);
    assert_eq!(ui.focused(), None);
    assert_eq!(ui.next_deadline(), None);
    ui.root.find_mut(a).unwrap().hidden = false;
    ui.invalidate();
    ui.dispatch(
        Event::PointerMove(Point::new(100., 35.)),
        NOW,
        &mut TestText,
    );
    assert_eq!(input(&ui, a).cursor(), 0);
}

struct NullPainter;
impl Painter for NullPainter {
    fn save(&mut self) {}
    fn restore(&mut self) {}
    fn translate(&mut self, _: Point) {}
    fn transform(&mut self, _: [f32; 6]) {}
    fn clip(&mut self, _: Rect) {}
    fn rect(&mut self, _: Rect, _: f32, _: Brush) {}
    fn stroke(&mut self, _: Rect, _: f32, _: f32, _: Color) {}
    fn path(&mut self, _: &[PathCommand], _: Brush, _: Option<f32>) {}
    fn text(&mut self, _: &str, _: Point, _: TextStyle, _: Brush) {}
}
#[test]
fn caret_blink_invalidates_only_its_rectangle() {
    let (mut ui, _, _) = two_inputs();
    key(&mut ui, Key::Tab, false, false);
    ui.layout(&mut TestText);
    ui.paint(&mut NullPainter);
    ui.advance(Duration::from_millis(530), &mut TestText);
    let damage = ui.damage().unwrap();
    assert!(damage.width < 10.);
    assert!(damage.height < 30.);
}

struct PaintCounter(Rc<Cell<usize>>);
impl Widget for PaintCounter {
    fn layout(&mut self, _: &mut [Node], c: Constraints, _: &mut dyn TextEngine) -> Size {
        c.max
    }
    fn event(&mut self, ctx: &mut EventCtx<'_>, e: &Event) {
        if matches!(e, Event::User(_)) {
            ctx.repaint();
            ctx.handle();
        }
    }
    fn paint(&self, _: &mut PaintCtx<'_>) {
        self.0.set(self.0.get() + 1);
    }
}
#[test]
fn partial_paint_skips_unrelated_widgets() {
    let a = Rc::new(Cell::new(0));
    let b = Rc::new(Cell::new(0));
    let first = Node::new(PaintCounter(a.clone()));
    let id = first.id;
    let root = Node::new(Flex::row(20.).lengths([Length::Fixed(80.), Length::Fixed(80.)]))
        .children([first, Node::new(PaintCounter(b.clone()))]);
    let mut ui = Ui::new(root, Size::new(200., 100.));
    ui.layout(&mut TestText);
    ui.paint(&mut NullPainter);
    ui.send(id, Event::user(()), NOW, &mut TestText);
    ui.paint_damage(&mut NullPainter);
    assert_eq!(a.get(), 2);
    assert_eq!(b.get(), 1);
}
#[test]
fn many_resize_requests_coalesce_into_one_layout() {
    struct LayoutCounter(Rc<Cell<usize>>);
    impl Widget for LayoutCounter {
        fn layout(&mut self, _: &mut [Node], c: Constraints, _: &mut dyn TextEngine) -> Size {
            self.0.set(self.0.get() + 1);
            c.max
        }
    }
    let count = Rc::new(Cell::new(0));
    let mut ui = Ui::new(
        Node::new(LayoutCounter(count.clone())),
        Size::new(100., 100.),
    );
    ui.layout(&mut TestText);
    for width in 200..300 {
        ui.resize(Size::new(width as f32, 100.));
    }
    assert_eq!(count.get(), 1);
    ui.layout(&mut TestText);
    assert_eq!(count.get(), 2);
    assert_eq!(ui.root.rect.width, 299.);
}

#[test]
fn resize_reuses_text_metrics_until_wrapping_actually_becomes_necessary() {
    struct CountingText(usize);
    impl TextEngine for CountingText {
        fn measure(&mut self, text: &str, style: TextStyle) -> LineMetrics {
            self.0 += 1;
            TestText.measure(text, style)
        }
    }
    let mut text = CountingText(0);
    let mut ui = Ui::new(
        Node::new(TextInput::editor("short\nlines", Theme::default())),
        Size::new(500., 200.),
    );
    ui.layout(&mut text);
    let initial = text.0;
    for width in (100..500).rev() {
        ui.resize(Size::new(width as f32, 200.));
        ui.layout(&mut text);
    }
    assert_eq!(text.0, initial);
    ui.resize(Size::new(40., 200.));
    ui.layout(&mut text);
    assert!(text.0 > initial);
    assert!(ui.root.widget::<TextInput>().unwrap().lines().len() > 2);

    let mut ui = Ui::new(
        Node::new(TextInput::new("a long single-line input", Theme::default())),
        Size::new(500., 50.),
    );
    ui.layout(&mut text);
    let initial = text.0;
    key(&mut ui, Key::Tab, false, false);
    key(&mut ui, Key::End, false, false);
    ui.resize(Size::new(50., 50.));
    ui.layout(&mut text);
    assert_eq!(text.0, initial);
    assert!(
        ui.root.widget::<TextInput>().unwrap().scroll.x > 0.,
        "shrinking must still reveal the caret"
    );
}

#[test]
fn height_only_resize_keeps_the_caret_visible() {
    let mut ui = Ui::new(
        Node::new(TextInput::editor("one\ntwo\nthree\nfour", Theme::default())),
        Size::new(300., 300.),
    );
    key(&mut ui, Key::Tab, false, false);
    key(&mut ui, Key::End, false, true);
    ui.resize(Size::new(300., 55.));
    ui.layout(&mut TestText);
    assert!(ui.root.widget::<TextInput>().unwrap().scroll.y > 0.);
}
