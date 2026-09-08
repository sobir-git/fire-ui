use crate::kit::*;
use crate::Event;
use fire_ui::*;
use fire_ui_widgets::*;

const SAMPLE: &str = "Good tools leave room for the person using them.\n\n\
Select a word. Move a paragraph. Undo it. The editor keeps its own text, \
selection and undo history; this page only hears that something changed.\n\n\
Café · Привет · Καλημέρα · 你好";

/// The editor, a single-line field, and what the framework knows about text.
pub struct TextPage {
    editor: Child<Section<Surface<Editor>>>,
    field: Child<Field<Editor>>,
    counter: Child<Label>,
    note: Child<Label>,
}
pub enum Signal {
    Body(EditorOutput),
    Field(EditorOutput),
}
impl Data for Signal {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Body(v) | Self::Field(v) => v.bytes(),
            }
    }
}
impl TextPage {
    pub fn new() -> Element<Self> {
        Element::build(|children| Self {
            editor: children.connect(
                Section::new(
                    "Editing",
                    "Shaped text, visual caret movement, selection and undo. Mixed \
                     directions share one shaping pass.",
                    // The surface is the well; the editor draws no chrome of its own,
                    // so there is one box here rather than a box inside a box.
                    surface(SurfaceStyle::Sunken)
                        .wrap(Element::leaf(Editor::new(SAMPLE).chrome(false))),
                ),
                |v| Signal::Body(v.clone()),
            ),
            field: children.connect(
                Field::new(
                    "A single-line field, with a placeholder",
                    Element::leaf(Editor::field("").placeholder("Type a title…")),
                ),
                |v| Signal::Field(v.clone()),
            ),
            counter: children.add(muted(
                format!("{} characters", SAMPLE.chars().count()),
                TextRole::Small,
            )),
            note: children.add(muted_paragraph(
                "Composition from an input method keeps its own selection and replaces \
                 the selected text in a temporary layout, so the committed document is \
                 never disturbed mid-word.",
                TextRole::Small,
            )),
        })
    }
}
impl Widget for TextPage {
    type Command = Signal;
    type Output = Event;
    fn update(&mut self, cx: &mut Update<'_, Self>, signal: Signal) {
        let status = match signal {
            Signal::Body(EditorOutput::Changed { text, .. }) => {
                let count = text.chars().count();
                let _ = cx.send(self.counter, format!("{count} characters"));
                format!("{count} characters in the note")
            }
            Signal::Field(EditorOutput::Changed { text, .. }) => {
                format!("Title is now {:?}", text.as_ref())
            }
            _ => return,
        };
        let _ = cx.emit(Event::Status(status));
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let unit = gap(cx, 1.);
        column(
            cx,
            c,
            Flow::gap(unit * 2.).align(Align::Stretch),
            &[
                Entry::fixed(self.editor, 300.),
                Entry::natural(self.counter),
                Entry::natural(self.field),
                Entry::natural(self.note),
            ],
        )
    }
}
