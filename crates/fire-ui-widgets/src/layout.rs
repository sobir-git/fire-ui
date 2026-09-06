use fire_ui::*;
#[derive(Clone, Copy, Debug)]
pub enum Length {
    Natural,
    Fixed(f32),
    Fill(f32),
}
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub child: LayoutChild,
    pub length: Length,
}
impl Entry {
    pub fn new<W: Widget>(child: Child<W>, length: Length) -> Self {
        Self {
            child: child.into(),
            length,
        }
    }
}
/// A borrowed layout policy, not a second ownership tree.
pub fn column(cx: &mut Layout<'_>, c: Constraints, gap: f32, children: &[Entry]) -> Metrics {
    linear(cx, c, gap, children, true)
}
pub fn row(cx: &mut Layout<'_>, c: Constraints, gap: f32, children: &[Entry]) -> Metrics {
    linear(cx, c, gap, children, false)
}
fn linear(
    cx: &mut Layout<'_>,
    c: Constraints,
    gap: f32,
    children: &[Entry],
    vertical: bool,
) -> Metrics {
    let main = |s: Size| if vertical { s.height } else { s.width };
    let cross = |s: Size| if vertical { s.width } else { s.height };
    let size = |m: f32, x: f32| {
        if vertical {
            Size::new(x, m)
        } else {
            Size::new(m, x)
        }
    };
    let available = main(c.max);
    let breadth = cross(c.max);
    let mut used = gap * children.len().saturating_sub(1) as f32;
    let mut weight = 0.;
    let mut measured = vec![Metrics::default(); children.len()];
    for (i, entry) in children.iter().enumerate() {
        match entry.length {
            Length::Fill(w) if available.is_finite() => weight += w.max(0.),
            Length::Fixed(n) => {
                let n = n.max(0.).min(available);
                measured[i] = cx.measure_child(
                    entry.child,
                    Constraints {
                        min: size(n, 0.),
                        max: size(n, breadth),
                    },
                );
                used += main(measured[i].size)
            }
            _ => {
                measured[i] = cx.measure_child(entry.child, Constraints::loose(c.max));
                used += main(measured[i].size)
            }
        }
    }
    for (i, entry) in children.iter().enumerate() {
        if let Length::Fill(w) = entry.length {
            if available.is_finite() {
                let n = if weight > 0. {
                    (available - used).max(0.) * w.max(0.) / weight
                } else {
                    0.
                };
                measured[i] = cx.measure_child(
                    entry.child,
                    Constraints {
                        min: size(n, 0.),
                        max: size(n, breadth),
                    },
                )
            }
        }
    }
    let baseline = if vertical {
        measured.first().and_then(|m| m.baseline)
    } else {
        measured
            .iter()
            .map(|m| m.baseline.unwrap_or(m.size.height))
            .reduce(f32::max)
    };
    let mut cursor = 0.;
    let mut widest: f32 = 0.;
    for (entry, m) in children.iter().zip(&measured) {
        let offset = if vertical {
            0.
        } else {
            baseline.unwrap_or(0.) - m.baseline.unwrap_or(m.size.height)
        };
        cx.place_child(
            entry.child,
            if vertical {
                Point::new(0., cursor)
            } else {
                Point::new(cursor, offset)
            },
        );
        cursor += main(m.size) + gap;
        widest = widest.max(cross(m.size) + offset)
    }
    Metrics {
        size: c.constrain(size(
            if children.is_empty() {
                0.
            } else {
                cursor - gap
            },
            widest,
        )),
        baseline,
    }
}
pub struct Padding<C: Widget> {
    child: Child<C>,
    inset: f32,
}
impl<C: Widget> Padding<C> {
    pub fn new(content: Element<C>, inset: f32) -> Element<Self> {
        Element::build(|children| Self {
            child: children.forward(content),
            inset,
        })
    }
}
impl<C: Widget> Widget for Padding<C> {
    type Command = C::Output;
    type Output = C::Output;
    fn update(&mut self, cx: &mut Update<'_, Self>, o: Self::Command) {
        let _ = cx.emit(o);
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let inset = self
            .inset
            .max(0.)
            .min(c.max.width / 2.)
            .min(c.max.height / 2.);
        let m = cx.measure(self.child, c.inset(inset));
        cx.place(self.child, Point::new(inset, inset));
        Metrics {
            size: c.constrain(Size::new(
                m.size.width + 2. * inset,
                m.size.height + 2. * inset,
            )),
            baseline: m.baseline.map(|b| b + inset),
        }
    }
}
/// The viewport derives content extent by measuring at its own width with unbounded height.
pub struct Scroll<C: Widget> {
    child: Child<C>,
    offset: f32,
    extent: f32,
    height: f32,
}
impl<C: Widget> Scroll<C> {
    pub fn new(content: Element<C>) -> Element<Self> {
        Element::build(|children| Self {
            child: children.forward(content),
            offset: 0.,
            extent: 0.,
            height: 0.,
        })
    }
    pub fn offset(&self) -> f32 {
        self.offset
    }
}
impl<C: Widget> Widget for Scroll<C> {
    type Command = C::Output;
    type Output = C::Output;
    fn update(&mut self, cx: &mut Update<'_, Self>, o: Self::Command) {
        let _ = cx.emit(o);
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let m = cx.measure(
            self.child,
            Constraints::loose(Size::new(c.max.width, f32::INFINITY)),
        );
        let size = c.constrain(Size::new(m.size.width, m.size.height.min(c.max.height)));
        self.extent = m.size.height;
        self.height = size.height;
        self.offset = self.offset.clamp(0., (self.extent - self.height).max(0.));
        cx.place(self.child, Point::new(0., -self.offset));
        Metrics::new(size)
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if phase != Phase::Preview {
            if let Input::Scroll { delta, .. } = input {
                let offset = (self.offset - delta.y).clamp(0., (self.extent - self.height).max(0.));
                if offset != self.offset {
                    self.offset = offset;
                    cx.relayout();
                    cx.stop()
                }
            }
        }
    }
}
