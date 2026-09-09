use fire_ui::*;
use std::sync::Arc;

struct MetricsEngine {
    token: TextRevision,
    scale: f32,
    layouts: usize,
}
impl MetricsEngine {
    fn new(scale: f32) -> Self {
        Self {
            token: TextRevision::new(),
            scale,
            layouts: 0,
        }
    }
    fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
        self.token = TextRevision::new();
    }
}
impl TextEngine for MetricsEngine {
    fn revision(&self) -> TextRevision {
        self.token
    }
    fn layout(&mut self, mut request: TextRequest) -> Arc<Paragraph> {
        self.layouts += 1;
        let style = request.style;
        request.style.size *= self.scale;
        let mut paragraph = TestText.layout(request);
        let output = Arc::make_mut(&mut paragraph);
        output.style = style;
        output.service_revision = self.token;
        paragraph
    }
}
#[derive(Default)]
struct CachedText {
    paragraph: Option<Arc<Paragraph>>,
}
impl Widget for CachedText {
    type Command = ();
    type Output = ();
    fn layout(&mut self, cx: &mut Layout<'_>, _: Constraints) -> Metrics {
        if self
            .paragraph
            .as_ref()
            .is_none_or(|p| p.service_revision != cx.text_revision())
        {
            self.paragraph = Some(cx.paragraph(TextRequest {
                previous: None,
                text: "metrics".into(),
                style: TextStyle::default(),
                width: None,
                revision: 7,
            }));
        }
        Metrics::new(self.paragraph.as_ref().unwrap().size)
    }
}
#[test]
fn custom_engine_replacement_and_metric_changes_invalidate_both_cache_owners() {
    let mut first = MetricsEngine::new(1.);
    let mut second = MetricsEngine::new(2.);
    let mut ui = Ui::new(
        Element::leaf(CachedText::default()),
        Size::new(500., 200.),
        Limits::default(),
    )
    .unwrap();
    ui.pump(100, |_| {}, |_| {});
    ui.layout(&mut first);
    let first_width = ui.root().paragraph.as_ref().unwrap().size.width;
    assert!(ui.layout_pending(&second));
    ui.layout(&mut second);
    assert_eq!(second.layouts, 1);
    assert_eq!(
        ui.root().paragraph.as_ref().unwrap().size.width,
        first_width * 2.
    );
    assert!(!ui.layout_pending(&second));
    ui.layout(&mut second);
    assert_eq!(second.layouts, 1);
    second.set_scale(3.);
    assert!(ui.layout_pending(&second));
    ui.layout(&mut second);
    assert_eq!(second.layouts, 2);
    let paragraph = ui.root().paragraph.as_ref().unwrap();
    assert!((paragraph.size.width - first_width * 3.).abs() < 0.001);
    assert_eq!(paragraph.revision, 7);
    assert_eq!(paragraph.service_revision, second.revision());
}
