use crate::{theme, Scrollbar};
use fire_ui::*;

/// How much of the main axis a child claims.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    /// Whatever the child asks for.
    #[default]
    Natural,
    /// Exactly this many pixels.
    Fixed(f32),
    /// A share of what is left after natural and fixed children are measured.
    /// Weights are relative: two `Fill(1.)` children split the remainder evenly.
    /// Under an unbounded axis there is no remainder, and a filling child is
    /// measured naturally instead.
    Fill(f32),
}

/// Where leftover main-axis space goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    /// First and last flush to the edges, equal gaps between.
    SpaceBetween,
    /// Equal gaps between, half a gap at each edge.
    SpaceAround,
    /// Equal gaps between and at both edges.
    SpaceEvenly,
}

/// Where a child sits across the flow's other axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
    /// Fill the cross axis. The child is measured with a tight cross constraint.
    Stretch,
    /// Line text up on its baseline. Falls back to `Start` without one, and means
    /// `Start` in a column, where baselines do not share a line.
    Baseline,
}
impl Align {
    /// The offset that places `size` inside `available`.
    fn offset(self, available: f32, size: f32) -> f32 {
        match self {
            Self::Start | Self::Stretch | Self::Baseline => 0.,
            Self::Center => (available - size).max(0.) / 2.,
            Self::End => (available - size).max(0.),
        }
    }
}

/// The shape of a row or column: the gap between children and how they line up.
///
/// `Flow::gap(8.)` is the common case; the rest are opt-in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flow {
    pub gap: f32,
    pub justify: Justify,
    pub align: Align,
}
impl Flow {
    pub fn gap(gap: f32) -> Self {
        Self {
            gap,
            ..Self::default()
        }
    }
    pub fn justify(mut self, justify: Justify) -> Self {
        self.justify = justify;
        self
    }
    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }
}

/// One child's participation in a flow.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub child: LayoutChild,
    pub length: Length,
    /// Overrides the flow's cross-axis alignment for this child alone.
    pub align: Option<Align>,
}
impl Entry {
    pub fn new<W: Widget>(child: Child<W>, length: Length) -> Self {
        Self {
            child: child.into(),
            length,
            align: None,
        }
    }
    /// A child at its natural size.
    pub fn natural<W: Widget>(child: Child<W>) -> Self {
        Self::new(child, Length::Natural)
    }
    pub fn fixed<W: Widget>(child: Child<W>, length: f32) -> Self {
        Self::new(child, Length::Fixed(length))
    }
    /// A child taking an equal share of the remaining space.
    pub fn fill<W: Widget>(child: Child<W>) -> Self {
        Self::new(child, Length::Fill(1.))
    }
    /// A child taking a weighted share of the remaining space.
    pub fn weighted<W: Widget>(child: Child<W>, weight: f32) -> Self {
        Self::new(child, Length::Fill(weight))
    }
    pub fn aligned(mut self, align: Align) -> Self {
        self.align = Some(align);
        self
    }
}

/// A borrowed layout policy, not a second ownership tree.
///
/// The caller keeps its typed `Child` handles and its own widget state; `column`
/// only measures and places. Custom widgets use exactly this to lay themselves out.
pub fn column(cx: &mut Layout<'_>, c: Constraints, flow: Flow, children: &[Entry]) -> Metrics {
    linear(cx, Point::default(), c, flow, children, true)
}
pub fn row(cx: &mut Layout<'_>, c: Constraints, flow: Flow, children: &[Entry]) -> Metrics {
    linear(cx, Point::default(), c, flow, children, false)
}
/// A column laid out from `origin` instead of the widget's top-left corner.
///
/// Policies compose by sequencing: lay out one group, take its height, and start
/// the next below it. This is how a widget mixes a column with a grid without
/// inventing a container for every combination.
pub fn column_at(
    cx: &mut Layout<'_>,
    origin: Point,
    c: Constraints,
    flow: Flow,
    children: &[Entry],
) -> Metrics {
    linear(cx, origin, c, flow, children, true)
}
/// A row laid out from `origin`.
pub fn row_at(
    cx: &mut Layout<'_>,
    origin: Point,
    c: Constraints,
    flow: Flow,
    children: &[Entry],
) -> Metrics {
    linear(cx, origin, c, flow, children, false)
}

