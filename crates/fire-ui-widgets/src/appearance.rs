use fire_ui::*;
use std::rc::Rc;
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub background: Color,
    pub panel: Color,
    pub raised: Color,
    pub border: Color,
    pub foreground: Color,
    pub muted: Color,
    pub accent: Color,
    pub selection: Color,
    pub font_size: f32,
    pub inset: f32,
    pub radius: f32,
}
impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color::hex(0x171918),
            panel: Color::hex(0x202422),
            raised: Color::hex(0x303733),
            border: Color::hex(0x485149),
            foreground: Color::hex(0xf0eee4),
            muted: Color::hex(0xa9b2a8),
            accent: Color::hex(0xf4b46e),
            selection: Color::hex(0xf4b46e).alpha(0.24),
            font_size: 15.,
            inset: 12.,
            radius: 7.,
        }
    }
}
impl Theme {
    pub fn metrics_changed(&self, previous: &Self) -> bool {
        self.font_size != previous.font_size || self.inset != previous.inset
    }
}
#[derive(Clone, Default)]
pub struct Appearance {
    pub foreground: Option<Color>,
    pub font_size: Option<f32>,
}
impl Appearance {
    pub fn resolve(&self, parent: &Theme) -> Theme {
        let mut t = parent.clone();
        if let Some(c) = self.foreground {
            t.foreground = c
        }
        if let Some(s) = self.font_size {
            t.font_size = s
        }
        t
    }
}
pub fn theme(cx: &Layout<'_>) -> Theme {
    cx.environment::<Theme>().cloned().unwrap_or_default()
}
pub struct AppearanceScope<C: Widget> {
    child: Child<C>,
    theme: Rc<Theme>,
}
impl<C: Widget<Output: Clone>> AppearanceScope<C> {
    pub fn new(content: Element<C>, theme: Rc<Theme>) -> Element<Self> {
        Element::build(|children| Self {
            child: children.connect(content, |o| ScopeCommand::Child(o.clone())),
            theme,
        })
    }
}
pub enum ScopeCommand<C: Data> {
    Child(C),
    Theme(Rc<Theme>),
}
impl<C: Data> Data for ScopeCommand<C> {
    fn bytes(&self) -> usize {
        match self {
            Self::Child(c) => c.bytes(),
            Self::Theme(_) => std::mem::size_of::<Theme>(),
        }
    }
}
impl<C: Widget> Widget for AppearanceScope<C> {
    type Command = ScopeCommand<C::Output>;
    type Output = C::Output;
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, e: Lifecycle) {
        if e == Lifecycle::Mount {
            let _ = cx.set_environment(self.child, self.theme.clone(), true);
        }
    }
    fn update(&mut self, cx: &mut Update<'_, Self>, command: Self::Command) {
        match command {
            ScopeCommand::Child(o) => {
                let _ = cx.emit(o);
            }
            ScopeCommand::Theme(theme) => {
                let layout = theme.metrics_changed(&self.theme);
                self.theme = theme;
                let _ = cx.set_environment(self.child, self.theme.clone(), layout);
            }
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let m = cx.measure(self.child, c);
        cx.place(self.child, Point::default());
        m
    }
}
