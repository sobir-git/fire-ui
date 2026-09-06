use crate::Theme;
use fire_ui::*;

#[derive(Debug)]
pub struct Clicked;
#[derive(Debug)]
pub struct SelectionChanged(pub usize);
#[derive(Debug)]
pub struct Activated(pub usize);
#[derive(Debug)]
pub struct Dismissed;

pub struct Label {
    pub text: String,
    pub color: Color,
    pub style: TextStyle,
}
impl Label {
    pub fn new(text: impl Into<String>, color: Color, size: f32) -> Self {
        Self {
            text: text.into(),
            color,
            style: TextStyle { size, font: 0 },
        }
    }
}
impl Widget for Label {
    fn layout(&mut self, _: &mut [Node], c: Constraints, text: &mut dyn TextEngine) -> Size {
        let width = self
            .text
            .split('\n')
            .map(|line| text.measure(line, self.style).width)
            .fold(0., f32::max);
        c.constrain(Size::new(
            width,
            self.style.size * 1.5 * self.text.split('\n').count() as f32,
        ))
    }
    fn paint(&self, ctx: &mut PaintCtx<'_>) {
        for (i, line) in self.text.split('\n').enumerate() {
            ctx.painter.text(
                line,
                Point::new(0., i as f32 * self.style.size * 1.5),
                self.style,
                self.color.into(),
            );
        }
    }
}

pub struct Button {
    pub label: String,
    pub theme: Theme,
    pub disabled: bool,
    pressed: bool,
}
impl Button {
    pub fn new(label: impl Into<String>, theme: Theme) -> Self {
        Self {
            label: label.into(),
            theme,
            disabled: false,
            pressed: false,
        }
    }
}
impl Widget for Button {
    fn layout(&mut self, _: &mut [Node], c: Constraints, text: &mut dyn TextEngine) -> Size {
        c.constrain(Size::new(
            text.measure(
                &self.label,
                TextStyle {
                    size: self.theme.font_size,
                    font: 0,
                },
            )
            .width
                + 28.,
            40.,
        ))
    }
    fn focusable(&self) -> bool {
        !self.disabled
    }
    fn event(&mut self, ctx: &mut EventCtx<'_>, event: &Event) {
        if self.disabled {
            return;
        }
        match event {
            Event::PointerDown(_) => {
                self.pressed = true;
                ctx.focus();
                ctx.capture_pointer();
                ctx.repaint();
                ctx.handle();
            }
            Event::PointerUp(p) => {
                if self.pressed && ctx.bounds.contains(*p) {
                    ctx.emit(Clicked);
                }
                self.pressed = false;
                ctx.release_pointer();
                ctx.repaint();
                ctx.handle();
            }
            Event::Cancel | Event::Focus(false) => {
                self.pressed = false;
                ctx.repaint();
            }
            Event::Key {
                key: Key::Enter | Key::Character(' '),
                ..
            } => {
                ctx.emit(Clicked);
                ctx.handle();
            }
            _ => {}
        }
    }
    fn paint(&self, ctx: &mut PaintCtx<'_>) {
        let t = self.theme;
        ctx.painter.rect(
            ctx.bounds,
            t.radius,
            if self.pressed {
                t.accent.alpha(0.30)
            } else if ctx.hovered {
                t.raised
            } else {
                t.panel
            }
            .into(),
        );
        ctx.painter.stroke(
            ctx.bounds.inset(0.75),
            t.radius,
            1.,
            if ctx.focused { t.accent } else { t.border },
        );
        ctx.painter.text(
            &self.label,
            Point::new(14., (ctx.bounds.height - t.font_size * 1.3) * 0.5),
            TextStyle {
                size: t.font_size,
                font: 0,
            },
            if self.disabled { t.muted } else { t.foreground }.into(),
        );
    }
}