fn linear(
    cx: &mut Layout<'_>,
    origin: Point,
    c: Constraints,
    flow: Flow,
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
    let bounded = available.is_finite();
    // A stretched child is measured against the full cross extent, which only
    // exists when that axis is bounded.
    let stretch = |align: Align| align == Align::Stretch && breadth.is_finite();
    let limits = |m: f32, align: Align| Constraints {
        min: size(m, if stretch(align) { breadth } else { 0. }),
        max: size(m, breadth),
    };

    let gaps = flow.gap * children.len().saturating_sub(1) as f32;
    // Each child is offered only what its predecessors left, so a column of natural
    // children cannot measure past the space the flow actually has. Without this a
    // heading and a caption above a panel push the panel out of the bottom.
    let mut remaining = if bounded {
        (available - gaps).max(0.)
    } else {
        f32::INFINITY
    };
    let mut weight = 0.;
    let mut measured = vec![Metrics::default(); children.len()];
    for (i, entry) in children.iter().enumerate() {
        let align = entry.align.unwrap_or(flow.align);
        match entry.length {
            Length::Fill(w) if bounded => weight += w.max(0.),
            Length::Fixed(n) => {
                let n = n.max(0.).min(remaining);
                measured[i] = cx.measure_child(entry.child, limits(n, align));
                remaining = (remaining - main(measured[i].size)).max(0.)
            }
            _ => {
                measured[i] = cx.measure_child(
                    entry.child,
                    Constraints {
                        min: size(0., if stretch(align) { breadth } else { 0. }),
                        max: size(remaining, breadth),
                    },
                );
                remaining = (remaining - main(measured[i].size)).max(0.)
            }
        }
    }
    let slack = remaining;
    for (i, entry) in children.iter().enumerate() {
        if let Length::Fill(w) = entry.length {
            if bounded {
                let align = entry.align.unwrap_or(flow.align);
                let n = if weight > 0. {
                    slack * w.max(0.) / weight
                } else {
                    0.
                };
                measured[i] = cx.measure_child(entry.child, limits(n, align))
            }
        }
    }

    let extent: f32 = measured.iter().map(|m| main(m.size)).sum::<f32>() + gaps;
    // Filling children have already consumed the slack; only distribute what is
    // still unclaimed, and only when the axis is bounded.
    let free = if bounded {
        (available - extent).max(0.)
    } else {
        0.
    };
    let count = children.len() as f32;
    let (mut cursor, spacing) = match flow.justify {
        Justify::Start => (0., flow.gap),
        Justify::Center => (free / 2., flow.gap),
        Justify::End => (free, flow.gap),
        Justify::SpaceBetween if children.len() > 1 => (0., flow.gap + free / (count - 1.)),
        Justify::SpaceBetween => (0., flow.gap),
        Justify::SpaceAround => (free / count / 2., flow.gap + free / count),
        Justify::SpaceEvenly => (free / (count + 1.), flow.gap + free / (count + 1.)),
    };

    // A row shares one baseline; a column reports its first child's.
    let baseline = if vertical {
        measured.first().and_then(|m| m.baseline)
    } else if flow.align == Align::Baseline {
        measured
            .iter()
            .zip(children)
            .filter(|(_, e)| e.align.unwrap_or(flow.align) == Align::Baseline)
            .map(|(m, _)| m.baseline.unwrap_or(m.size.height))
            .reduce(f32::max)
    } else {
        None
    };

    let span = if breadth.is_finite() {
        breadth
    } else {
        measured.iter().map(|m| cross(m.size)).fold(0., f32::max)
    };
    let mut widest: f32 = 0.;
    for (entry, m) in children.iter().zip(&measured) {
        let align = entry.align.unwrap_or(flow.align);
        let offset = if !vertical && align == Align::Baseline {
            baseline.unwrap_or(0.) - m.baseline.unwrap_or(m.size.height)
        } else {
            align.offset(span, cross(m.size))
        };
        cx.place_child(
            entry.child,
            if vertical {
                Point::new(origin.x + offset, origin.y + cursor)
            } else {
                Point::new(origin.x + cursor, origin.y + offset)
            },
        );
        cursor += main(m.size) + spacing;
        widest = widest.max(cross(m.size) + offset.max(0.))
    }
    Metrics {
        size: c.constrain(size(
            if children.is_empty() {
                0.
            } else {
                (cursor - spacing).max(0.)
            },
            widest,
        )),
        baseline,
    }
}

