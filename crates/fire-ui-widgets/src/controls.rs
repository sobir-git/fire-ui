use crate::{theme, Appearance, Theme};
use fire_ui::*;
use std::{convert::Infallible, sync::Arc};
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
        let t = self.appearance.resolve(&theme(cx));
        let style = TextStyle {
            size: t.font_size,
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
        let parent = cx.environment::<Theme>().cloned().unwrap_or_default();
        let t = self.appearance.resolve(&parent);
        if let Some(p) = &self.paragraph {
            cx.painter
                .paragraph(p, Point::default(), t.foreground.into())
        }
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::Text,
            label: self.text.to_string(),
            ..Semantics::default()
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
}
impl<C: Data> Data for ButtonCommand<C> {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Content(command) => command.bytes(),
                Self::Label(label) => label.capacity(),
                Self::Disabled(_) => 0,
            }
    }
}
/// Content is an ordinary owned widget. Decorative content does not consume activation.
pub struct Button<C: Widget<Output = Infallible>> {
    content: Child<C>,
    pressed: Option<u32>,
    disabled: bool,
    theme: Theme,
    label: String,
    appearance: Option<Theme>,
}
impl<C: Widget<Output = Infallible>> Button<C> {
    pub fn new(content: Element<C>, label: impl Into<String>) -> Element<Self> {
        Self::build(content, label.into(), None)
    }
    /// Per-control styling without replacing keyboard, pointer or accessibility behavior.
    pub fn styled(
        content: Element<C>,
        label: impl Into<String>,
        appearance: Theme,
    ) -> Element<Self> {
        Self::build(content, label.into(), Some(appearance))
    }
    fn build(content: Element<C>, label: String, appearance: Option<Theme>) -> Element<Self> {
        Element::build(|children| Self {
            content: children.add(content),
            pressed: None,
            disabled: false,
            theme: Theme::default(),
            label,
            appearance,
        })
    }
}
impl<C: Widget<Output = Infallible>> Widget for Button<C> {
    type Command = ButtonCommand<C::Command>;
    type Output = ();
    fn update(&mut self, cx: &mut Update<'_, Self>, command: Self::Command) {
        match command {
            ButtonCommand::Content(command) => {
                let _ = cx.send(self.content, command);
            }
            ButtonCommand::Label(label) => {
                self.label = label;
                cx.repaint();
            }
            ButtonCommand::Disabled(value) => {
                self.disabled = value;
                if let Some(pointer) = self.pressed.take() {
                    let _ = cx.release(pointer);
                }
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
        self.theme = self.appearance.clone().unwrap_or_else(|| theme(cx));
        let inset = self.theme.inset;
        let m = cx.measure(self.content, c.inset(inset));
        let size = c.constrain(Size::new(
            m.size.width + 2. * inset,
            m.size.height + 2. * inset,
        ));
        cx.place(
            self.content,
            Point::new(inset, (size.height - m.size.height) / 2.),
        );
        Metrics {
            size,
            baseline: m.baseline.map(|b| b + (size.height - m.size.height) / 2.),
        }
    }
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        if matches!(
            event,
            Lifecycle::CaptureLost(_) | Lifecycle::Focus(false) | Lifecycle::Visibility(false)
        ) {
            self.pressed = None;
            cx.repaint()
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
        let t = self
            .appearance
            .clone()
            .or_else(|| cx.environment::<Theme>().cloned())
            .unwrap_or_else(|| self.theme.clone());
        cx.painter.rect(
            cx.bounds,
            t.radius,
            if self.pressed.is_some() {
                t.accent.alpha(0.25)
            } else if cx.hovered {
                t.raised
            } else {
                t.panel
            }
            .into(),
        );
        cx.painter.stroke(
            cx.bounds.inset(0.5),
            t.radius,
            1.,
            if cx.focused { t.accent } else { t.border },
        );
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
