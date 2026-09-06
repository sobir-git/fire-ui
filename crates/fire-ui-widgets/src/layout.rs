use fire_ui::*;

#[derive(Clone, Copy)]
pub enum Axis {
    Horizontal,
    Vertical,
}
/// Each child has an explicit main-axis size or a proportional share of remaining space.
#[derive(Clone, Copy)]
pub enum Length {
    Auto,
    Fixed(f32),
    Fill(f32),
}
pub struct Flex {
    pub axis: Axis,
    pub gap: f32,
    pub padding: f32,
    pub lengths: Vec<Length>,
}
impl Flex {
    pub fn column(gap: f32) -> Self {
        Self {
            axis: Axis::Vertical,
            gap,
            padding: 0.,
            lengths: vec![],
        }
    }
    pub fn row(gap: f32) -> Self {
        Self {
            axis: Axis::Horizontal,
            ..Self::column(gap)
        }
    }
    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }
    pub fn lengths(mut self, lengths: impl IntoIterator<Item = Length>) -> Self {
        self.lengths = lengths.into_iter().collect();
        self
    }
}
impl Widget for Flex {
    fn layout(&mut self, children: &mut [Node], c: Constraints, text: &mut dyn TextEngine) -> Size {
        let vertical = matches!(self.axis, Axis::Vertical);
        let main = |s: Size| if vertical { s.height } else { s.width };
        let cross = |s: Size| if vertical { s.width } else { s.height };
        let size = |m: f32, x: f32| {
            if vertical {
                Size::new(x, m)
            } else {
                Size::new(m, x)
            }
        };
        let available = (main(c.max) - self.padding * 2.).max(0.);
        let breadth = (cross(c.max) - self.padding * 2.).max(0.);
        let visible = children.iter().filter(|n| !n.hidden).count();
        let mut used = self.gap * visible.saturating_sub(1) as f32;
        let mut weight = 0.;
        for (i, child) in children.iter_mut().enumerate() {
            if child.hidden {
                continue;
            }
            match self.lengths.get(i).copied().unwrap_or(Length::Auto) {
                Length::Fill(w) => weight += w.max(0.),
                Length::Fixed(n) => {
                    child.layout(Constraints::tight(size(n.max(0.), breadth)), text);
                    used += n.max(0.);
                }
                Length::Auto => {
                    let s = child.layout(Constraints::loose(size(available, breadth)), text);
                    used += main(s);
                }
            }
        }
        let mut cursor = self.padding;
        for (i, child) in children.iter_mut().enumerate() {
            if child.hidden {
                continue;
            }
            if let Length::Fill(w) = self.lengths.get(i).copied().unwrap_or(Length::Auto) {
                let n = if weight > 0. {
                    (available - used).max(0.) * w.max(0.) / weight
                } else {
                    0.
                };
                child.layout(Constraints::tight(size(n, breadth)), text);
            }
            if vertical {
                child.place(self.padding, cursor);
            } else {
                child.place(cursor, self.padding);
            }
            cursor += main(child.rect.size()) + self.gap;
        }
        let desired = (cursor - self.gap + self.padding).max(self.padding * 2.);
        c.constrain(size(desired, cross(c.max)))
    }
}

pub struct Panel {
    pub color: Color,
    pub padding: f32,
    pub radius: f32,
}
impl Widget for Panel {
    fn layout(&mut self, children: &mut [Node], c: Constraints, text: &mut dyn TextEngine) -> Size {
        let inner = Size::new(
            (c.max.width - 2. * self.padding).max(0.),
            (c.max.height - 2. * self.padding).max(0.),
        );
        for child in children {
            child.layout(Constraints::tight(inner), text);
            child.place(self.padding, self.padding);
        }
        c.max
    }
    fn paint(&self, ctx: &mut PaintCtx<'_>) {
        ctx.painter.rect(ctx.bounds, self.radius, self.color.into());
    }
}

/// Children overlap in paint order. Useful for HUDs, popovers, and modal hosts.
pub struct Stack;
impl Widget for Stack {
    fn layout(&mut self, children: &mut [Node], c: Constraints, text: &mut dyn TextEngine) -> Size {
        for child in children {
            child.layout(Constraints::tight(c.max), text);
            child.place(0., 0.);
        }
        c.max
    }
}

/// Vertically scrolls one child with the same offset for painting and hit testing.
pub struct ScrollView {
    pub offset: f32,
    pub content_height: f32,
    max_scroll: f32,
}
impl ScrollView {
    pub fn new(content_height: f32) -> Self {
        Self {
            offset: 0.,
            content_height,
            max_scroll: 0.,
        }
    }
}
impl Widget for ScrollView {
    fn layout(&mut self, children: &mut [Node], c: Constraints, text: &mut dyn TextEngine) -> Size {
        self.max_scroll = (self.content_height - c.max.height).max(0.);
        self.offset = self.offset.clamp(0., self.max_scroll);
        for child in children {
            child.layout(
                Constraints::tight(Size::new(c.max.width, self.content_height)),
                text,
            );
            child.place(0., -self.offset);
        }
        c.max
    }
    fn event(&mut self, ctx: &mut EventCtx<'_>, event: &Event) {
        if let Event::Scroll { delta, .. } = event {
            let offset = (self.offset - delta.y).clamp(0., self.max_scroll);
            if offset != self.offset {
                self.offset = offset;
                ctx.relayout();
                ctx.handle();
            }
        }
    }
}