/// A child in a stack, and where it sits.
#[derive(Clone, Copy, Debug)]
pub struct Layer {
    pub child: LayoutChild,
    pub horizontal: Align,
    pub vertical: Align,
}
impl Layer {
    /// A layer filling the stack.
    pub fn new<W: Widget>(child: Child<W>) -> Self {
        Self {
            child: child.into(),
            horizontal: Align::Stretch,
            vertical: Align::Stretch,
        }
    }
    /// A layer at its natural size, placed at the given alignment.
    pub fn at<W: Widget>(child: Child<W>, horizontal: Align, vertical: Align) -> Self {
        Self {
            child: child.into(),
            horizontal,
            vertical,
        }
    }
}

/// Places children on top of one another, in the order given: last draws last.
///
/// The stack takes the size of its largest child, or the full constraint when any
/// layer stretches.
pub fn stack(cx: &mut Layout<'_>, c: Constraints, layers: &[Layer]) -> Metrics {
    stack_at(cx, Point::default(), c, layers)
}
/// A stack laid out from `origin`.
pub fn stack_at(cx: &mut Layout<'_>, origin: Point, c: Constraints, layers: &[Layer]) -> Metrics {
    let mut measured = vec![Metrics::default(); layers.len()];
    for (i, layer) in layers.iter().enumerate() {
        let tight = |align: Align, extent: f32| {
            if align == Align::Stretch && extent.is_finite() {
                extent
            } else {
                0.
            }
        };
        measured[i] = cx.measure_child(
            layer.child,
            Constraints {
                min: Size::new(
                    tight(layer.horizontal, c.max.width),
                    tight(layer.vertical, c.max.height),
                ),
                max: c.max,
            },
        );
    }
    let size = c.constrain(Size::new(
        measured.iter().map(|m| m.size.width).fold(0., f32::max),
        measured.iter().map(|m| m.size.height).fold(0., f32::max),
    ));
    for (layer, m) in layers.iter().zip(&measured) {
        cx.place_child(
            layer.child,
            Point::new(
                origin.x + layer.horizontal.offset(size.width, m.size.width),
                origin.y + layer.vertical.offset(size.height, m.size.height),
            ),
        );
    }
    Metrics::new(size)
}

/// Lays children out in equal-width columns, wrapping onto new rows.
///
/// `columns` of zero fits as many `min_width` columns as the constraint allows,
/// which is how a panel grid reflows when a window narrows.
pub fn grid(
    cx: &mut Layout<'_>,
    c: Constraints,
    columns: usize,
    min_width: f32,
    gap: f32,
    children: &[LayoutChild],
) -> Metrics {
    grid_at(cx, Point::default(), c, columns, min_width, gap, children)
}
/// A grid laid out from `origin`.
#[allow(clippy::too_many_arguments)]
pub fn grid_at(
    cx: &mut Layout<'_>,
    origin: Point,
    c: Constraints,
    columns: usize,
    min_width: f32,
    gap: f32,
    children: &[LayoutChild],
) -> Metrics {
    if children.is_empty() {
        return Metrics::new(c.constrain(Size::ZERO));
    }
    let width = if c.max.width.is_finite() {
        c.max.width
    } else {
        min_width.max(1.) * children.len() as f32
    };
    let columns = if columns > 0 {
        columns
    } else {
        (((width + gap) / (min_width.max(1.) + gap)).floor() as usize).clamp(1, children.len())
    };
    let column_width = ((width - gap * (columns - 1) as f32) / columns as f32).max(0.);
    let mut y = 0.;
    let mut row_height: f32 = 0.;
    for (i, child) in children.iter().enumerate() {
        let index = i % columns;
        if index == 0 && i > 0 {
            y += row_height + gap;
            row_height = 0.
        }
        let m = cx.measure_child(
            *child,
            Constraints {
                min: Size::new(column_width, 0.),
                max: Size::new(column_width, c.max.height),
            },
        );
        cx.place_child(
            *child,
            Point::new(origin.x + index as f32 * (column_width + gap), origin.y + y),
        );
        row_height = row_height.max(m.size.height)
    }
    Metrics::new(c.constrain(Size::new(width, y + row_height)))
}