pub struct List {
    pub items: Vec<String>,
    pub selected: usize,
    pub theme: Theme,
    pub row_height: f32,
    pub scroll: f32,
    viewport: f32,
    last_click: Option<(usize, std::time::Duration)>,
}
impl List {
    pub fn new(items: impl IntoIterator<Item = String>, theme: Theme) -> Self {
        Self {
            items: items.into_iter().collect(),
            selected: 0,
            theme,
            row_height: 38.,
            scroll: 0.,
            viewport: 0.,
            last_click: None,
        }
    }
    fn max_scroll(&self) -> f32 {
        (self.items.len() as f32 * self.row_height - self.viewport).max(0.)
    }
    fn reveal(&mut self) {
        let y = self.selected as f32 * self.row_height;
        if y < self.scroll {
            self.scroll = y;
        }
        if y + self.row_height > self.scroll + self.viewport {
            self.scroll = y + self.row_height - self.viewport;
        }
        self.scroll = self.scroll.clamp(0., self.max_scroll());
    }
}
impl Widget for List {
    fn focusable(&self) -> bool {
        true
    }
    fn layout(&mut self, _: &mut [Node], c: Constraints, _: &mut dyn TextEngine) -> Size {
        self.viewport = c.max.height;
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
        self.scroll = self.scroll.clamp(0., self.max_scroll());
        c.max
    }
    fn event(&mut self, ctx: &mut EventCtx<'_>, event: &Event) {
        match event {
            Event::PointerDown(p) => {
                ctx.focus();
                let index = ((p.y + self.scroll) / self.row_height).floor().max(0.) as usize;
                if index < self.items.len() {
                    self.selected = index;
                    ctx.emit(SelectionChanged(index));
                    if self.last_click.is_some_and(|(i, t)| {
                        i == index
                            && ctx.now.saturating_sub(t) < std::time::Duration::from_millis(350)
                    }) {
                        ctx.emit(Activated(index));
                        self.last_click = None;
                    } else {
                        self.last_click = Some((index, ctx.now));
                    }
                }
                ctx.repaint();
                ctx.handle();
            }
            Event::Key {
                key: Key::Up | Key::Down | Key::Home | Key::End,
                ..
            } => {
                if let Event::Key { key, .. } = event {
                    match key {
                        Key::Up => self.selected = self.selected.saturating_sub(1),
                        Key::Down => {
                            self.selected =
                                (self.selected + 1).min(self.items.len().saturating_sub(1))
                        }
                        Key::Home => self.selected = 0,
                        Key::End => self.selected = self.items.len().saturating_sub(1),
                        _ => {}
                    }
                }
                self.reveal();
                if !self.items.is_empty() {
                    ctx.emit(SelectionChanged(self.selected));
                }
                ctx.repaint();
                ctx.handle();
            }
            Event::Key {
                key: Key::Enter, ..
            } => {
                if !self.items.is_empty() {
                    ctx.emit(Activated(self.selected));
                }
                ctx.handle();
            }
            Event::Scroll { delta, .. } => {
                let next = (self.scroll - delta.y).clamp(0., self.max_scroll());
                if next != self.scroll {
                    self.scroll = next;
                    ctx.repaint();
                    ctx.handle();
                }
            }
            _ => {}
        }
    }
    fn paint(&self, ctx: &mut PaintCtx<'_>) {
        let t = self.theme;
        ctx.painter.rect(ctx.bounds, t.radius, t.panel.into());
        let first = (self.scroll / self.row_height).floor() as usize;
        let count = (ctx.bounds.height / self.row_height).ceil() as usize + 1;
        for (i, item) in self.items.iter().enumerate().skip(first).take(count) {
            let y = i as f32 * self.row_height - self.scroll;
            if i == self.selected {
                ctx.painter.rect(
                    Rect::new(0., y, ctx.bounds.width, self.row_height),
                    0.,
                    t.selection.into(),
                );
            }
            ctx.painter.text(
                item,
                Point::new(12., y + 9.),
                TextStyle {
                    size: t.font_size,
                    font: 0,
                },
                if i == self.selected {
                    t.foreground
                } else {
                    t.muted
                }
                .into(),
            );
        }
        if self.max_scroll() > 0. {
            let h = (ctx.bounds.height * ctx.bounds.height
                / (self.items.len() as f32 * self.row_height))
                .max(20.)
                .min(ctx.bounds.height);
            let y = (ctx.bounds.height - h) * self.scroll / self.max_scroll();
            ctx.painter.rect(
                Rect::new(ctx.bounds.width - 5., y, 3., h),
                1.5,
                t.muted.alpha(0.4).into(),
            );
        }
        if ctx.focused {
            ctx.painter
                .stroke(ctx.bounds.inset(0.5), t.radius, 1., t.accent);
        }
    }
}

pub struct Modal {
    pub size: Size,
    pub theme: Theme,
    panel: Rect,
}
impl Modal {
    pub fn new(size: Size, theme: Theme) -> Self {
        Self {
            size,
            theme,
            panel: Rect::default(),
        }
    }
}
impl Widget for Modal {
    fn layout(&mut self, children: &mut [Node], c: Constraints, text: &mut dyn TextEngine) -> Size {
        let size = Size::new(
            self.size.width.min((c.max.width - 32.).max(0.)),
            self.size.height.min((c.max.height - 32.).max(0.)),
        );
        self.panel = Rect::new(
            (c.max.width - size.width) * 0.5,
            (c.max.height - size.height) * 0.5,
            size.width,
            size.height,
        );
        for child in children {
            child.layout(Constraints::tight(size), text);
            child.place(self.panel.x, self.panel.y);
        }
        c.max
    }
    fn event(&mut self, ctx: &mut EventCtx<'_>, event: &Event) {
        match event {
            Event::Key {
                key: Key::Escape, ..
            } => {
                ctx.emit(Dismissed);
                ctx.handle();
            }
            Event::PointerDown(p) if !self.panel.contains(*p) => {
                ctx.emit(Dismissed);
                ctx.handle();
            }
            _ => {}
        }
    }
    fn paint(&self, ctx: &mut PaintCtx<'_>) {
        ctx.painter
            .rect(ctx.bounds, 0., Color::hex(0).alpha(0.65).into());
        ctx.painter
            .rect(self.panel, self.theme.radius, self.theme.panel.into());
        ctx.painter
            .stroke(self.panel, self.theme.radius, 1., self.theme.border);
    }
}
