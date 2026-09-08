use fire_ui::*;
use std::rc::Rc;

/// Semantic colours. Widgets name a role; a theme decides its value.
///
/// Every surface has a foreground that is legible on it. `on_accent` pairs with
/// `accent`, `foreground` with `background`, `surface` and `raised`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// The window ground.
    pub background: Color,
    /// Cards and panels resting on the ground.
    pub surface: Color,
    /// A surface under the pointer, or one deliberately lifted.
    pub raised: Color,
    /// Wells that recede: text fields, list bodies, canvases.
    pub sunken: Color,
    /// Hairlines between surfaces.
    pub border: Color,
    /// Borders that must be seen: focused fields, active dividers.
    pub border_strong: Color,
    /// Primary text on `background`, `surface` and `raised`.
    pub foreground: Color,
    /// Secondary text: subtitles, captions, inactive labels.
    pub muted: Color,
    /// Text at the edge of legibility: watermarks, disabled labels.
    pub faint: Color,
    /// The single accent. Fills, active states and emphasis.
    pub accent: Color,
    /// The lighter end of an accent gradient.
    pub accent_light: Color,
    /// Accent under the pointer.
    pub accent_hover: Color,
    /// Text and icons drawn on top of `accent`.
    pub on_accent: Color,
    /// Focus rings and keyboard affordances.
    pub focus: Color,
    /// Text selection wash.
    pub selection: Color,
    /// Destructive actions and errors.
    pub danger: Color,
    /// Confirmation and healthy state.
    pub success: Color,
}
impl Palette {
    /// Warm charcoal ground with a single ember accent. The default.
    pub fn ember_dark() -> Self {
        Self {
            background: Color::hex(0x14110f),
            surface: Color::hex(0x1c1815),
            raised: Color::hex(0x27211b),
            sunken: Color::hex(0x100d0b),
            border: Color::hex(0x332b24),
            border_strong: Color::hex(0x4d4036),
            foreground: Color::hex(0xf5ece2),
            muted: Color::hex(0xa89684),
            faint: Color::hex(0x6f6053),
            accent: Color::hex(0xf2924f),
            accent_light: Color::hex(0xffc07a),
            accent_hover: Color::hex(0xffa869),
            on_accent: Color::hex(0x1a0f06),
            focus: Color::hex(0xffb06a),
            selection: Color::hex(0xf2924f).alpha(0.24),
            danger: Color::hex(0xe5675a),
            success: Color::hex(0xa8c97e),
        }
    }
    /// The same ember accent on warm paper.
    pub fn ember_light() -> Self {
        Self {
            background: Color::hex(0xf6f1ea),
            surface: Color::hex(0xfffcf8),
            raised: Color::hex(0xf0e7dc),
            sunken: Color::hex(0xeee6db),
            border: Color::hex(0xe0d4c5),
            border_strong: Color::hex(0xbfab96),
            foreground: Color::hex(0x231b14),
            muted: Color::hex(0x6b5b4b),
            faint: Color::hex(0x9a8975),
            accent: Color::hex(0xc25a15),
            accent_light: Color::hex(0xe8873c),
            accent_hover: Color::hex(0xa8490c),
            on_accent: Color::hex(0xfff7ef),
            focus: Color::hex(0xc25a15),
            selection: Color::hex(0xc25a15).alpha(0.20),
            danger: Color::hex(0xb02a1c),
            success: Color::hex(0x4c7a2c),
        }
    }
}

/// Sizes and rhythm. Every length a widget draws derives from these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    /// Body text size. The whole type scale is a multiple of it.
    pub font_size: f32,
    /// One unit of the spacing rhythm.
    pub unit: f32,
    /// Padding inside a control, between its border and its content.
    pub inset: f32,
    /// Corner radius for controls.
    pub radius: f32,
    /// Corner radius for panels and cards.
    pub panel_radius: f32,
    /// Hairline width.
    pub border: f32,
    /// The height a single-line control occupies.
    pub control: f32,
    /// Room a control reserves around its own visible box for the halo it draws
    /// when hovered, checked or focused. Without it the halo crosses the widget's
    /// bounds and an ancestor clips it into a visible hard edge.
    pub halo: f32,
}
impl Scale {
    /// `steps` units of the spacing rhythm.
    pub fn space(&self, steps: f32) -> f32 {
        self.unit * steps
    }
}
impl Default for Scale {
    fn default() -> Self {
        Self {
            font_size: 15.,
            unit: 8.,
            inset: 12.,
            radius: 9.,
            panel_radius: 14.,
            border: 1.,
            control: 38.,
            halo: 5.,
        }
    }
}