/// Space on each side of something.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}
impl Insets {
    pub const fn all(n: f32) -> Self {
        Self {
            left: n,
            top: n,
            right: n,
            bottom: n,
        }
    }
    pub const fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self {
            left: horizontal,
            top: vertical,
            right: horizontal,
            bottom: vertical,
        }
    }
    pub const fn only(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
    pub fn horizontal(self) -> f32 {
        self.left + self.right
    }
    pub fn vertical(self) -> f32 {
        self.top + self.bottom
    }
    /// Shrinks constraints by these insets, never below zero.
    pub fn shrink(self, c: Constraints) -> Constraints {
        let take = |extent: f32, by: f32| {
            if extent.is_finite() {
                (extent - by).max(0.)
            } else {
                extent
            }
        };
        Constraints {
            min: Size::new(
                take(c.min.width, self.horizontal()),
                take(c.min.height, self.vertical()),
            ),
            max: Size::new(
                take(c.max.width, self.horizontal()),
                take(c.max.height, self.vertical()),
            ),
        }
    }
}
impl From<f32> for Insets {
    fn from(n: f32) -> Self {
        Self::all(n)
    }
}

/// Space around content. Transparent in both directions: commands reach the
/// content and its output passes straight through.
pub struct Padding<C: Widget> {
    child: Child<C>,
    insets: Insets,
}
impl<C: Widget> Padding<C> {
    pub fn new(content: Element<C>, insets: impl Into<Insets>) -> Element<Self> {
        let insets = insets.into();
        Element::build(|children| Self {
            child: children.bubble(content),
            insets,
        })
    }
}
impl<C: Widget> Widget for Padding<C> {
    type Command = C::Command;
    type Output = C::Output;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: Self::Command) {
        let _ = cx.send(self.child, command);
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let m = cx.measure(self.child, self.insets.shrink(c));
        cx.place(self.child, Point::new(self.insets.left, self.insets.top));
        Metrics {
            size: c.constrain(Size::new(
                m.size.width + self.insets.horizontal(),
                m.size.height + self.insets.vertical(),
            )),
            baseline: m.baseline.map(|b| b + self.insets.top),
        }
    }
}

/// Places content within the space it is given.
pub struct Aligned<C: Widget> {
    child: Child<C>,
    horizontal: Align,
    vertical: Align,
}
impl<C: Widget> Aligned<C> {
    pub fn new(content: Element<C>, horizontal: Align, vertical: Align) -> Element<Self> {
        Element::build(|children| Self {
            child: children.bubble(content),
            horizontal,
            vertical,
        })
    }
    /// Content centred on both axes.
    pub fn center(content: Element<C>) -> Element<Self> {
        Self::new(content, Align::Center, Align::Center)
    }
}
impl<C: Widget> Widget for Aligned<C> {
    type Command = C::Command;
    type Output = C::Output;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: Self::Command) {
        let _ = cx.send(self.child, command);
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let m = cx.measure(self.child, Constraints::loose(c.max));
        // Alignment needs room to align within, so claim the whole constraint on
        // any axis that is both bounded and not asking for the natural size.
        let size = c.constrain(Size::new(
            if c.max.width.is_finite() && self.horizontal != Align::Start {
                c.max.width
            } else {
                m.size.width
            },
            if c.max.height.is_finite() && self.vertical != Align::Start {
                c.max.height
            } else {
                m.size.height
            },
        ));
        let offset = Point::new(
            self.horizontal.offset(size.width, m.size.width),
            self.vertical.offset(size.height, m.size.height),
        );
        cx.place(self.child, offset);
        Metrics {
            size,
            baseline: m.baseline.map(|b| b + offset.y),
        }
    }
}

