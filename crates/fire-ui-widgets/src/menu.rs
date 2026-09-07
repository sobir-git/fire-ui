use crate::{theme, Theme};
use fire_ui::*;
use std::sync::Arc;

/// An action's presentation. Keys belong to the consumer; the menu knows no app actions.
#[derive(Clone)]
pub struct MenuItem<K> {
    pub key: K,
    pub label: Arc<str>,
    pub hint: Arc<str>,
    pub enabled: bool,
    pub checked: bool,
}
impl<K> MenuItem<K> {
    pub fn new(key: K, label: impl Into<Arc<str>>) -> Self {
        Self {
            key,
            label: label.into(),
            hint: Arc::from(""),
            enabled: true,
            checked: false,
        }
    }
    pub fn hint(mut self, hint: impl Into<Arc<str>>) -> Self {
        self.hint = hint.into();
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}
#[derive(Clone, Debug)]
pub enum MenuOutput<K> {
    Selected(K),
    Dismissed,
}
impl<K: Data> Data for MenuOutput<K> {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Selected(k) => k.bytes(),
                Self::Dismissed => 0,
            }
    }
}
/// Commands from the menu's owned action rows.
pub struct MenuCommand(usize);
impl Data for MenuCommand {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}
/// A context menu placed in a full-window overlay. Its owner opens/closes the modal scope.
/// Rows expose ordinary button semantics and support keyboard, pointer and accessibility input.
pub struct Menu<K: Data + Clone> {
    items: Vec<MenuItem<K>>,
    rows: Vec<Child<MenuRow>>,
    at: Point,
    panel: Rect,
    selected: Option<usize>,
}
impl<K: Data + Clone> Menu<K> {
    pub fn new(items: Vec<MenuItem<K>>, at: Point) -> Element<Self> {
        Element::build(|c| {
            let rows = items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    c.connect(
                        Element::leaf(MenuRow {
                            label: item.label.clone(),
                            hint: item.hint.clone(),
                            enabled: item.enabled,
                            checked: item.checked,
                            text: None,
                            shortcut: None,
                            pressed: None,
                        }),
                        move |_| MenuCommand(index),
                    )
                })
                .collect();
            Self {
                selected: None,
                items,
                rows,
                at,
                panel: Rect::default(),
            }
        })
    }
    fn select(&mut self, cx: &mut Update<'_, Self>, index: usize) {
        if self.items[index].enabled {
            self.selected = Some(index);
            let _ = cx.focus_child(self.rows[index]);
        }
    }
    fn step(&mut self, cx: &mut Update<'_, Self>, forward: bool) {
        let len = self.items.len();
        for n in 1..=len {
            let index = if forward {
                (self.selected.map_or(len - 1, |i| i) + n) % len
            } else {
                (self.selected.unwrap_or(0) + len - n) % len
            };
            if self.items[index].enabled {
                self.select(cx, index);
                break;
            }
        }
    }
}
impl<K: Data + Clone> Widget for Menu<K> {
    type Command = MenuCommand;
    type Output = MenuOutput<K>;
    fn focusable(&self) -> bool {
        true
    }
    fn update(&mut self, cx: &mut Update<'_, Self>, command: MenuCommand) {
        if let Some(item) = self.items.get(command.0).filter(|i| i.enabled) {
            let _ = cx.emit(MenuOutput::Selected(item.key.clone()));
        }
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if !matches!(phase, Phase::Preview | Phase::Target) {
            return;
        }
        match input {
            Input::Pointer { position, .. } if self.panel.contains(*position) => {
                let row = ((position.y - self.panel.y - 6.) / 32.).floor();
                if row >= 0.
                    && (row as usize) < self.items.len()
                    && self.selected != Some(row as usize)
                {
                    self.select(cx, row as usize);
                }
            }
            Input::Button {
                down: true,
                position,
                ..
            } if !self.panel.contains(*position) => {
                let _ = cx.emit(MenuOutput::Dismissed);
                cx.stop();
            }
            Input::Key {
                key: Key::Escape,
                down: true,
                ..
            } => {
                let _ = cx.emit(MenuOutput::Dismissed);
                cx.stop();
            }
            Input::Key {
                key: Key::Down | Key::Up | Key::Tab,
                down: true,
                modifiers,
                ..
            } => {
                let forward = matches!(input, Input::Key { key: Key::Down, .. })
                    || matches!(input, Input::Key { key: Key::Tab, .. }) && !modifiers.shift;
                self.step(cx, forward);
                cx.stop();
            }
            Input::Key {
                key: Key::Home | Key::End,
                down: true,
                ..
            } => {
                let index = if matches!(input, Input::Key { key: Key::Home, .. }) {
                    self.items.iter().position(|i| i.enabled)
                } else {
                    self.items.iter().rposition(|i| i.enabled)
                };
                if let Some(i) = index {
                    self.select(cx, i);
                }
                cx.stop();
            }
            _ => {}
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let mut width: f32 = 220.;
        for row in &self.rows {
            width = width.max(
                cx.measure(*row, Constraints::loose(Size::new(c.max.width, 32.)))
                    .size
                    .width
                    + 12.,
            );
        }
        width = width.min((c.max.width - 8.).max(0.));
        let height = (self.rows.len() as f32 * 32. + 12.).min((c.max.height - 8.).max(0.));
        self.panel = Rect::new(
            self.at.x.clamp(4., (c.max.width - width - 4.).max(4.)),
            self.at.y.clamp(4., (c.max.height - height - 4.).max(4.)),
            width,
            height,
        );
        for (i, row) in self.rows.iter().enumerate() {
            cx.measure(
                *row,
                Constraints::tight(Size::new((width - 12.).max(0.), 32.)),
            );
            cx.place(
                *row,
                Point::new(self.panel.x + 6., self.panel.y + 6. + i as f32 * 32.),
            );
        }
        Metrics::new(c.max)
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = cx.environment::<Theme>().cloned().unwrap_or_default();
        cx.painter.rect(self.panel, t.radius, t.panel.into());
        cx.painter
            .stroke(self.panel.inset(0.5), t.radius, 1., t.border);
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::Dialog,
            label: "Actions".into(),
            ..Semantics::default()
        }
    }
}
struct MenuRow {
    label: Arc<str>,
    hint: Arc<str>,
    enabled: bool,
    checked: bool,
    text: Option<Arc<Paragraph>>,
    shortcut: Option<Arc<Paragraph>>,
    pressed: Option<u32>,
}
impl Widget for MenuRow {
    type Command = ();
    type Output = ();
    fn focusable(&self) -> bool {
        self.enabled
    }
    fn cursor(&self, _: Point) -> Option<CursorIcon> {
        Some(if self.enabled {
            CursorIcon::Pointer
        } else {
            CursorIcon::Arrow
        })
    }
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, e: Lifecycle) {
        if matches!(
            e,
            Lifecycle::CaptureLost(_) | Lifecycle::Focus(false) | Lifecycle::Visibility(false)
        ) {
            self.pressed = None;
            cx.repaint();
        }
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if phase != Phase::Target || !self.enabled {
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
                let _ = cx.capture(*pointer);
                let _ = cx.focus();
                cx.stop();
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
                cx.stop();
            }
            Input::Key {
                key: Key::Enter | Key::Character(' '),
                down: true,
                repeat: false,
                ..
            } => {
                let _ = cx.emit(());
                cx.stop();
            }
            _ => {}
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let style = TextStyle {
            size: theme(cx).font_size,
            font: 0,
        };
        for (source, target) in [
            (&self.label, &mut self.text),
            (&self.hint, &mut self.shortcut),
        ] {
            if target
                .as_ref()
                .is_none_or(|p| p.style != style || p.service_revision != cx.text_revision())
            {
                *target = Some(cx.paragraph(TextRequest {
                    text: source.clone(),
                    style,
                    width: None,
                    revision: 0,
                }));
            }
        }
        Metrics::new(c.constrain(Size::new(
            self.text.as_ref().unwrap().size.width
                + self.shortcut.as_ref().unwrap().size.width
                + 60.,
            32.,
        )))
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = cx.environment::<Theme>().cloned().unwrap_or_default();
        if self.enabled && (cx.focused || cx.hovered) {
            cx.painter.rect(cx.bounds, 3., t.raised.into());
        }
        let fg = if self.enabled { t.foreground } else { t.muted };
        if self.checked {
            cx.painter.path(
                &[
                    Path::Move(Point::new(7., 16.)),
                    Path::Line(Point::new(10., 19.)),
                    Path::Line(Point::new(16., 12.)),
                ],
                fg.into(),
                Some(1.5),
            );
        }
        if let Some(p) = &self.text {
            cx.painter
                .paragraph(p, Point::new(24., (32. - p.line_height) / 2.), fg.into());
        }
        if let Some(p) = &self.shortcut {
            cx.painter.paragraph(
                p,
                Point::new(
                    cx.bounds.width - p.size.width - 8.,
                    (32. - p.line_height) / 2.,
                ),
                t.muted.into(),
            );
        }
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::Button,
            label: self.label.to_string(),
            disabled: !self.enabled,
            selected: self.checked,
            ..Semantics::default()
        }
    }
    fn accessibility(&mut self, cx: &mut Update<'_, Self>, action: SemanticAction) {
        if self.enabled {
            match action {
                SemanticAction::Activate => {
                    let _ = cx.emit(());
                }
                SemanticAction::Focus => {
                    let _ = cx.focus();
                }
            }
        }
    }
}
