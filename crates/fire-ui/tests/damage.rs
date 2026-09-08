use fire_ui::*;
use std::{cell::Cell, convert::Infallible, rc::Rc};
struct Sink;
impl Painter for Sink {
    fn save(&mut self) {}
    fn restore(&mut self) {}
    fn transform(&mut self, _: Transform) {}
    fn clip(&mut self, _: Rect) {}
    fn rect(&mut self, _: Rect, _: f32, _: Brush) {}
    fn stroke(&mut self, _: Rect, _: f32, _: f32, _: Color) {}
    fn path(&mut self, _: &[Path], _: Brush, _: Option<f32>) {}
    fn paragraph(&mut self, _: &Paragraph, _: Point, _: Brush) {}
}
struct Patch(Rc<Cell<usize>>);
impl Widget for Patch {
    type Command = usize;
    type Output = Infallible;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: usize) {
        match command {
            0 => cx.repaint_rect(Rect::new(1., 2., 3., 4.)),
            1 => cx.repaint_rect(Rect::new(8., 2., 3., 4.)),
            _ => cx.relayout(),
        }
    }
    fn layout(&mut self, _: &mut Layout<'_>, c: Constraints) -> Metrics {
        Metrics::new(c.max)
    }
    fn paint(&self, _: &mut Paint<'_>) {
        self.0.set(self.0.get() + 1);
    }
}
struct Pair {
    left: Child<Patch>,
    right: Child<Patch>,
}
impl Widget for Pair {
    type Command = usize;
    type Output = Infallible;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: usize) {
        cx.send(self.left, command).unwrap();
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        for (child, x) in [(self.left, 20.), (self.right, 120.)] {
            cx.measure(child, Constraints::tight(Size::new(50., 50.)));
            cx.place(child, Point::new(x, 10.));
        }
        Metrics::new(c.max)
    }
}
#[test]
fn local_damage_is_transformed_coalesced_and_does_not_paint_unrelated_siblings() {
    let left = Rc::new(Cell::new(0));
    let right = Rc::new(Cell::new(0));
    let root = Element::build(|children| Pair {
        left: children.add(Element::leaf(Patch(left.clone()))),
        right: children.add(Element::leaf(Patch(right.clone()))),
    });
    let mut ui = Ui::new(root, Size::new(200., 100.), Limits::default()).unwrap();
    ui.pump(100, |_| {}, |_| {});
    ui.layout(&mut TestText);
    ui.paint(&mut Sink);
    assert_eq!((left.get(), right.get()), (1, 1));
    ui.send(0).unwrap();
    ui.send(1).unwrap();
    ui.pump(100, |_| {}, |_| {});
    let damage = ui.paint_damage().unwrap();
    assert_eq!(damage, Rect::new(21., 12., 10., 4.));
    ui.paint_region(&mut Sink, damage);
    assert_eq!((left.get(), right.get()), (2, 1));
    assert_eq!(ui.paint_damage(), None);
    ui.send(0).unwrap();
    ui.pump(100, |_| {}, |_| {});
    ui.paint_region(&mut Sink, Rect::new(0., 0., 1., 1.));
    assert!(
        ui.paint_damage().is_some(),
        "unpainted damage must not be lost"
    );
    ui.send(2).unwrap();
    ui.pump(100, |_| {}, |_| {});
    ui.layout(&mut TestText);
    assert_eq!(ui.paint_damage(), Some(Rect::new(0., 0., 200., 100.)));
}
