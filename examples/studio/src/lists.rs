use crate::kit::*;
use crate::Event;
use fire_ui::*;
use fire_ui_widgets::*;
use std::convert::Infallible;

const TOTAL: usize = 100_000;

type Row = Padding<Label>;
type Rows = VirtualList<usize, Row, fn(&usize) -> Element<Row>>;
fn row_for(key: &usize) -> Element<Row> {
    Padding::new(
        Element::leaf(Label::styled(
            format!("Material study {:05}", key + 1),
            TextRole::Body,
        )),
        Insets::symmetric(14., 4.),
    )
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
                        // A virtual list needs one fixed row height up front, so this
                        // is the rhythm of the theme the studio starts in; it does not
                        // follow a later theme change.
                        Scale::default().space(4.),
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
