use crate::{theme, Appearance, Label, TextRole};
use fire_ui::*;

/// Commands shared by the two-state controls.
#[derive(Clone, Debug)]
pub enum ToggleCommand {
    Checked(bool),
    Disabled(bool),
    Label(String),
}
impl Data for ToggleCommand {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Label(label) => label.capacity(),
                _ => 0,
            }
    }
}

/// State and input shared by `Checkbox` and `Switch`, which differ only in how
/// they draw and which role they publish.
struct Toggle {
    caption: Child<Label>,
    text: String,
    checked: bool,
    disabled: bool,
}
impl Toggle {
    fn new(children: &mut Children<impl Widget>, text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            caption: children.add(Element::leaf(
                Label::new(text.clone()).appearance(Appearance::role(TextRole::Body)),
            )),
            text,
            checked: false,
            disabled: false,
        }
    }
    /// Returns the new state when it changed, so the owner can emit it.
    fn command<W: Widget>(&mut self, cx: &mut Update<'_, W>, command: ToggleCommand) -> bool {
        match command {
            ToggleCommand::Checked(value) => {
                if self.checked == value {
                    return false;
                }
                self.checked = value
            }
            ToggleCommand::Disabled(value) => self.disabled = value,
            ToggleCommand::Label(text) => {
                let _ = cx.send(self.caption, text.clone());
                self.text = text;
            }
        }
        cx.repaint();
        true
    }
    /// Whether this input activates the control.
    fn activates(&self, cx: &Update<'_, impl Widget>, phase: Phase, input: &Input) -> bool {
        if phase == Phase::Preview || self.disabled {
            return false;
        }
        match input {
            Input::Button {
                button: 1,
                down: false,
                position,
                ..
            } => cx.bounds().contains(*position),
            Input::Key {
                key: Key::Character(' ') | Key::Enter,
                down: true,
                repeat: false,
                ..
            } => true,
            _ => false,
        }
    }
    fn semantics(&self, role: Role) -> Semantics {
        Semantics {
            role,
            label: self.text.clone(),
            checked: Some(self.checked),
            disabled: self.disabled,
            actions: vec![SemanticActionKind::Activate, SemanticActionKind::Focus],
            ..Semantics::default()
        }
    }
    /// Places the caption after a control box of `box_size`, reserving `halo` around
    /// the box so the glow it paints stays inside this widget's own bounds.
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints, box_size: Size) -> Metrics {
        let t = theme(cx);
        let gap = t.scale.space(1.25);
        let halo = t.scale.halo;
        let lead = box_size.width + 2. * halo + gap;
        let m = cx.measure(
            self.caption,
            Constraints::loose(Size::new((c.max.width - lead).max(0.), c.max.height)),
        );
        let height = m
            .size
            .height
            .max(t.scale.control * 0.62)
            .max(box_size.height + 2. * halo);
        cx.place(
            self.caption,
            Point::new(lead, (height - m.size.height) / 2.),
        );
        Metrics {
            size: c.constrain(Size::new(lead + m.size.width, height)),
            baseline: m.baseline.map(|b| b + (height - m.size.height) / 2.),
        }
    }
    /// The rectangle the visible box occupies, inside the reserved halo.
    fn box_rect(bounds: Rect, size: Size, halo: f32) -> Rect {
        Rect::new(
            bounds.x + halo,
            bounds.y + (bounds.height - size.height) / 2.,
            size.width,
            size.height,
        )
    }
}

/// A labelled box that is either checked or not.
pub struct Checkbox(Toggle);
impl Checkbox {
    pub fn new(text: impl Into<String>) -> Element<Self> {
        Element::build(|children| Self(Toggle::new(children, text)))
    }
    pub fn checked(&self) -> bool {
        self.0.checked
    }
    fn size(t: &crate::Theme) -> f32 {
        (t.scale.font_size * 1.25).round()
    }
}
impl Widget for Checkbox {
    type Command = ToggleCommand;
    type Output = bool;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: ToggleCommand) {
        self.0.command(cx, command);
    }
    fn focusable(&self) -> bool {
        !self.0.disabled
    }
    fn cursor(&self, _: Point) -> Option<CursorIcon> {
        Some(if self.0.disabled {
            CursorIcon::Arrow
        } else {
            CursorIcon::Pointer
        })
    }
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        if matches!(event, Lifecycle::Hover(_) | Lifecycle::Focus(_)) {
            cx.repaint()
        }
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if matches!(
            input,
            Input::Button {
                button: 1,
                down: true,
                ..
            }
        ) && phase != Phase::Preview
            && !self.0.disabled
        {
            let _ = cx.focus();
            cx.stop();
            return;
        }
        if self.0.activates(cx, phase, input) {
            self.0.checked = !self.0.checked;
            let _ = cx.emit(self.0.checked);
            cx.repaint();
            cx.stop()
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let side = Self::size(&theme(cx));
        self.0.layout(cx, c, Size::new(side, side))
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = crate::painted_theme(cx);
        let side = Self::size(&t);
        let r = Toggle::box_rect(cx.bounds, Size::new(side, side), t.scale.halo);
        let radius = t.scale.radius * 0.5;
        if self.0.checked && !self.0.disabled {
            crate::glow(cx.painter, r, radius, 8., t.color.accent.alpha(0.4));
            cx.painter.rect(r, radius, crate::accent_gradient(r, &t));
            // A check drawn as two strokes, scaled to the box.
            let s = |fx: f32, fy: f32| Point::new(r.x + side * fx, r.y + side * fy);
            cx.painter.path(
                &[
                    Path::Move(s(0.24, 0.52)),
                    Path::Line(s(0.43, 0.71)),
                    Path::Line(s(0.77, 0.31)),
                ],
                t.color.on_accent.into(),
                Some((side * 0.14).max(1.5)),
            );
        } else {
            cx.painter.rect(
                r,
                radius,
                if cx.hovered && !self.0.disabled {
                    t.color.raised
                } else {
                    t.color.sunken
                }
                .into(),
            );
            cx.painter.stroke(
                r.inset(0.5),
                radius,
                t.scale.border,
                if self.0.disabled {
                    t.color.border
                } else {
                    t.color.border_strong
                },
            );
        }
        if cx.focused && !self.0.disabled {
            crate::focus_ring(cx.painter, r.inset(-2.), radius + 2., &t);
        }
    }
    fn semantics(&self) -> Semantics {
        self.0.semantics(Role::CheckBox)
    }
    fn accessibility(
        &mut self,
        cx: &mut Update<'_, Self>,
        action: SemanticAction,
    ) -> Result<(), SemanticError> {
        toggle_action(&mut self.0, cx, action, |cx, checked| cx.emit(checked))
    }
}