/// Bounds on how large or small content may be.
pub struct Constrain<C: Widget> {
    child: Child<C>,
    min: Size,
    max: Size,
}
impl<C: Widget> Constrain<C> {
    /// Content held between `min` and `max`. Use `f32::INFINITY` for no maximum
    /// and zero for no minimum.
    pub fn new(content: Element<C>, min: Size, max: Size) -> Element<Self> {
        Element::build(|children| Self {
            child: children.bubble(content),
            min,
            max,
        })
    }
    /// Content that never grows past `width`, whatever room it is offered.
    pub fn width(content: Element<C>, width: f32) -> Element<Self> {
        Self::new(content, Size::ZERO, Size::new(width, f32::INFINITY))
    }
    /// Content at exactly this size.
    pub fn exact(content: Element<C>, size: Size) -> Element<Self> {
        Self::new(content, size, size)
    }
}
impl<C: Widget> Widget for Constrain<C> {
    type Command = C::Command;
    type Output = C::Output;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: Self::Command) {
        let _ = cx.send(self.child, command);
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let clamp = |value: f32, low: f32, high: f32| value.max(low).min(high);
        let limits = Constraints {
            min: Size::new(
                clamp(self.min.width, c.min.width, c.max.width),
                clamp(self.min.height, c.min.height, c.max.height),
            ),
            max: Size::new(
                clamp(self.max.width, c.min.width, c.max.width),
                clamp(self.max.height, c.min.height, c.max.height),
            ),
        };
        let m = cx.measure(self.child, limits);
        cx.place(self.child, Point::default());
        Metrics {
            size: limits.constrain(m.size),
            baseline: m.baseline,
        }
    }
}

/// Blank space. In a flow, `Entry::fill` on a spacer pushes its neighbours apart.
#[derive(Default)]
pub struct Spacer {
    size: Size,
}
impl Spacer {
    /// A spacer with no natural size, for use with `Entry::fill`.
    pub fn flexible() -> Element<Self> {
        Element::leaf(Self::default())
    }
    /// A fixed gap, for when the flow's own gap is not the right one here.
    pub fn fixed(size: Size) -> Element<Self> {
        Element::leaf(Self { size })
    }
}
impl Widget for Spacer {
    type Command = std::convert::Infallible;
    type Output = std::convert::Infallible;
    fn layout(&mut self, _: &mut Layout<'_>, c: Constraints) -> Metrics {
        Metrics::new(c.constrain(self.size))
    }
}

/// Which way a viewport scrolls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    #[default]
    Vertical,
    Horizontal,
}

