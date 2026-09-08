use crate::{theme, Appearance, Label, Menu, MenuItem, MenuOutput, TextRole};
use fire_ui::*;
use std::{rc::Rc, sync::Arc};

#[derive(Clone, Debug)]
pub enum DropdownCommand {
    /// Choose an option by index. Out-of-range indices are ignored.
    Select(usize),
    Disabled(bool),
    /// Replace the whole option list. The selection resets to the first option.
    Options(Vec<Arc<str>>),
}
impl Data for DropdownCommand {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Options(options) => options.iter().map(|o| o.len()).sum::<usize>(),
                _ => 0,
            }
    }
}
/// Routed internally: the list's own result, or a row the owner never sees.
pub enum Routed {
    Command(DropdownCommand),
    Chosen(MenuOutput<usize>),
}
impl Data for Routed {
    fn bytes(&self) -> usize {
        match self {
            Self::Command(c) => c.bytes(),
            Self::Chosen(c) => c.bytes(),
        }
    }
}

/// A field showing one of several options, with a list that opens on demand.
///
/// The list is the ordinary `Menu`, opened as a modal overlay and positioned under
/// the field in window coordinates, so it escapes any scrolling or clipping the
/// field itself sits inside.
pub struct Dropdown {
    options: Vec<Arc<str>>,
    selected: usize,
    caption: Child<Label>,
    list: Option<Child<Menu<usize>>>,
    label: String,
    disabled: bool,
}
impl Dropdown {
    pub fn new(
        label: impl Into<String>,
        options: impl IntoIterator<Item = impl Into<Arc<str>>>,
    ) -> Element<Self> {
        let options: Vec<Arc<str>> = options.into_iter().map(Into::into).collect();
        let first = options.first().cloned().unwrap_or_else(|| Arc::from(""));
        let label = label.into();
        Element::build(|children| Self {
            caption: children.add(Element::leaf(
                Label::new(first).appearance(Appearance::role(TextRole::Body)),
            )),
            options,
            selected: 0,
            list: None,
            label,
            disabled: false,
        })
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    pub fn value(&self) -> Option<&Arc<str>> {
        self.options.get(self.selected)
    }
    fn open(&mut self, cx: &mut Update<'_, Self>) {
        if self.list.is_some() || self.disabled || self.options.is_empty() {
            return;
        }
        let items = self
            .options
            .iter()
            .enumerate()
            .map(|(i, option)| MenuItem::new(i, option.clone()).checked(i == self.selected))
            .collect();
        let field = cx.window_bounds();
        let at = Point::new(field.x, field.y + field.height + 4.);
        // Born an overlay against the window, so the list is neither positioned nor
        // clipped by whatever panel the field happens to sit in.
        match cx.insert_at(Menu::new(items, at), Anchor::<Self>::Window, |o| {
            Routed::Chosen(o.clone())
        }) {
            Ok(list) => {
                self.list = Some(list);
                let _ = cx.open_modal(list);
                let _ = cx.focus_child(list);
                cx.relayout();
                cx.repaint()
            }
            Err(_) => cx.repaint(),
        }
    }
    fn close(&mut self, cx: &mut Update<'_, Self>) {
        if let Some(list) = self.list.take() {
            let _ = cx.close_modal();
            let _ = cx.remove(list);
            let _ = cx.focus();
            cx.repaint()
        }
    }
    fn choose(&mut self, cx: &mut Update<'_, Self>, index: usize) {
        if index >= self.options.len() || index == self.selected {
            return;
        }
        self.selected = index;
        let _ = cx.send(self.caption, self.options[index].to_string());
        let _ = cx.emit(index);
        cx.repaint()
    }
    /// Publishes the caption colour, which dims when the control is disabled.
    fn publish(&self, cx: &mut Update<'_, Self>) {
        let mut derived = cx
            .environment::<crate::Theme>()
            .copied()
            .unwrap_or_default();
        if self.disabled {
            derived.color.foreground = derived.color.faint
        }
        let _ = cx.set_environment(self.caption, Rc::new(derived), false);
    }
    fn chevron(t: &crate::Theme) -> f32 {
        (t.scale.font_size * 0.7).round()
    }
}
impl Widget for Dropdown {
    type Command = Routed;
    /// The index of the newly chosen option.
    type Output = usize;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: Routed) {
        match command {
            Routed::Chosen(MenuOutput::Selected(index)) => {
                self.close(cx);
                self.choose(cx, index)
            }
            Routed::Chosen(MenuOutput::Dismissed) => self.close(cx),
            Routed::Command(DropdownCommand::Select(index)) => {
                if index < self.options.len() && index != self.selected {
                    self.selected = index;
                    let _ = cx.send(self.caption, self.options[index].to_string());
                    cx.repaint()
                }
            }
            Routed::Command(DropdownCommand::Disabled(value)) => {
                self.disabled = value;
                if value {
                    self.close(cx)
                }
                self.publish(cx);
                cx.repaint()
            }
            Routed::Command(DropdownCommand::Options(options)) => {
                self.close(cx);
                self.options = options;
                self.selected = 0;
                let first = self
                    .options
                    .first()
                    .map(|o| o.to_string())
                    .unwrap_or_default();
                let _ = cx.send(self.caption, first);
                cx.relayout()
            }
        }
    }
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        match event {
            Lifecycle::Mount | Lifecycle::Inherited => self.publish(cx),
            Lifecycle::Hover(_) | Lifecycle::Focus(_) => cx.repaint(),
            Lifecycle::Visibility(false) => self.close(cx),
            _ => {}
        }
    }
    fn focusable(&self) -> bool {
        !self.disabled
    }
    fn cursor(&self, _: Point) -> Option<CursorIcon> {
        Some(if self.disabled {
            CursorIcon::Arrow
        } else {
            CursorIcon::Pointer
        })
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if phase == Phase::Preview || self.disabled {
            return;
        }
        match input {
            Input::Button {
                button: 1,
                down: true,
                position,
                ..
            } if cx.bounds().contains(*position) => {
                let _ = cx.focus();
                self.open(cx);
                cx.stop()
            }
            Input::Key {
                key: Key::Enter | Key::Character(' ') | Key::Down,
                down: true,
                repeat: false,
                ..
            } => {
                self.open(cx);
                cx.stop()
            }
            _ => {}
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let t = theme(cx);
        let inset = t.scale.inset;
        let chevron = Self::chevron(&t);
        let gap = t.scale.space(1.);
        let m = cx.measure(
            self.caption,
            Constraints::loose(Size::new(
                (c.max.width - 2. * inset - chevron - gap).max(0.),
                c.max.height,
            )),
        );
        let size = c.constrain(Size::new(
            if c.max.width.is_finite() {
                c.max.width
            } else {
                m.size.width + 2. * inset + chevron + gap
            },
            (m.size.height + 2. * inset).max(t.scale.control),
        ));
        cx.place(
            self.caption,
            Point::new(inset, (size.height - m.size.height) / 2.),
        );
        if let Some(list) = self.list {
            // Window-anchored: measured against the whole window and placed at its
            // origin, so the menu positions itself in window coordinates.
            let viewport = cx.viewport();
            cx.measure(list, Constraints::tight(viewport.size()));
            cx.place(list, Point::new(viewport.x, viewport.y));
        }
        Metrics {
            size,
            baseline: m.baseline.map(|b| b + (size.height - m.size.height) / 2.),
        }
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = crate::painted_theme(cx);
        let r = t.scale.radius;
        let open = self.list.is_some();
        cx.painter.rect(
            cx.bounds,
            r,
            if self.disabled {
                t.color.surface.alpha(0.6)
            } else if open || cx.hovered {
                t.color.raised
            } else {
                t.color.sunken
            }
            .into(),
        );
        cx.painter.stroke(
            cx.bounds.inset(0.5),
            r,
            t.scale.border,
            if open {
                t.color.accent
            } else if cx.hovered && !self.disabled {
                t.color.border_strong
            } else {
                t.color.border
            },
        );
        // The chevron points down when closed and up while the list is showing.
        let size = Self::chevron(&t);
        let x = cx.bounds.x + cx.bounds.width - t.scale.inset - size;
        let y = cx.bounds.y + (cx.bounds.height - size * 0.55) / 2.;
        let (top, bottom) = if open {
            (y + size * 0.55, y)
        } else {
            (y, y + size * 0.55)
        };
        cx.painter.path(
            &[
                Path::Move(Point::new(x, top)),
                Path::Line(Point::new(x + size / 2., bottom)),
                Path::Line(Point::new(x + size, top)),
            ],
            if self.disabled {
                t.color.faint
            } else {
                t.color.muted
            }
            .into(),
            Some(1.8),
        );
        if cx.focused && !self.disabled {
            crate::focus_ring(cx.painter, cx.bounds, r, &t);
        }
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::Menu,
            label: self.label.clone(),
            value: self.options.get(self.selected).map(|o| o.to_string()),
            disabled: self.disabled,
            actions: vec![SemanticActionKind::Activate, SemanticActionKind::Focus],
            ..Semantics::default()
        }
    }
    fn accessibility(
        &mut self,
        cx: &mut Update<'_, Self>,
        action: SemanticAction,
    ) -> Result<(), SemanticError> {
        if self.disabled {
            return Err(SemanticError::Unavailable);
        }
        match action {
            SemanticAction::Focus => cx.focus().map_err(|_| SemanticError::Unavailable),
            SemanticAction::Activate => {
                self.open(cx);
                Ok(())
            }
            SemanticAction::SetValue(value) => {
                let index = self
                    .options
                    .iter()
                    .position(|o| **o == *value)
                    .ok_or(SemanticError::InvalidValue)?;
                self.choose(cx, index);
                Ok(())
            }
            _ => Err(SemanticError::Unsupported),
        }
    }
}