/// A step on the type scale. Widgets ask for a role, not a number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextRole {
    /// One per screen: the thing the window is.
    Display,
    /// Section openers.
    Title,
    /// Panel and group headings.
    Heading,
    #[default]
    Body,
    /// Supporting text under a heading or control.
    Small,
    /// Metadata, keyboard hints, overlines.
    Micro,
}
impl TextRole {
    /// Multiple of the base body size.
    pub fn ratio(self) -> f32 {
        match self {
            Self::Display => 2.6,
            Self::Title => 1.85,
            Self::Heading => 1.25,
            Self::Body => 1.,
            Self::Small => 0.87,
            Self::Micro => 0.73,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub color: Palette,
    pub scale: Scale,
}
impl Default for Theme {
    fn default() -> Self {
        Self {
            color: Palette::ember_dark(),
            scale: Scale::default(),
        }
    }
}
impl Theme {
    pub fn dark() -> Self {
        Self::default()
    }
    pub fn light() -> Self {
        Self {
            color: Palette::ember_light(),
            scale: Scale::default(),
        }
    }
    /// Resolved size for a type-scale role.
    pub fn size(&self, role: TextRole) -> f32 {
        (self.scale.font_size * role.ratio()).round()
    }
    /// Whether a swap changes geometry and therefore requires layout, not just paint.
    pub fn metrics_changed(&self, previous: &Self) -> bool {
        self.scale != previous.scale
    }
}

/// Which palette entry text draws in. Naming the role rather than the colour is
/// what lets one widget read correctly under every theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorRole {
    #[default]
    Foreground,
    Muted,
    Faint,
    Accent,
    OnAccent,
    Danger,
    Success,
}
impl ColorRole {
    pub fn resolve(self, theme: &Theme) -> Color {
        match self {
            Self::Foreground => theme.color.foreground,
            Self::Muted => theme.color.muted,
            Self::Faint => theme.color.faint,
            Self::Accent => theme.color.accent,
            Self::OnAccent => theme.color.on_accent,
            Self::Danger => theme.color.danger,
            Self::Success => theme.color.success,
        }
    }
}

/// A local override of inherited text appearance. Absent fields inherit.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Appearance {
    pub role: Option<TextRole>,
    pub color: Option<ColorRole>,
    /// An exact colour, when no palette role is the right answer.
    pub foreground: Option<Color>,
    /// An exact size, for the rare case the scale does not have the right step.
    pub font_size: Option<f32>,
}
impl Appearance {
    pub fn role(role: TextRole) -> Self {
        Self {
            role: Some(role),
            ..Self::default()
        }
    }
    /// Draw in a palette role, resolved against whatever theme is in effect.
    pub fn color(mut self, color: ColorRole) -> Self {
        self.color = Some(color);
        self
    }
    pub fn muted(self) -> Self {
        self.color(ColorRole::Muted)
    }
    pub fn accent(self) -> Self {
        self.color(ColorRole::Accent)
    }
    /// Draw in an exact colour, ignoring the palette.
    pub fn exact(mut self, color: Color) -> Self {
        self.foreground = Some(color);
        self
    }
    pub fn size(mut self, size: f32) -> Self {
        self.font_size = Some(size);
        self
    }
    /// The size this appearance draws at under `theme`.
    pub fn resolve_size(&self, theme: &Theme) -> f32 {
        self.font_size
            .unwrap_or_else(|| theme.size(self.role.unwrap_or_default()))
    }
    /// The colour this appearance draws in under `theme`.
    pub fn resolve_color(&self, theme: &Theme) -> Color {
        self.foreground
            .unwrap_or_else(|| self.color.unwrap_or_default().resolve(theme))
    }
}

/// The theme in effect for this subtree, or the default when none was installed.
pub fn theme(cx: &Layout<'_>) -> Theme {
    cx.environment::<Theme>().copied().unwrap_or_default()
}
/// The theme in effect while painting.
pub fn painted_theme(cx: &Paint<'_>) -> Theme {
    cx.environment::<Theme>().copied().unwrap_or_default()
}

/// Installs a theme for its content. Send `ScopeCommand::Theme` to swap it live.
pub struct AppearanceScope<C: Widget> {
    child: Child<C>,
    theme: Rc<Theme>,
}
impl<C: Widget<Output: Clone>> AppearanceScope<C> {
    pub fn new(content: Element<C>, theme: Theme) -> Element<Self> {
        Element::build(|children| Self {
            child: children.connect(content, |o| ScopeCommand::Child(o.clone())),
            theme: Rc::new(theme),
        })
    }
}
pub enum ScopeCommand<C: Data> {
    Child(C),
    /// Boxed: a whole theme dwarfs a typical child command, and every message in
    /// the mailbox would otherwise be sized for it.
    Theme(Box<Theme>),
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
                self.theme = Rc::new(*theme);
                let _ = cx.set_environment(self.child, self.theme.clone(), layout);
                cx.repaint();
            }
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let m = cx.measure(self.child, c);
        cx.place(self.child, Point::default());
        m
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        cx.painter
            .rect(cx.bounds, 0., self.theme.color.background.into());
    }
}
