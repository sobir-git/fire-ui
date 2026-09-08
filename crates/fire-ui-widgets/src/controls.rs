use crate::{theme, Appearance, TextRole, Theme};
use fire_ui::*;
use std::{convert::Infallible, rc::Rc, sync::Arc};

pub struct Label {
    text: Arc<str>,
    appearance: Appearance,
    paragraph: Option<Arc<Paragraph>>,
    revision: u64,
    wrap: bool,
}
impl Label {
    pub fn new(text: impl Into<Arc<str>>) -> Self {
        Self {
            text: text.into(),
            appearance: Appearance::default(),
            paragraph: None,
            revision: 0,
            wrap: false,
        }
    }
    /// A label at a step of the type scale.
    pub fn styled(text: impl Into<Arc<str>>, role: TextRole) -> Self {
        Self::new(text).appearance(Appearance::role(role))
    }
    /// A label at a step of the type scale, in a palette role.
    pub fn toned(text: impl Into<Arc<str>>, role: TextRole, color: crate::ColorRole) -> Self {
        Self::new(text).appearance(Appearance::role(role).color(color))
    }
    pub fn appearance(mut self, appearance: Appearance) -> Self {
        self.appearance = appearance;
        self
    }
    pub fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }
    pub fn paragraph(&self) -> Option<&Arc<Paragraph>> {
        self.paragraph.as_ref()
    }
}
impl Widget for Label {
    type Command = String;
    type Output = Infallible;
    fn update(&mut self, cx: &mut Update<'_, Self>, text: String) {
        if *self.text != text {
            self.text = text.into();
            self.revision += 1;
            cx.relayout()
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let style = TextStyle {
            size: self.appearance.resolve_size(&theme(cx)),
            font: 0,
        };
        let width = self.wrap.then_some(c.max.width).filter(|w| w.is_finite());
        if self.paragraph.as_ref().is_none_or(|p| {
            p.revision != self.revision
                || p.style != style
                || p.width != width
                || p.service_revision != cx.text_revision()
        }) {
            self.paragraph = Some(cx.paragraph(TextRequest {
                text: self.text.clone(),
                style,
                width,
                revision: self.revision,
            }));
        }
        let p = self.paragraph.as_ref().unwrap();
        Metrics {
            size: c.constrain(p.size),
            baseline: Some(p.baseline.min(c.max.height)),
        }
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = crate::painted_theme(cx);
        if let Some(p) = &self.paragraph {
            cx.painter.paragraph(
                p,
                Point::default(),
                self.appearance.resolve_color(&t).into(),
            )
        }
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: if matches!(
                self.appearance.role,
                Some(TextRole::Display | TextRole::Title | TextRole::Heading)
            ) {
                Role::Heading
            } else {
                Role::Text
            },
            label: self.text.to_string(),
            ..Semantics::default()
        }
    }
}

