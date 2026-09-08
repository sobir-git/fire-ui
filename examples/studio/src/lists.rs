use crate::kit::*;
use crate::Event;
use fire_ui::*;
use fire_ui_widgets::*;
use std::convert::Infallible;

const TOTAL: usize = 100_000;

/// One row: a label inset from the row's edges by the theme's rhythm rather than by
/// numbers, so a row tightens with the theme exactly as its height does.
pub struct Row {
    label: Child<Label>,
}
impl Widget for Row {
    type Command = Infallible;
    type Output = Infallible;
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        // A constant, not the theme's rhythm: a virtual list publishes selection
        // state to each row, and a node holds one ambient value of one type, so a
        // row cannot also see the theme. Row height is chosen by the list itself,
        // which can see it, and does follow a theme change.
        let inset = 10.;
        // Only the sides are inset: the row's height is fixed by the list, and
        // insetting it too would clamp the label to whatever was left over.
        let m = cx.measure(
            self.label,
            Constraints::loose(Size::new((c.max.width - 2. * inset).max(0.), c.max.height)),
        );
        cx.place(
            self.label,
            Point::new(inset, ((c.max.height - m.size.height) / 2.).max(0.)),
        );
        Metrics::new(c.max)
    }
}

type Rows = VirtualList<usize, Row, fn(&usize) -> Element<Row>>;

fn row_for(key: &usize) -> Element<Row> {
    let text = format!("Material study {:05}", key + 1);
    Element::build(|children| Row {
        label: children.add(Element::leaf(Label::styled(text, TextRole::Body))),
    })
}

/// Row height as a rule: tall enough for a line of body text plus breathing room,
/// in whatever theme is in effect.
fn row_height(scale: &Scale) -> f32 {
    (scale.font_size * 1.7 + scale.space(0.75)).round()
}

/// A hundred thousand rows, of which only the visible ones exist.
pub struct Lists {
    heading: Child<Label>,
    caption: Child<Label>,
    search: Child<Editor>,
    list: Child<Surface<Rows>>,
    count: Child<Label>,
    /// Held for retry when the mailbox is full, so a fast typist cannot lose a filter.
    pending: Option<Vec<usize>>,
}
pub enum Signal {
    Query(EditorOutput),
    Row(ListOutput<usize, Infallible>),
}
impl Data for Signal {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Query(v) => v.bytes(),
                Self::Row(v) => v.bytes(),
            }
    }
}
impl Lists {
    pub fn new() -> Element<Self> {
        Element::build(|children| Self {
            heading: children.add(text("A hundred thousand rows", TextRole::Heading)),
            caption: children.add(muted_paragraph(
                "Rows mount as they scroll into view and retire as they leave. Type digits \
                 to filter; the list is told the new keys, not rebuilt.",
                TextRole::Small,
            )),
            search: children.connect(
                Element::leaf(Editor::field("").placeholder("Find a material study…")),
                |v| Signal::Query(v.clone()),
            ),
            list: children.connect(
                surface(SurfaceStyle::Sunken)
                    .inset(6.)
                    .wrap(Element::leaf(Rows::new(
                        (0..TOTAL).collect(),
                        // A rule, not a number: rows re-resolve when the theme
                        // changes, so they tighten with everything else.
                        RowHeight::Scaled(row_height),
                        row_for as fn(&usize) -> Element<Row>,
                    ))),
                |v| Signal::Row(v.clone()),
            ),
            count: children.add(muted(format!("{TOTAL} rows"), TextRole::Small)),
            pending: None,
        })
    }
    /// Delivers the filtered keys, retrying on the next frame if the mailbox is full.
    fn publish(&mut self, cx: &mut Update<'_, Self>) {
        if let Some(keys) = self.pending.take() {
            if let Err((_, ListCommand::Keys(keys))) = cx.send(self.list, ListCommand::Keys(keys)) {
                self.pending = Some(keys);
                cx.request_frame()
            }
        }
    }
}
impl Widget for Lists {
    type Command = Signal;
    type Output = Event;
    fn update(&mut self, cx: &mut Update<'_, Self>, signal: Signal) {
        match signal {
            Signal::Query(EditorOutput::Changed { text, .. }) => {
                let keys: Vec<usize> = (0..TOTAL)
                    .filter(|i| text.is_empty() || format!("{:05}", i + 1).contains(text.as_ref()))
                    .collect();
                let _ = cx.send(self.count, format!("{} of {TOTAL} rows", keys.len()));
                self.pending = Some(keys);
                self.publish(cx)
            }
            Signal::Row(ListOutput::Selected(key)) => {
                let _ = cx.emit(Event::Status(format!(
                    "Selected material study {:05}",
                    key + 1
                )));
            }
            _ => {}
        }
    }
    fn frame(&mut self, cx: &mut Update<'_, Self>, _: FrameTime) {
        self.publish(cx)
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let unit = gap(cx, 1.);
        column(
            cx,
            c,
            Flow::gap(unit).align(Align::Stretch),
            &[
                Entry::natural(self.heading),
                Entry::natural(self.caption),
                Entry::natural(self.search),
                Entry::fill(self.list),
                Entry::natural(self.count),
            ],
        )
    }
}
