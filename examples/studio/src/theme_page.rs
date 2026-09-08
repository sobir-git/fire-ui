use crate::kit::*;
use crate::Event;
use fire_ui::*;
use fire_ui_widgets::*;

/// Picks one entry out of a palette, so a swatch stays correct across a theme swap.
type Pick = fn(&Palette) -> Color;

/// One palette entry: the colour, and the role it answers to.
struct Swatch {
    name: Child<Label>,
    pick: Pick,
}
impl Swatch {
    fn new(name: &str, pick: Pick) -> Element<Self> {
        Element::build(|children| Self {
            name: children.add(muted(name.to_string(), TextRole::Micro)),
            pick,
        })
    }
}
impl Widget for Swatch {
    type Command = std::convert::Infallible;
    type Output = std::convert::Infallible;
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let t = theme(cx);
        let chip = t.scale.space(4.5);
        let m = cx.measure(
            self.name,
            Constraints::loose(Size::new(c.max.width, c.max.height)),
        );
        cx.place(self.name, Point::new(0., chip + t.scale.space(0.75)));
        Metrics::new(c.constrain(Size::new(
            c.max.width.min(m.size.width.max(chip)),
            chip + t.scale.space(0.75) + m.size.height,
        )))
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = painted_theme(cx);
        let chip = Rect::new(
            cx.bounds.x,
            cx.bounds.y,
            cx.bounds.width,
            t.scale.space(4.5),
        );
        cx.painter
            .rect(chip, t.scale.radius, (self.pick)(&t.color).into());
        cx.painter.stroke(
            chip.inset(0.5),
            t.scale.radius,
            t.scale.border,
            t.color.border,
        );
    }
}

const SWATCHES: [(&str, Pick); 10] = [
    ("background", |p| p.background),
    ("surface", |p| p.surface),
    ("raised", |p| p.raised),
    ("sunken", |p| p.sunken),
    ("border", |p| p.border),
    ("foreground", |p| p.foreground),
    ("muted", |p| p.muted),
    ("accent", |p| p.accent),
    ("danger", |p| p.danger),
    ("success", |p| p.success),
];
const STEPS: [(&str, TextRole); 6] = [
    ("Display", TextRole::Display),
    ("Title", TextRole::Title),
    ("Heading", TextRole::Heading),
    ("Body", TextRole::Body),
    ("Small", TextRole::Small),
    ("Micro", TextRole::Micro),
];

/// The token system, and the switch that proves it works both ways.
pub struct ThemePage {
    heading: Child<Label>,
    caption: Child<Label>,
    mode: Child<Field<Switch>>,
    palette_heading: Child<Label>,
    swatches: Vec<Child<Swatch>>,
    type_heading: Child<Label>,
    type_caption: Child<Label>,
    samples: Vec<Child<Label>>,
}
impl ThemePage {
    pub fn new() -> Element<Self> {
        Element::build(|children| Self {
            heading: children.add(text("Theme", TextRole::Heading)),
            caption: children.add(muted_paragraph(
                "A widget names a role — surface, muted, accent — and the theme decides \
                 the value. Nothing below hardcodes a colour, which is why one switch \
                 repaints the whole studio.",
                TextRole::Small,
            )),
            mode: children.connect(Field::new("Swap the palette", Switch::new("Light")), |v| *v),
            palette_heading: children.add(text("Palette", TextRole::Heading)),
            swatches: SWATCHES
                .iter()
                .map(|(name, pick)| children.add(Swatch::new(name, *pick)))
                .collect(),
            type_heading: children.add(text("Type scale", TextRole::Heading)),
            type_caption: children.add(muted_paragraph(
                "Six steps, each a fixed multiple of the base body size. Changing the base \
                 rescales every one of them and relays the window.",
                TextRole::Small,
            )),
            samples: STEPS
                .iter()
                .map(|(name, role)| children.add(text(name.to_string(), *role)))
                .collect(),
        })
    }
}
impl Widget for ThemePage {
    /// True for the light palette.
    type Command = bool;
    type Output = Event;
    fn update(&mut self, cx: &mut Update<'_, Self>, light: bool) {
        let _ = cx.emit(Event::Light(light));
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
                Entry::natural(self.mode),
                Entry::natural(self.palette_heading),
            ],
        );
        let y = head.size.height + unit * 1.5;
        let chips: Vec<LayoutChild> = self.swatches.iter().map(|s| (*s).into()).collect();
        let palette = grid_at(cx, Point::new(0., y), width, 0, 96., unit, &chips);
        let y = y + palette.size.height + unit * 3.;
        let mut entries = vec![
            Entry::natural(self.type_heading),
            Entry::natural(self.type_caption),
        ];
        entries.extend(self.samples.iter().map(|s| Entry::natural(*s)));
        let scale = column_at(
            cx,
            Point::new(0., y),
            width,
            Flow::gap(unit).align(Align::Stretch),
            &entries,
        );
        Metrics::new(c.constrain(Size::new(c.max.width, y + scale.size.height)))
    }
}
