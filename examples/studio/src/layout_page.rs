use crate::kit::*;
use crate::Event;
use fire_ui::*;
use fire_ui_widgets::*;

/// A coloured block with a caption, so a layout policy has something visible to move.
struct Block {
    caption: Child<Label>,
    tone: f32,
}
impl Block {
    fn new(caption: &str, tone: f32) -> Element<Self> {
        Element::build(|children| Self {
            caption: children.add(Element::leaf(Label::toned(
                caption.to_string(),
                TextRole::Small,
                ColorRole::OnAccent,
            ))),
            tone,
        })
    }
}
impl Widget for Block {
    type Command = std::convert::Infallible;
    type Output = std::convert::Infallible;
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let t = theme(cx);
        let inset = t.scale.space(1.5);
        let m = cx.measure(self.caption, c.inset(inset));
        let size = c.constrain(Size::new(
            m.size.width + 2. * inset,
            (m.size.height + 2. * inset).max(t.scale.control),
        ));
        cx.place(
            self.caption,
            Point::new(inset, (size.height - m.size.height) / 2.),
        );
        Metrics::new(size)
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = painted_theme(cx);
        let c = t.color.accent;
        cx.painter.rect(
            cx.bounds,
            t.scale.radius,
            Color(c.0 * self.tone, c.1 * self.tone, c.2 * self.tone, 1.).into(),
        )
    }
}

const JUSTIFY: [(&str, Justify); 6] = [
    ("Start", Justify::Start),
    ("Center", Justify::Center),
    ("End", Justify::End),
    ("SpaceBetween", Justify::SpaceBetween),
    ("SpaceAround", Justify::SpaceAround),
    ("SpaceEvenly", Justify::SpaceEvenly),
];
const ALIGN: [(&str, Align); 4] = [
    ("Start", Align::Start),
    ("Center", Align::Center),
    ("End", Align::End),
    ("Stretch", Align::Stretch),
];

/// The layout vocabulary, driven live: change a rule and watch the same three
/// blocks move. This page is the framework's layout documentation.
pub struct LayoutPage {
    heading: Child<Label>,
    caption: Child<Label>,
    justify: Child<Field<Dropdown>>,
    align: Child<Field<Dropdown>>,
    demo: Child<Surface<Demo>>,
    grid_heading: Child<Label>,
    grid_caption: Child<Label>,
    cells: Vec<Child<Block>>,
}
pub enum Signal {
    Justify(usize),
    Align(usize),
}
impl Data for Signal {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}
impl LayoutPage {
    pub fn new() -> Element<Self> {
        Element::build(|children| Self {
            heading: children.add(text("Layout", TextRole::Heading)),
            caption: children.add(muted_paragraph(
                "A row or a column takes a flow — a gap, how leftover space is shared, \
                 and how children line up across the axis. Children claim their length \
                 as natural, fixed or a weighted share of what remains.",
                TextRole::Small,
            )),
            justify: children.connect(
                Field::new(
                    "Justify: where leftover space goes",
                    Dropdown::new("Justify", JUSTIFY.map(|(n, _)| n)),
                ),
                |v| Signal::Justify(*v),
            ),
            align: children.connect(
                Field::new(
                    "Align: placement across the axis",
                    Dropdown::new("Align", ALIGN.map(|(n, _)| n)),
                ),
                |v| Signal::Align(*v),
            ),
            demo: children.discard(surface(SurfaceStyle::Sunken).wrap(Demo::new())),
            grid_heading: children.add(text("A grid that reflows", TextRole::Heading)),
            grid_caption: children.add(muted_paragraph(
                "Given a minimum column width instead of a column count, the grid fits as \
                 many columns as the window allows. Narrow the window to watch it fold.",
                TextRole::Small,
            )),
            cells: (1..=6)
                .map(|i| children.add(Block::new(&format!("Cell {i}"), 0.8 + i as f32 * 0.035)))
                .collect(),
        })
    }
}
impl Widget for LayoutPage {
    type Command = Signal;
    type Output = Event;
    fn update(&mut self, cx: &mut Update<'_, Self>, signal: Signal) {
        let status = match signal {
            Signal::Justify(i) => {
                let _ = cx.send(self.demo, DemoCommand::Justify(JUSTIFY[i].1));
                format!("Justify::{}", JUSTIFY[i].0)
            }
            Signal::Align(i) => {
                let _ = cx.send(self.demo, DemoCommand::Align(ALIGN[i].1));
                format!("Align::{}", ALIGN[i].0)
            }
        };
        let _ = cx.emit(Event::Status(status));
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let unit = gap(cx, 1.);
        let width = Constraints::loose(Size::new(c.max.width, f32::INFINITY));
        let head = column(
            cx,
            width,
            Flow::gap(unit * 1.5).align(Align::Stretch),
            &[
                Entry::natural(self.heading),
                Entry::natural(self.caption),
                Entry::natural(self.justify),
                Entry::natural(self.align),
                Entry::fixed(self.demo, 170.),
                Entry::natural(self.grid_heading),
                Entry::natural(self.grid_caption),
            ],
        );
        let y = head.size.height + unit * 1.5;
        let cells: Vec<LayoutChild> = self.cells.iter().map(|b| (*b).into()).collect();
        let grid = grid_at(cx, Point::new(0., y), width, 0, 150., unit, &cells);
        Metrics::new(c.constrain(Size::new(c.max.width, y + grid.size.height)))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum DemoCommand {
    Justify(Justify),
    Align(Align),
}
impl Data for DemoCommand {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}
/// Three blocks under one flow, relaid whenever the rule changes.
struct Demo {
    blocks: Vec<Child<Block>>,
    flow: Flow,
}
impl Demo {
    fn new() -> Element<Self> {
        Element::build(|children| Self {
            blocks: vec![
                children.add(Block::new("natural", 0.82)),
                children.add(Block::new("a wider one", 0.91)),
                children.add(Block::new("third", 1.)),
            ],
            flow: Flow::gap(12.),
        })
    }
}
impl Widget for Demo {
    type Command = DemoCommand;
    type Output = std::convert::Infallible;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: DemoCommand) {
        match command {
            DemoCommand::Justify(justify) => self.flow.justify = justify,
            DemoCommand::Align(align) => self.flow.align = align,
        }
        cx.relayout()
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let entries: Vec<Entry> = self.blocks.iter().map(|b| Entry::natural(*b)).collect();
        let flow = Flow {
            gap: gap(cx, 1.5),
            ..self.flow
        };
        row(cx, Constraints::tight(c.max), flow, &entries)
    }
}
