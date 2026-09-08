use crate::kit::*;
use crate::Event;
use fire_ui::*;
use fire_ui_widgets::*;

/// A number and what it counts. Three of these open the studio.
struct Stat {
    value: Child<Label>,
    caption: Child<Label>,
}
impl Stat {
    fn new(value: &str, caption: &str) -> Element<Surface<Self>> {
        surface(SurfaceStyle::Card).wrap(Element::build(|children| Self {
            value: children.add(accent(value.to_string(), TextRole::Title)),
            caption: children.add(muted_paragraph(caption.to_string(), TextRole::Small)),
        }))
    }
}
impl Widget for Stat {
    type Command = std::convert::Infallible;
    type Output = std::convert::Infallible;
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        column(
            cx,
            c,
            Flow::gap(gap(cx, 0.5)).align(Align::Stretch),
            &[Entry::natural(self.value), Entry::natural(self.caption)],
        )
    }
}

/// The banner across the top of the overview: the one accent-filled surface in the
/// studio, so the accent still means something everywhere else.
struct Banner {
    headline: Child<Label>,
    detail: Child<Label>,
}
impl Banner {
    fn new() -> Element<Surface<Self>> {
        surface(SurfaceStyle::Accent).wrap(Element::build(|children| Self {
            headline: children.add(Element::leaf(Label::toned(
                "Widgets that keep their own state.",
                TextRole::Heading,
                ColorRole::OnAccent,
            ))),
            detail: children.add(Element::leaf(
                Label::toned(
                    "Nothing rebuilds a tree every frame. A widget owns its data, \
                     declares what it accepts and what it reports, and asks for a repaint \
                     when it has something new to show.",
                    TextRole::Small,
                    ColorRole::OnAccent,
                )
                .wrap(),
            )),
        }))
    }
}
impl Widget for Banner {
    type Command = std::convert::Infallible;
    type Output = std::convert::Infallible;
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        column(
            cx,
            c,
            Flow::gap(gap(cx, 1.)).align(Align::Stretch),
            &[Entry::natural(self.headline), Entry::natural(self.detail)],
        )
    }
}

pub struct Overview {
    title: Child<Label>,
    tagline: Child<Label>,
    banner: Child<Surface<Banner>>,
    stats: Vec<Child<Surface<Stat>>>,
}
impl Overview {
    pub fn new() -> Element<Self> {
        Element::build(|children| Self {
            title: children.add(text("Fire UI", TextRole::Display)),
            tagline: children.add(muted_paragraph(
                "A small Rust toolkit for native desktop interfaces. Three crates: a \
                 dependency-free core, a set of controls, and a native host. Take the \
                 layers you want.",
                TextRole::Body,
            )),
            banner: children.add(Banner::new()),
            stats: vec![
                children.add(Stat::new("3", "crates, independently usable")),
                children.add(Stat::new("0", "dependencies in the core")),
                children.add(Stat::new("100k", "rows the list handles at once")),
            ],
        })
    }
}
impl Widget for Overview {
    type Command = std::convert::Infallible;
    type Output = Event;
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let unit = gap(cx, 1.);
        // Two policies, sequenced: a column of prose, then a reflowing grid of
        // stats starting where the column ended.
        let head = column(
            cx,
            Constraints::loose(Size::new(c.max.width, f32::INFINITY)),
            Flow::gap(unit * 2.).align(Align::Stretch),
            &[
                Entry::natural(self.title),
                Entry::natural(self.tagline),
                Entry::natural(self.banner),
            ],
        );
        let y = head.size.height + unit * 2.5;
        let cells: Vec<LayoutChild> = self.stats.iter().map(|s| (*s).into()).collect();
        let stats = grid_at(
            cx,
            Point::new(0., y),
            Constraints::loose(Size::new(c.max.width, f32::INFINITY)),
            0,
            190.,
            unit * 1.5,
            &cells,
        );
        Metrics::new(c.constrain(Size::new(c.max.width, y + stats.size.height)))
    }
}
