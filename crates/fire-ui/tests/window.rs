use fire_ui::*;
use std::{cell::RefCell, convert::Infallible, rc::Rc};
struct Root(Rc<RefCell<Vec<Lifecycle>>>);
impl Widget for Root {
    type Command = f32;
    type Output = Infallible;
    fn lifecycle(&mut self, _: &mut Update<'_, Self>, event: Lifecycle) {
        if matches!(
            event,
            Lifecycle::WindowFocus(_) | Lifecycle::WindowTransparent(_)
        ) {
            self.0.borrow_mut().push(event);
        }
    }
    fn layout(&mut self, _: &mut Layout<'_>, c: Constraints) -> Metrics {
        Metrics::new(Size::new(c.max.width, 123.))
    }
}
#[test]
fn root_receives_native_focus_once_per_transition() {
    let seen = Rc::new(RefCell::new(vec![]));
    let mut ui = Ui::new(
        Element::leaf(Root(seen.clone())),
        Size::new(680., 400.),
        Limits::default(),
    )
    .unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    ui.window_focus(false);
    ui.window_focus(false);
    ui.window_focus(true);
    ui.pump(100, |v| match v {}, |_| {});
    assert_eq!(
        *seen.borrow(),
        vec![Lifecycle::WindowFocus(false), Lifecycle::WindowFocus(true)]
    );
}
#[test]
fn root_learns_when_window_alpha_starts_and_stops_showing() {
    let seen = Rc::new(RefCell::new(vec![]));
    let mut ui = Ui::new(
        Element::leaf(Root(seen.clone())),
        Size::new(680., 400.),
        Limits::default(),
    )
    .unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    assert!(!ui.window_transparent());
    ui.set_window_transparent(false);
    ui.set_window_transparent(true);
    ui.set_window_transparent(true);
    ui.pump(100, |v| match v {}, |_| {});
    ui.set_window_transparent(false);
    ui.pump(100, |v| match v {}, |_| {});
    assert_eq!(
        *seen.borrow(),
        vec![
            Lifecycle::WindowTransparent(true),
            Lifecycle::WindowTransparent(false)
        ]
    );
}
#[test]
fn content_height_preserves_width_and_obeys_bounds() {
    let mut ui = Ui::new(
        Element::leaf(Root(Rc::default())),
        Size::new(680., 400.),
        Limits::default(),
    )
    .unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    ui.layout_content_height(&mut TestText, 30., 300.);
    assert_eq!(ui.size(), Size::new(680., 123.));
    ui.resize(Size::new(700., 400.));
    ui.layout_content_height(&mut TestText, 150., 300.);
    assert_eq!(ui.size(), Size::new(700., 150.));
}