/// A labelled switch: the same state as a checkbox, for settings that take effect
/// immediately rather than on submit.
pub struct Switch(Toggle);
impl Switch {
    pub fn new(text: impl Into<String>) -> Element<Self> {
        Element::build(|children| Self(Toggle::new(children, text)))
    }
    pub fn checked(&self) -> bool {
        self.0.checked
    }
    fn track(t: &crate::Theme) -> Size {
        let h = (t.scale.font_size * 1.2).round();
        Size::new(h * 1.85, h)
    }
}
impl Widget for Switch {
    type Command = ToggleCommand;
    type Output = bool;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: ToggleCommand) {
        self.0.command(cx, command);
    }
    fn focusable(&self) -> bool {
        !self.0.disabled
    }
    fn cursor(&self, _: Point) -> Option<CursorIcon> {
        Some(if self.0.disabled {
            CursorIcon::Arrow
        } else {
            CursorIcon::Pointer
        })
    }
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        if matches!(event, Lifecycle::Hover(_) | Lifecycle::Focus(_)) {
            cx.repaint()
        }
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if matches!(
            input,
            Input::Button {
                button: 1,
                down: true,
                ..
            }
        ) && phase != Phase::Preview
            && !self.0.disabled
        {
            let _ = cx.focus();
            cx.stop();
            return;
        }
        if self.0.activates(cx, phase, input) {
            self.0.checked = !self.0.checked;
            let _ = cx.emit(self.0.checked);
            cx.repaint();
            cx.stop()
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let track = Self::track(&theme(cx));
        self.0.layout(cx, c, track)
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = crate::painted_theme(cx);
        let track = Self::track(&t);
        let r = Toggle::box_rect(cx.bounds, track, t.scale.halo);
        let radius = track.height / 2.;
        if self.0.checked && !self.0.disabled {
            crate::glow(cx.painter, r, radius, 9., t.color.accent.alpha(0.35));
            cx.painter.rect(r, radius, crate::accent_gradient(r, &t));
        } else {
            cx.painter.rect(
                r,
                radius,
                if cx.hovered && !self.0.disabled {
                    t.color.raised
                } else {
                    t.color.sunken
                }
                .into(),
            );
            cx.painter
                .stroke(r.inset(0.5), radius, t.scale.border, t.color.border_strong);
        }
        let pad = 2.5;
        let knob = track.height - pad * 2.;
        let x = if self.0.checked {
            r.x + track.width - knob - pad
        } else {
            r.x + pad
        };
        cx.painter.rect(
            Rect::new(x, r.y + pad, knob, knob),
            knob / 2.,
            if self.0.checked {
                t.color.on_accent
            } else if self.0.disabled {
                t.color.faint
            } else {
                t.color.muted
            }
            .into(),
        );
        if cx.focused && !self.0.disabled {
            crate::focus_ring(cx.painter, r.inset(-2.), radius + 2., &t);
        }
    }
    fn semantics(&self) -> Semantics {
        self.0.semantics(Role::Switch)
    }
    fn accessibility(
        &mut self,
        cx: &mut Update<'_, Self>,
        action: SemanticAction,
    ) -> Result<(), SemanticError> {
        toggle_action(&mut self.0, cx, action, |cx, checked| cx.emit(checked))
    }
}

fn toggle_action<W: Widget>(
    toggle: &mut Toggle,
    cx: &mut Update<'_, W>,
    action: SemanticAction,
    emit: impl FnOnce(&mut Update<'_, W>, bool) -> Result<(), (Error, W::Output)>,
) -> Result<(), SemanticError> {
    if toggle.disabled {
        return Err(SemanticError::Unavailable);
    }
    match action {
        SemanticAction::Focus => cx.focus().map_err(|_| SemanticError::Unavailable),
        SemanticAction::Activate => {
            toggle.checked = !toggle.checked;
            cx.repaint();
            emit(cx, toggle.checked).map_err(|_| SemanticError::Unavailable)
        }
        _ => Err(SemanticError::Unsupported),
    }
}
