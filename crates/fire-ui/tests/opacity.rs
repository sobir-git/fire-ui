use fire_ui::*;
use std::convert::Infallible;

/// Records the effective alpha of every rectangle, the way a backend applies it.
#[derive(Default)]
struct Alpha {
    alpha: Vec<f32>,
    rects: Vec<f32>,
}
impl Painter for Alpha {
    fn save(&mut self) {
        let top = self.alpha.last().copied().unwrap_or(1.);
        self.alpha.push(top);
    }
    fn restore(&mut self) {
        self.alpha.pop();
    }
    fn transform(&mut self, _: Transform) {}
    fn clip(&mut self, _: Rect) {}
    fn opacity(&mut self, alpha: f32) {
        if let Some(top) = self.alpha.last_mut() {
            *top *= alpha;
        }
    }
    fn rect(&mut self, _: Rect, _: f32, _: Brush) {
        self.rects.push(self.alpha.last().copied().unwrap_or(1.));
    }
    fn stroke(&mut self, _: Rect, _: f32, _: f32, _: Color) {}
    fn path(&mut self, _: &[Path], _: Brush, _: Option<f32>) {}
    fn paragraph(&mut self, _: &Paragraph, _: Point, _: Brush) {}
}

struct Leaf;
impl Widget for Leaf {
    type Command = Infallible;
    type Output = Infallible;
    fn layout(&mut self, _: &mut Layout<'_>, c: Constraints) -> Metrics {
        Metrics::new(c.max)
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        cx.painter.rect(cx.bounds, 0., Color::hex(0xffffff).into());
    }
}
struct Inner {
    alpha: f32,
    _leaf: Child<Leaf>,
}
impl Widget for Inner {
    type Command = Infallible;
    type Output = Infallible;
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        cx.overlay(c)
    }
    fn opacity(&self) -> f32 {
        self.alpha
    }
}
struct Fade {
    alpha: f32,
    _inner: Child<Inner>,
}
impl Widget for Fade {
    type Command = f32;
    type Output = Infallible;
    fn update(&mut self, cx: &mut Update<'_, Self>, alpha: f32) {
        self.alpha = alpha;
        cx.repaint();
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        cx.overlay(c)
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        cx.painter.rect(cx.bounds, 0., Color::hex(0x000000).into());
    }
    fn opacity(&self) -> f32 {
        self.alpha
    }
}

#[test]
fn opacity_fades_a_whole_subtree_and_nests_multiplicatively() {
    let root = Element::build(|c| Fade {
        alpha: 0.8,
        _inner: c.add(Element::build(|c| Inner {
            alpha: 0.5,
            _leaf: c.add(Element::leaf(Leaf)),
        })),
    });
    let mut ui = Ui::new(root, Size::new(100., 100.), Limits::default()).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    ui.layout(&mut TestText);
    let mut paint = Alpha::default();
    ui.paint(&mut paint);
    assert_eq!(paint.rects, vec![0.8, 0.4]);
    assert!(paint.alpha.is_empty(), "every save is restored");

    ui.send(1.).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    let mut paint = Alpha::default();
    ui.paint(&mut paint);
    assert_eq!(
        paint.rects,
        vec![1., 0.5],
        "an opaque widget adds no opacity"
    );

    ui.send(0.).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    let mut paint = Alpha::default();
    ui.paint(&mut paint);
    assert!(
        paint.rects.is_empty(),
        "a transparent subtree is not painted"
    );
}
