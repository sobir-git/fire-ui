use crate::kit::*;
use crate::Event;
use fire_ui::*;
use fire_ui_widgets::*;

fn button(text: &str, style: ButtonStyle) -> Element<Button<Label>> {
    Button::styled(Element::leaf(Label::new(text)), text, style)
}

/// Every control the widget crate ships, live, with what it reports.
pub struct Controls {
    buttons: Vec<Child<Button<Label>>>,
    labels: Vec<&'static str>,
    disabled: Child<Button<Label>>,
    check: Child<Field<Checkbox>>,
    switch: Child<Field<Switch>>,
    choice: Child<Field<Dropdown>>,
    slider: Child<Field<Slider>>,
    readout: Child<Label>,
    progress: Child<Field<Progress>>,
    tabs: Child<Field<Tabs>>,
    buttons_section: Child<Label>,
    inputs_section: Child<Label>,
}
/// Everything this page can report, before it becomes a status line.
pub enum Signal {
    Pressed(usize),
    Checked(bool),
    Switched(bool),
    Chose(usize),
    Slid(f32),
    Tab(usize),
}
impl Data for Signal {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}
const BUTTONS: [(&str, ButtonStyle); 4] = [
    ("Primary", ButtonStyle::Primary),
    ("Secondary", ButtonStyle::Secondary),
    ("Ghost", ButtonStyle::Ghost),
    ("Delete", ButtonStyle::Danger),
];
const FLAVOURS: [&str; 4] = ["Ember", "Charcoal", "Paper", "Signal"];

impl Controls {
    pub fn new() -> Element<Self> {
        Element::build(|children| Self {
            buttons_section: children.add(text("Buttons", TextRole::Heading)),
            buttons: BUTTONS
                .iter()
                .enumerate()
                .map(|(i, (label, style))| {
                    children.connect(button(label, *style), move |_| Signal::Pressed(i))
                })
                .collect(),
            labels: BUTTONS.map(|(label, _)| label).to_vec(),
            disabled: children.discard(button("Disabled", ButtonStyle::Secondary)),
            inputs_section: children.add(text("Inputs", TextRole::Heading)),
            check: children.connect(
                Field::new("A box you tick", Checkbox::new("Repaint only what changed")),
                |v| Signal::Checked(*v),
            ),
            switch: children.connect(
                Field::new("A setting that applies at once", Switch::new("Animate")),
                |v| Signal::Switched(*v),
            ),
            choice: children.connect(
                Field::new("One of several", Dropdown::new("Palette", FLAVOURS)),
                |v| Signal::Chose(*v),
            ),
            slider: children.connect(
                Field::new(
                    "A quantity, by pointer or arrow keys",
                    Slider::stepped("Warmth", 62., 0., 100., 1.),
                ),
                |v| Signal::Slid(*v),
            ),
            readout: children.add(accent("62", TextRole::Title)),
            progress: children.discard(Field::new(
                "Read-only progress",
                Progress::new("Coverage", 0.62),
            )),
            tabs: children.connect(
                Field::new(
                    "A choice that switches a view",
                    Tabs::new(["Day", "Week", "Month"]),
                ),
                |v| Signal::Tab(*v),
            ),
        })
    }
}
impl Widget for Controls {
    type Command = Signal;
    type Output = Event;
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        if event == Lifecycle::Mount {
            let _ = cx.send(self.disabled, ButtonCommand::Disabled(true));
        }
    }
    fn update(&mut self, cx: &mut Update<'_, Self>, signal: Signal) {
        let status = match signal {
            Signal::Pressed(i) => format!("Pressed {}", self.labels[i]),
            Signal::Checked(v) => format!("Checkbox is now {}", if v { "ticked" } else { "clear" }),
            Signal::Switched(v) => format!("Animation {}", if v { "on" } else { "off" }),
            Signal::Chose(i) => format!("Chose {}", FLAVOURS[i]),
            Signal::Slid(v) => {
                let _ = cx.send(self.readout, format!("{v:.0}"));
                let _ = cx.send(self.progress, (v / 100.).clamp(0., 1.));
                format!("Warmth {v:.0}")
            }
            Signal::Tab(i) => format!("Tab {i}"),
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
            &[Entry::natural(self.buttons_section)],
        );
        // Buttons keep their natural widths and wrap is not needed: a row with a
        // trailing spacer would over-stretch them, so they simply sit side by side.
        let mut entries: Vec<Entry> = self.buttons.iter().map(|b| Entry::natural(*b)).collect();
        entries.push(Entry::natural(self.disabled));
        let buttons = row_at(
            cx,
            Point::new(0., head.size.height),
            Constraints::loose(Size::new(c.max.width, f32::INFINITY)),
            Flow::gap(unit).align(Align::Center),
            &entries,
        );
        let y = head.size.height + buttons.size.height + unit * 3.;
        let rest = column_at(
            cx,
            Point::new(0., y),
            width,
            Flow::gap(unit * 2.).align(Align::Stretch),
            &[
                Entry::natural(self.inputs_section),
                Entry::natural(self.check),
                Entry::natural(self.switch),
                Entry::natural(self.choice),
                Entry::natural(self.slider),
                Entry::natural(self.readout),
                Entry::natural(self.progress),
                Entry::natural(self.tabs),
            ],
        );
        Metrics::new(c.constrain(Size::new(c.max.width, y + rest.size.height)))
    }
}