/// How much visual weight a button carries. One accent, four levels of emphasis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonStyle {
    /// The one action a panel wants you to take. Filled with the accent.
    Primary,
    /// The ordinary action. A bordered surface.
    #[default]
    Secondary,
    /// Tertiary actions and toolbars. No fill until hovered.
    Ghost,
    /// Destructive actions.
    Danger,
}
impl ButtonStyle {
    fn foreground(self, t: &Theme) -> Color {
        match self {
            Self::Primary | Self::Danger => t.color.on_accent,
            Self::Secondary => t.color.foreground,
            Self::Ghost => t.color.muted,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ButtonCommand<C: Data> {
    Disabled(bool),
    /// Update the owned content through its ordinary command protocol.
    Content(C),
    /// Update the button's accessible action name.
    Label(String),
    Style(ButtonStyle),
}
impl<C: Data> Data for ButtonCommand<C> {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Content(command) => command.bytes(),
                Self::Label(label) => label.capacity(),
                Self::Disabled(_) | Self::Style(_) => 0,
            }
    }
}

/// Content is an ordinary owned widget. Decorative content does not consume activation.
///
/// The button publishes a derived theme to its content so a filled button's text
/// resolves to `on_accent` without the content knowing which button holds it.
pub struct Button<C: Widget<Output = Infallible>> {
    content: Child<C>,
    pressed: Option<u32>,
    disabled: bool,
    style: ButtonStyle,
    label: String,
    published: Option<Color>,
}
impl<C: Widget<Output = Infallible>> Button<C> {
    pub fn new(content: Element<C>, label: impl Into<String>) -> Element<Self> {
        Self::styled(content, label, ButtonStyle::default())
    }
    pub fn styled(
        content: Element<C>,
        label: impl Into<String>,
        style: ButtonStyle,
    ) -> Element<Self> {
        let label = label.into();
        Element::build(|children| Self {
            content: children.add(content),
            pressed: None,
            disabled: false,
            style,
            label,
            published: None,
        })
    }
    fn publish(&mut self, cx: &mut Update<'_, Self>) {
        let t = cx.environment::<Theme>().copied().unwrap_or_default();
        let foreground = if self.disabled {
            t.color.faint
        } else {
            self.style.foreground(&t)
        };
        if self.published != Some(foreground) {
            let mut derived = t;
            derived.color.foreground = foreground;
            if cx
                .set_environment(self.content, Rc::new(derived), false)
                .is_ok()
            {
                self.published = Some(foreground);
            }
        }
    }
}
impl<C: Widget<Output = Infallible>> Widget for Button<C> {
    type Command = ButtonCommand<C::Command>;
    type Output = ();
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        match event {
            Lifecycle::Mount | Lifecycle::Inherited => self.publish(cx),
            Lifecycle::CaptureLost(_) | Lifecycle::Focus(false) | Lifecycle::Visibility(false) => {
                self.pressed = None;
                cx.repaint()
            }
            Lifecycle::Hover(_) => cx.repaint(),
            _ => {}
        }
    }
    fn update(&mut self, cx: &mut Update<'_, Self>, command: Self::Command) {
        match command {
            ButtonCommand::Content(command) => {
                let _ = cx.send(self.content, command);
            }
            ButtonCommand::Label(label) => {
                self.label = label;
                cx.repaint()
            }
            ButtonCommand::Style(style) => {
                self.style = style;
                self.publish(cx);
                cx.repaint()
            }
            ButtonCommand::Disabled(value) => {
                self.disabled = value;
                if let Some(pointer) = self.pressed.take() {
                    let _ = cx.release(pointer);
                }
                self.publish(cx);
                cx.repaint()
            }
        }
    }
    fn focusable(&self) -> bool {
        !self.disabled
    }
    fn cursor(&self, _position: Point) -> Option<CursorIcon> {
        Some(if self.disabled {
            CursorIcon::Arrow
        } else {
            CursorIcon::Pointer
        })
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let t = theme(cx);
        let inset = t.scale.inset + t.scale.halo;
        let m = cx.measure(self.content, c.inset(inset));
        let size = c.constrain(Size::new(
            m.size.width + 2. * inset,
            (m.size.height + 2. * inset).max(t.scale.control + 2. * t.scale.halo),
        ));
        cx.place(
            self.content,
            Point::new(
                (size.width - m.size.width) / 2.,
                (size.height - m.size.height) / 2.,
            ),
        );
        Metrics {
            size,
            baseline: m.baseline.map(|b| b + (size.height - m.size.height) / 2.),
        }
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if phase == Phase::Preview || self.disabled {
            return;
        }
        match input {
            Input::Button {
                pointer,
                button: 1,
                down: true,
                ..
            } => {
                self.pressed = Some(*pointer);
                let _ = cx.focus();
                let _ = cx.capture(*pointer);
                cx.repaint();
                cx.stop()
            }
            Input::Button {
                pointer,
                button: 1,
                down: false,
                position,
            } if self.pressed == Some(*pointer) => {
                self.pressed = None;
                let _ = cx.release(*pointer);
                if cx.bounds().contains(*position) {
                    let _ = cx.emit(());
                }
                cx.repaint();
                cx.stop()
            }
            Input::Key {
                key: Key::Enter | Key::Character(' '),
                down: true,
                repeat: false,
                ..
            } => {
                let _ = cx.emit(());
                cx.stop()
            }
            _ => {}
        }
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = crate::painted_theme(cx);
        let r = t.scale.radius;
        // The visible button sits inside the room reserved for its halo.
        let bounds = cx.bounds.inset(t.scale.halo);
        let pressed = self.pressed.is_some();
        let hovered = cx.hovered && !self.disabled;
        if self.disabled {
            cx.painter
                .rect(bounds, r, t.color.surface.alpha(0.6).into());
            cx.painter
                .stroke(bounds.inset(0.5), r, t.scale.border, t.color.border);
        } else {
            match self.style {
                ButtonStyle::Primary | ButtonStyle::Danger => {
                    let base = if self.style == ButtonStyle::Danger {
                        t.color.danger
                    } else if hovered {
                        t.color.accent_hover
                    } else {
                        t.color.accent
                    };
                    let top = if self.style == ButtonStyle::Danger {
                        base
                    } else {
                        t.color.accent_light
                    };
                    if hovered && !pressed {
                        crate::glow(cx.painter, bounds, r, 14., base.alpha(0.45));
                    }
                    cx.painter.rect(
                        bounds,
                        r,
                        Brush::Linear {
                            start: Point::new(bounds.x, bounds.y),
                            end: Point::new(bounds.x, bounds.y + bounds.height),
                            from: if pressed { base } else { top },
                            to: base,
                        },
                    );
                }
                ButtonStyle::Secondary => {
                    cx.painter.rect(
                        bounds,
                        r,
                        if pressed {
                            t.color.sunken
                        } else if hovered {
                            t.color.raised
                        } else {
                            t.color.surface
                        }
                        .into(),
                    );
                    cx.painter.stroke(
                        bounds.inset(0.5),
                        r,
                        t.scale.border,
                        if hovered {
                            t.color.border_strong
                        } else {
                            t.color.border
                        },
                    );
                }
                ButtonStyle::Ghost => {
                    if hovered || pressed {
                        cx.painter.rect(
                            bounds,
                            r,
                            if pressed {
                                t.color.sunken
                            } else {
                                t.color.raised
                            }
                            .into(),
                        );
                    }
                }
            }
        }
        if cx.focused && !self.disabled {
            crate::focus_ring(cx.painter, bounds, r, &t);
        }
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::Button,
            actions: vec![SemanticActionKind::Activate],
            label: self.label.clone(),
            disabled: self.disabled,
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
            SemanticAction::Activate => cx.emit(()).map_err(|_| SemanticError::Unavailable),
            SemanticAction::Focus => cx.focus().map_err(|_| SemanticError::Unavailable),
            _ => Err(SemanticError::Unsupported),
        }
    }
}

/// A one-pixel rule. Horizontal when wider than tall.
#[derive(Default)]
pub struct Divider;
impl Widget for Divider {
    type Command = Infallible;
    type Output = Infallible;
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let border = theme(cx).scale.border;
        Metrics::new(c.constrain(Size::new(
            if c.max.width.is_finite() {
                c.max.width
            } else {
                border
            },
            border,
        )))
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        cx.painter
            .rect(cx.bounds, 0., crate::painted_theme(cx).color.border.into())
    }
}