/// A viewport over content taller than itself.
///
/// The content is measured at the viewport's own width with unbounded height, so
/// its natural extent decides how far the view can travel. A scrollbar appears
/// only while there is somewhere to go, and can be dragged.
pub struct Scroll<C: Widget> {
    child: Child<C>,
    offset: f32,
    extent: f32,
    height: f32,
    dragging: Option<(u32, f32)>,
    hovered: bool,
    /// Whether a bar is showing, and therefore whether a strip is reserved.
    reserved: bool,
    /// Whether the pointer is resting on the bar.
    bar_hover: bool,
    /// Viewport width and scrollbar strip from the last layout. `cursor` has no
    /// context to consult, so these two numbers are all it needs remembered.
    width: f32,
    bar: f32,
}
impl<C: Widget> Scroll<C> {
    pub fn new(content: Element<C>) -> Element<Self> {
        Element::build(|children| Self {
            child: children.bubble(content),
            offset: 0.,
            extent: 0.,
            height: 0.,
            dragging: None,
            hovered: false,
            reserved: false,
            bar_hover: false,
            width: 0.,
            bar: 0.,
        })
    }
    pub fn offset(&self) -> f32 {
        self.offset
    }
    fn overflow(&self) -> f32 {
        (self.extent - self.height).max(0.)
    }
    /// The bar for the current geometry, or `None` when everything fits.
    fn bar(&self, bounds: Rect) -> Option<Scrollbar> {
        Scrollbar::new(bounds, self.height, self.extent, self.offset, self.bar)
    }
    fn scroll_to(&mut self, cx: &mut Update<'_, Self>, offset: f32) {
        let offset = offset.clamp(0., self.overflow());
        if offset != self.offset {
            self.offset = offset;
            cx.relayout()
        }
    }
}
impl<C: Widget> Widget for Scroll<C> {
    type Command = C::Command;
    type Output = C::Output;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: Self::Command) {
        let _ = cx.send(self.child, command);
    }
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        match event {
            Lifecycle::Hover(hovered) => {
                self.hovered = hovered;
                if !hovered {
                    self.bar_hover = false
                }
                cx.repaint()
            }
            Lifecycle::CaptureLost(_) => {
                self.dragging = None;
                cx.repaint()
            }
            _ => {}
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        self.width = c.max.width;
        self.bar = Scrollbar::width(&theme(cx));
        let bar = self.bar;
        let unbounded = |width: f32| Constraints::loose(Size::new(width, f32::INFINITY));
        let mut m = cx.measure(self.child, unbounded(c.max.width));
        // A bar takes its own strip rather than floating over the content, so the
        // content is measured again beside it once we know one is needed.
        self.reserved = m.size.height > c.max.height;
        if self.reserved {
            m = cx.measure(self.child, unbounded((c.max.width - bar).max(0.)));
        }
        let size = c.constrain(Size::new(c.max.width, m.size.height.min(c.max.height)));
        self.extent = m.size.height;
        self.height = size.height;
        self.offset = self.offset.clamp(0., self.overflow());
        cx.place(self.child, Point::new(0., -self.offset));
        Metrics::new(size)
    }

    /// The pointer over the bar is not over the content. Reserving the strip already
    /// keeps the content out from under it; saying so here means a child that ignores
    /// its constraints cannot put a caret over the scrollbar either.
    fn cursor(&self, position: Point) -> Option<CursorIcon> {
        self.reserved
            .then_some(CursorIcon::Arrow)
            .filter(|_| position.x >= self.width - self.bar)
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if phase == Phase::Preview {
            return;
        }
        match input {
            Input::Scroll { delta, .. } => {
                let offset = self.offset - delta.y;
                if offset.clamp(0., self.overflow()) != self.offset {
                    self.scroll_to(cx, offset);
                    cx.stop()
                }
            }
            Input::Button {
                pointer,
                button: 1,
                down: true,
                position,
            } => {
                let Some(bar) = self.bar(cx.bounds()) else {
                    return;
                };
                if !bar.track.contains(*position) {
                    return;
                }
                let _ = cx.capture(*pointer);
                if bar.thumb.contains(*position) {
                    self.dragging = Some((*pointer, position.y - bar.thumb.y))
                } else {
                    // Jump so the thumb centres on the click, then keep dragging.
                    self.dragging = Some((*pointer, bar.thumb.height / 2.));
                    let offset =
                        bar.offset_for(position.y - bar.thumb.height / 2., self.overflow());
                    self.scroll_to(cx, offset)
                }
                cx.repaint();
                cx.stop()
            }
            Input::Pointer { position, .. } => {
                if let Some((_, grip)) = self.dragging {
                    let Some(bar) = self.bar(cx.bounds()) else {
                        return;
                    };
                    let offset = bar.offset_for(position.y - grip, self.overflow());
                    self.scroll_to(cx, offset);
                    cx.stop();
                    return;
                }
                let hovered = self
                    .bar(cx.bounds())
                    .is_some_and(|bar| bar.contains(*position));
                if hovered != self.bar_hover {
                    self.bar_hover = hovered;
                    cx.repaint()
                }
            }
            Input::Button {
                pointer,
                button: 1,
                down: false,
                ..
            } if self.dragging.map(|(p, _)| p) == Some(*pointer) => {
                self.dragging = None;
                let _ = cx.release(*pointer);
                cx.repaint();
                cx.stop()
            }
            _ => {}
        }
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = crate::painted_theme(cx);
        if let Some(bar) = self.bar(cx.bounds) {
            bar.paint(cx.painter, &t, self.bar_hover, self.dragging.is_some())
        }
    }
}

/// A row or column gap taken from the theme's spacing rhythm.
pub fn gap(cx: &Layout<'_>, steps: f32) -> f32 {
    theme(cx).scale.space(steps)
}
