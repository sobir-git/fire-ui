//! Fire UI Studio: every capability the framework ships, in one navigable window.
//!
//! Nothing here reaches past the public API. There is not a single literal colour
//! or hardcoded font size in this binary — the theme decides both — and no panel
//! computes its own pixel positions. If something in this program is awkward, the
//! framework is missing a capability.

mod canvas;
mod controls;
mod kit;
mod layout_page;
mod lists;
mod overview;
mod text_page;
mod theme_page;

use fire_ui::*;
use fire_ui_widgets::*;
use kit::*;

/// What any page can tell the shell.
#[derive(Clone, Debug)]
pub enum Event {
    /// A line for the status bar.
    Status(String),
    /// Swap the whole window's theme, by index into `theme_page::THEMES`.
    Theme(usize),
}
impl Data for Event {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Status(s) => s.capacity(),
                Self::Theme(_) => 0,
            }
    }
}

const SECTIONS: [&str; 7] = [
    "Overview", "Controls", "Text", "Lists", "Canvas", "Layout", "Theme",
];

/// The window's contents: a title, a rail, the current page, and a status line.
///
/// Pages are hidden rather than removed, so an edit or a scroll position survives
/// navigating away and back.
struct Body {
    wordmark: Child<Label>,
    version: Child<Label>,
    nav: Child<Tabs>,
    rule: Child<Divider>,
    status: Child<Label>,
    overview: Child<Scroll<Padding<overview::Overview>>>,
    controls: Child<Scroll<Padding<controls::Controls>>>,
    text: Child<Scroll<Padding<text_page::TextPage>>>,
    lists: Child<Padding<lists::Lists>>,
    canvas: Child<Scroll<Padding<canvas::Canvas>>>,
    layout: Child<Scroll<Padding<layout_page::LayoutPage>>>,
    theme: Child<Scroll<Padding<theme_page::ThemePage>>>,
    section: usize,
}
enum Message {
    Navigate(usize),
    Page(Event),
}
impl Data for Message {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Page(e) => e.bytes(),
                Self::Navigate(_) => 0,
            }
    }
}
/// The margin every page keeps from the window edge, in units of the theme's rhythm.
const PAGE_INSET: f32 = 3.5;

/// Wraps a page in the padding every page shares, inside a viewport.
fn page<W: Widget>(content: Element<W>) -> Element<Scroll<Padding<W>>> {
    Scroll::new(Padding::new(
        content,
        Insets::all(Scale::default().space(PAGE_INSET)),
    ))
}
impl Body {
    fn new() -> Element<Self> {
        Element::build(|c| Self {
            wordmark: c.add(text("Fire UI", TextRole::Title)),
            version: c.add(muted("Studio · 0.8.0", TextRole::Micro)),
            nav: c.connect(Tabs::vertical(SECTIONS), |v| Message::Navigate(*v)),
            rule: c.add(Element::leaf(Divider)),
            status: c.add(muted("Ready", TextRole::Small)),
            overview: c.connect(page(overview::Overview::new()), |e| {
                Message::Page(e.clone())
            }),
            controls: c.connect(page(controls::Controls::new()), |e| {
                Message::Page(e.clone())
            }),
            text: c.connect(page(text_page::TextPage::new()), |e| {
                Message::Page(e.clone())
            }),
            // The list owns its own viewport, so it is not put inside another one.
            lists: c.connect(
                Padding::new(
                    lists::Lists::new(),
                    Insets::all(Scale::default().space(PAGE_INSET)),
                ),
                |e| Message::Page(e.clone()),
            ),
            canvas: c.connect(page(canvas::Canvas::new()), |e| Message::Page(e.clone())),
            layout: c.connect(page(layout_page::LayoutPage::new()), |e| {
                Message::Page(e.clone())
            }),
            theme: c.connect(page(theme_page::ThemePage::new()), |e| {
                Message::Page(e.clone())
            }),
            section: 0,
        })
    }
    /// Shows exactly the page for `section`. Hidden pages keep their state.
    fn reveal(&mut self, cx: &mut Update<'_, Self>) {
        let n = self.section;
        let _ = cx.show(self.overview, n == 0);
        let _ = cx.show(self.controls, n == 1);
        let _ = cx.show(self.text, n == 2);
        let _ = cx.show(self.lists, n == 3);
        let _ = cx.show(self.canvas, n == 4);
        let _ = cx.show(self.layout, n == 5);
        let _ = cx.show(self.theme, n == 6);
        cx.relayout()
    }
    /// Shows a line in the status bar and reports it to the host.
    fn report(&mut self, cx: &mut Update<'_, Self>, line: String) {
        let _ = cx.send(self.status, line.clone());
        let _ = cx.emit(Event::Status(line));
    }
    /// Lays out whichever page is showing into `region`.
    fn place_page(&mut self, cx: &mut Layout<'_>, region: Rect) {
        let c = Constraints::tight(region.size());
        let at = Point::new(region.x, region.y);
        match self.section {
            0 => {
                cx.measure(self.overview, c);
                cx.place(self.overview, at)
            }
            1 => {
                cx.measure(self.controls, c);
                cx.place(self.controls, at)
            }
            2 => {
                cx.measure(self.text, c);
                cx.place(self.text, at)
            }
            3 => {
                cx.measure(self.lists, c);
                cx.place(self.lists, at)
            }
            4 => {
                cx.measure(self.canvas, c);
                cx.place(self.canvas, at)
            }
            5 => {
                cx.measure(self.layout, c);
                cx.place(self.layout, at)
            }
            _ => {
                cx.measure(self.theme, c);
                cx.place(self.theme, at)
            }
        }
    }
}
impl Widget for Body {
    type Command = Message;
    type Output = Event;
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        if event == Lifecycle::Mount {
            self.reveal(cx)
        }
    }
    fn update(&mut self, cx: &mut Update<'_, Self>, message: Message) {
        match message {
            Message::Navigate(section) => {
                if section != self.section {
                    self.section = section;
                    self.reveal(cx);
                    self.report(cx, SECTIONS[section].to_string())
                }
            }
            // The status line is shown here and also reported upwards, so a host —
            // or a test driving the window — sees everything the studio says.
            Message::Page(Event::Status(line)) => self.report(cx, line),
            Message::Page(event) => {
                let _ = cx.emit(event);
            }
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let t = theme(cx);
        let unit = t.scale.space(1.);
        let pad = t.scale.space(2.);
        // A rail while there is room for one; below that the nav sits on top and
        // the page takes the full width.
        let narrow = c.max.width < 720.;
        let rail = if narrow { 0. } else { 208. };

        let head = column_at(
            cx,
            Point::new(pad, pad),
            Constraints::loose(Size::new((rail - pad).max(160.), f32::INFINITY)),
            Flow::gap(unit * 0.25).align(Align::Start),
            &[Entry::natural(self.wordmark), Entry::natural(self.version)],
        );

        let status_height = cx
            .measure(self.status, Constraints::loose(c.max))
            .size
            .height;
        let bar = status_height + unit * 2.;
        let body_height = (c.max.height - bar).max(0.);

        let nav_top = pad + head.size.height + unit * 2.;
        let nav = cx.measure(
            self.nav,
            Constraints::loose(Size::new(
                if narrow {
                    c.max.width - 2. * pad
                } else {
                    rail - 2. * pad
                },
                f32::INFINITY,
            )),
        );
        cx.place(self.nav, Point::new(pad, nav_top));

        let page_top = if narrow {
            nav_top + nav.size.height + unit
        } else {
            0.
        };
        self.place_page(
            cx,
            Rect::new(
                rail,
                page_top,
                (c.max.width - rail).max(0.),
                (body_height - page_top).max(0.),
            ),
        );

        cx.measure(self.rule, Constraints::tight(Size::new(c.max.width, 1.)));
        cx.place(self.rule, Point::new(0., body_height));
        cx.measure(
            self.status,
            Constraints::loose(Size::new((c.max.width - 2. * pad).max(0.), status_height)),
        );
        cx.place(self.status, Point::new(pad, body_height + unit));
        Metrics::new(c.max)
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = painted_theme(cx);
        // The rail sits on its own surface so the page reads as the working area.
        cx.painter.rect(cx.bounds, 0., t.color.background.into());
        if cx.bounds.width >= 720. {
            cx.painter.rect(
                Rect::new(cx.bounds.x, cx.bounds.y, 208., cx.bounds.height),
                0.,
                t.color.surface.into(),
            );
        }
        // One ember line along the top edge: the whole accent budget of the shell.
        cx.painter.rect(
            Rect::new(cx.bounds.x, cx.bounds.y, cx.bounds.width, 2.),
            0.,
            accent_gradient(Rect::new(cx.bounds.x, cx.bounds.y, cx.bounds.width, 2.), &t),
        )
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::Group,
            label: "Fire UI Studio".into(),
            ..Semantics::default()
        }
    }
}

