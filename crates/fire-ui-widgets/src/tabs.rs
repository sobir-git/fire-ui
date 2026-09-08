use crate::{theme, Appearance, Label, TextRole};
use fire_ui::*;

/// One selectable tab. Owns its label and publishes its own accessible identity,
/// so assistive technology sees a real tab list rather than a row of text.
pub struct Tab {
    caption: Child<Label>,
    text: String,
    selected: bool,
}
impl Tab {
    fn new(text: impl Into<String>) -> Element<Self> {
        let text = text.into();
        Element::build(|children| Self {
            caption: children.add(Element::leaf(
                Label::new(text.clone()).appearance(Appearance::role(TextRole::Body)),
            )),
            text,
            selected: false,
        })
    }
    /// Publishes the caption colour the current selection calls for.
    fn publish(&self, cx: &mut Update<'_, Self>) {
        let mut derived = cx
            .environment::<crate::Theme>()
            .copied()
            .unwrap_or_default();
        derived.color.foreground = if self.selected {
            derived.color.on_accent
        } else {
            derived.color.muted
        };
        let _ = cx.set_environment(self.caption, std::rc::Rc::new(derived), false);
    }
}
impl Widget for Tab {
    /// Whether this tab is the selected one.
    type Command = bool;
    /// Emitted when the tab is chosen.
    type Output = ();
    fn update(&mut self, cx: &mut Update<'_, Self>, selected: bool) {
        if self.selected != selected {
            self.selected = selected;
            self.publish(cx);
            cx.repaint()
        }
    }
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        match event {
            Lifecycle::Mount | Lifecycle::Inherited => self.publish(cx),
            Lifecycle::Hover(_) => cx.repaint(),
            _ => {}
        }
    }
    fn cursor(&self, _: Point) -> Option<CursorIcon> {
        Some(CursorIcon::Pointer)
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if phase == Phase::Preview {
            return;
        }
        if let Input::Button {
            button: 1,
            down: false,
            position,
            ..
        } = input
        {
            if cx.bounds().contains(*position) {
                let _ = cx.emit(());
                cx.stop()
            }
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let t = theme(cx);
        let inset = t.scale.space(1.75);
        let m = cx.measure(self.caption, c.inset(inset));
        let size = c.constrain(Size::new(
            m.size.width + 2. * inset,
            (m.size.height + t.scale.space(1.)).max(t.scale.control - 6.),
        ));
        cx.place(
            self.caption,
            Point::new(
                (size.width - m.size.width) / 2.,
                (size.height - m.size.height) / 2.,
            ),
        );
        Metrics::new(size)
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = crate::painted_theme(cx);
        let r = t.scale.radius;
        if self.selected {
            cx.painter
                .rect(cx.bounds, r, crate::accent_gradient(cx.bounds, &t));
        } else if cx.hovered {
            cx.painter.rect(cx.bounds, r, t.color.raised.into());
        }
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::Tab,
            label: self.text.clone(),
            selected: self.selected,
            actions: vec![SemanticActionKind::Activate],
            ..Semantics::default()
        }
    }
    fn accessibility(
        &mut self,
        cx: &mut Update<'_, Self>,
        action: SemanticAction,
    ) -> Result<(), SemanticError> {
        match action {
            SemanticAction::Activate => cx.emit(()).map_err(|_| SemanticError::Unavailable),
            _ => Err(SemanticError::Unsupported),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TabsCommand(pub usize);
impl Data for TabsCommand {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}

/// A row of tabs with exactly one selected.
///
/// The row owns arrow-key navigation and the selection invariant; each tab owns
/// its own pointer handling and accessible identity.
pub struct Tabs {
    tabs: Vec<Child<Tab>>,
    selected: usize,
    axis: crate::Axis,
}
impl Tabs {
    /// A horizontal tab strip.
    pub fn new(labels: impl IntoIterator<Item = impl Into<String>>) -> Element<Self> {
        Self::along(labels, crate::Axis::Horizontal)
    }
    /// A vertical rail, for a sidebar.
    pub fn vertical(labels: impl IntoIterator<Item = impl Into<String>>) -> Element<Self> {
        Self::along(labels, crate::Axis::Vertical)
    }
    pub fn along(
        labels: impl IntoIterator<Item = impl Into<String>>,
        axis: crate::Axis,
    ) -> Element<Self> {
        Element::build(|children| Self {
            tabs: labels
                .into_iter()
                .enumerate()
                .map(|(i, label)| children.connect(Tab::new(label), move |_| TabsCommand(i)))
                .collect(),
            selected: 0,
            axis,
        })
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    /// Selects `index`, telling both the old and new tab. Emits only on a change.
    fn select(&mut self, cx: &mut Update<'_, Self>, index: usize) {
        if index >= self.tabs.len() || index == self.selected {
            return;
        }
        let previous = self.selected;
        self.selected = index;
        let _ = cx.send(self.tabs[previous], false);
        let _ = cx.send(self.tabs[index], true);
        let _ = cx.emit(index);
        cx.repaint()
    }
}
impl Widget for Tabs {
    type Command = TabsCommand;
    /// The index of the newly selected tab.
    type Output = usize;
    fn update(&mut self, cx: &mut Update<'_, Self>, TabsCommand(index): TabsCommand) {
        self.select(cx, index)
    }
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        match event {
            Lifecycle::Mount => {
                if let Some(tab) = self.tabs.get(self.selected) {
                    let _ = cx.send(*tab, true);
                }
            }
            Lifecycle::Focus(_) => cx.repaint(),
            _ => {}
        }
    }
    fn focusable(&self) -> bool {
        !self.tabs.is_empty()
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if phase == Phase::Preview || self.tabs.is_empty() {
            return;
        }
        if let Input::Key {
            key, down: true, ..
        } = input
        {
            let last = self.tabs.len() - 1;
            let target = match key {
                Key::Left | Key::Up => self.selected.saturating_sub(1),
                Key::Right | Key::Down => (self.selected + 1).min(last),
                Key::Home => 0,
                Key::End => last,
                _ => return,
            };
            self.select(cx, target);
            cx.stop()
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let t = theme(cx);
        let pad = t.scale.space(0.5);
        let gap = t.scale.space(0.25);
        let vertical = self.axis == crate::Axis::Vertical;
        // A rail stretches its tabs to a common width; a strip lets each be its own.
        let inner = Constraints {
            min: Size::new(
                if vertical && c.max.width.is_finite() {
                    (c.max.width - 2. * pad).max(0.)
                } else {
                    0.
                },
                0.,
            ),
            max: Size::new((c.max.width - 2. * pad).max(0.), c.max.height),
        };
        let mut cursor = pad;
        let mut breadth: f32 = 0.;
        for tab in &self.tabs {
            let m = cx.measure(*tab, inner);
            cx.place(
                *tab,
                if vertical {
                    Point::new(pad, cursor)
                } else {
                    Point::new(cursor, pad)
                },
            );
            cursor += if vertical {
                m.size.height
            } else {
                m.size.width
            } + gap;
            breadth = breadth.max(if vertical {
                m.size.width
            } else {
                m.size.height
            })
        }
        let main = (cursor - gap + pad).max(0.);
        let cross = breadth + 2. * pad;
        Metrics::new(c.constrain(if vertical {
            Size::new(cross, main)
        } else {
            Size::new(main, cross)
        }))
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = crate::painted_theme(cx);
        let r = t.scale.panel_radius;
        cx.painter.rect(cx.bounds, r, t.color.sunken.into());
        cx.painter
            .stroke(cx.bounds.inset(0.5), r, t.scale.border, t.color.border);
        if cx.focused {
            crate::focus_ring(cx.painter, cx.bounds, r, &t);
        }
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::TabList,
            label: "Sections".into(),
            actions: vec![SemanticActionKind::Focus],
            ..Semantics::default()
        }
    }
    fn accessibility(
        &mut self,
        cx: &mut Update<'_, Self>,
        action: SemanticAction,
    ) -> Result<(), SemanticError> {
        match action {
            SemanticAction::Focus => cx.focus().map_err(|_| SemanticError::Unavailable),
            _ => Err(SemanticError::Unsupported),
        }
    }
}