/// The root. It owns the theme and hands it to everything below through the
/// environment, which is the only reason a switch on one page can repaint the rest.
struct Studio {
    scope: Child<AppearanceScope<Body>>,
}
impl Studio {
    fn new() -> Element<Self> {
        Element::build(|c| Studio {
            scope: c.connect(AppearanceScope::new(Body::new(), Theme::dark()), |e| {
                e.clone()
            }),
        })
    }
}
impl Widget for Studio {
    type Command = Event;
    type Output = String;
    fn update(&mut self, cx: &mut Update<'_, Self>, event: Event) {
        match event {
            Event::Theme(index) => {
                let theme = theme_page::theme_for(index);
                let _ = cx.send(self.scope, ScopeCommand::Theme(Box::new(theme)));
                let name = theme_page::THEMES
                    .get(index)
                    .map_or("unknown", |(name, _)| name);
                let _ = cx.emit(format!("theme: {name}"));
            }
            Event::Status(line) => {
                let _ = cx.emit(line);
            }
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let m = cx.measure(self.scope, c);
        cx.place(self.scope, Point::default());
        m
    }
}

fn main() -> Result<(), String> {
    let mut paths =
        vec![fire_ui_text::system_font().ok_or("Set FIRE_UI_FONT to a readable font file")?];
    paths.extend(fallback_fonts());
    let fonts = fire_ui_fonts::Fonts::load(&paths)?;
    fire_ui_native::run_with(
        Studio::new(),
        fire_ui_native::WindowOptions {
            title: "Fire UI Studio".into(),
            size: Size::new(1180., 860.),
            ..fire_ui_native::WindowOptions::default()
        },
        fire_ui_text::Text::new(fonts.clone())?,
        fire_ui_gl::OpenGl {
            fonts,
            partial_repaint: true,
        },
        |status, _| println!("{status}"),
    )
}

/// This application chooses multilingual coverage; the framework loads no defaults.
fn fallback_fonts() -> Vec<std::path::PathBuf> {
    #[cfg(target_os = "linux")]
    {
        [
            &[
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/TTF/DejaVuSans.ttf",
            ][..],
            &[
                "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
            ][..],
            &[
                "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
                "/usr/share/fonts/noto/NotoColorEmoji.ttf",
            ][..],
        ]
        .into_iter()
        .filter_map(|choices| {
            choices
                .iter()
                .map(std::path::PathBuf::from)
                .find(|p| p.is_file())
        })
        .collect()
    }
    #[cfg(not(target_os = "linux"))]
    {
        vec![]
    }
}
