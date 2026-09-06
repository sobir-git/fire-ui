use fire_ui::*;
use fire_ui_native::{run_with, WindowOptions};
use fire_ui_widgets::*;
use std::{convert::Infallible, sync::Arc};

fn label(text: impl Into<Arc<str>>, size: f32, color: Color) -> Element<Label> {
    Element::leaf(Label::new(text).appearance(Appearance {
        font_size: Some(size),
        foreground: Some(color),
    }))
}
fn button(text: &str) -> Element<Button<Label>> {
    Button::new(Element::leaf(Label::new(text)), text)
}
fn place<W: Widget>(cx: &mut Layout<'_>, child: Child<W>, r: Rect) {
    cx.measure(child, Constraints::tight(r.size()));
    cx.place(child, Point::new(r.x, r.y));
}

// Decoration is ordinary composition. The content keeps its own command and output protocol.
struct Panel<C: Widget> {
    title: Child<Label>,
    subtitle: Child<Label>,
    content: Child<C>,
}
impl<C: Widget> Panel<C> {
    fn new(title: &str, subtitle: &str, content: Element<C>) -> Element<Self> {
        Element::build(|children| Self {
            title: children.add(label(title, 19., Color::hex(0xf0eee4))),
            subtitle: children.add(label(subtitle, 12., Color::hex(0xa9b2a8))),
            content: children.forward(content),
        })
    }
}
impl<C: Widget> Widget for Panel<C> {
    type Command = C::Output;
    type Output = C::Output;
    fn update(&mut self, cx: &mut Update<'_, Self>, output: Self::Command) {
        let _ = cx.emit(output);
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let w = c.max.width;
        let h = c.max.height;
        place(cx, self.title, Rect::new(22., 20., (w - 44.).max(0.), 26.));
        place(
            cx,
            self.subtitle,
            Rect::new(22., 50., (w - 44.).max(0.), 19.),
        );
        place(
            cx,
            self.content,
            Rect::new(22., 85., (w - 44.).max(0.), (h - 107.).max(0.)),
        );
        Metrics::new(c.max)
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        cx.painter.rect(cx.bounds, 12., Color::hex(0x202422).into());
        cx.painter
            .stroke(cx.bounds.inset(0.5), 12., 1., Color::hex(0x343b36));
    }
}

struct Counter {
    count: usize,
    value: Child<Label>,
    add: Child<Button<Label>>,
    pending: bool,
}
impl Counter {
    fn new() -> Element<Self> {
        Element::build(|children| Self {
            count: 0,
            value: children.add(label("0", 36., Color::hex(0xf0eee4))),
            add: children.connect(button("Add one"), |_| ()),
            pending: false,
        })
    }
    fn publish(&mut self, cx: &mut Update<'_, Self>) {
        self.pending = cx.send(self.value, self.count.to_string()).is_err();
        if self.pending {
            cx.request_frame();
        }
    }
}
impl Widget for Counter {
    type Command = ();
    type Output = usize;
    fn update(&mut self, cx: &mut Update<'_, Self>, _: ()) {
        self.count += 1;
        self.publish(cx);
        let _ = cx.emit(self.count);
    }
    fn frame(&mut self, cx: &mut Update<'_, Self>, _: FrameTime) {
        if self.pending {
            self.publish(cx);
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        place(cx, self.value, Rect::new(0., 0., c.max.width, 49.));
        place(cx, self.add, Rect::new(0., 62., c.max.width, 44.));
        Metrics::new(c.max)
    }
}
struct CounterPair {
    left: Child<Counter>,
    right: Child<Counter>,
}
#[derive(Clone, Debug)]
enum Count {
    Left(usize),
    Right(usize),
}
impl Data for Count {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}
impl CounterPair {
    fn new() -> Element<Self> {
        Element::build(|c| Self {
            left: c.connect(Counter::new(), |v| Count::Left(*v)),
            right: c.connect(Counter::new(), |v| Count::Right(*v)),
        })
    }
}
impl Widget for CounterPair {
    type Command = Count;
    type Output = Count;
    fn update(&mut self, cx: &mut Update<'_, Self>, count: Count) {
        let _ = cx.emit(count);
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let w = (c.max.width - 20.).max(0.) / 2.;
        place(cx, self.left, Rect::new(0., 0., w, c.max.height));
        place(cx, self.right, Rect::new(w + 20., 0., w, c.max.height));
        Metrics::new(c.max)
    }
}

type Rows = VirtualList<usize, Label, fn(&usize) -> Element<Label>>;
fn item(key: &usize) -> Element<Label> {
    label(
        format!("  Material study {:05}", key + 1),
        14.,
        Color::hex(0xd7ddd3),
    )
}
struct Picker {
    search: Child<Editor>,
    list: Child<Rows>,
    pending: Option<Vec<usize>>,
}
enum Pick {
    Query(EditorOutput),
    List(ListOutput<usize, Infallible>),
}
impl Data for Pick {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Query(v) => v.bytes(),
                Self::List(v) => v.bytes(),
            }
    }
}
impl Picker {
    fn new() -> Element<Self> {
        Element::build(|c| Self {
            search: c.connect(
                Element::leaf(Editor::field("").placeholder("Find a material study…")),
                |v| Pick::Query(v.clone()),
            ),
            list: c.connect(
                Element::leaf(Rows::new(
                    (0..100_000).collect(),
                    32.,
                    item as fn(&usize) -> Element<Label>,
                )),
                |v| Pick::List(v.clone()),
            ),
            pending: None,
        })
    }
    fn publish(&mut self, cx: &mut Update<'_, Self>) {
        if let Some(keys) = self.pending.take() {
            if let Err((_, ListCommand::Keys(keys))) = cx.send(self.list, ListCommand::Keys(keys)) {
                self.pending = Some(keys);
                cx.request_frame();
            }
        }
    }
}
impl Widget for Picker {
    type Command = Pick;
    type Output = usize;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: Pick) {
        match command {
            Pick::Query(EditorOutput::Changed { text, .. }) => {
                self.pending = Some(
                    (0..100_000)
                        .filter(|i| {
                            text.is_empty() || format!("{:05}", i + 1).contains(text.as_ref())
                        })
                        .collect(),
                );
                self.publish(cx);
            }
            Pick::List(ListOutput::Selected(key)) => {
                let _ = cx.emit(key);
            }
            _ => {}
        }
    }
    fn frame(&mut self, cx: &mut Update<'_, Self>, _: FrameTime) {
        self.publish(cx);
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        place(cx, self.search, Rect::new(0., 0., c.max.width, 44.));
        place(
            cx,
            self.list,
            Rect::new(0., 58., c.max.width, (c.max.height - 58.).max(0.)),
        );
        Metrics::new(c.max)
    }
}

// The canvas uses the same widget protocol as text and buttons. Pointer position is direct.
struct Playground {
    toggle: Child<Button<Label>>,
    running: bool,
    paddle: f32,
    ball: Point,
    velocity: Point,
    phase: f32,
    title: Option<Arc<Paragraph>>,
}
impl Playground {
    fn new() -> Element<Self> {
        Element::build(|c| Self {
            toggle: c.connect(button("Play / pause"), |_| ()),
            running: false,
            paddle: 0.5,
            ball: Point::new(0.45, 0.35),
            velocity: Point::new(0.32, 0.25),
            phase: 0.,
            title: None,
        })
    }
}
impl Widget for Playground {
    type Command = ();
    type Output = Infallible;
    fn update(&mut self, cx: &mut Update<'_, Self>, _: ()) {
        self.running = !self.running;
        if self.running {
            cx.request_frame();
        } else {
            cx.cancel_frame();
        }
        cx.repaint();
    }
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, e: Lifecycle) {
        if e == Lifecycle::Visibility(true) && self.running {
            cx.request_frame();
        }
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if phase == Phase::Preview {
            return;
        }
        if let Input::Pointer { position, .. } = input {
            self.paddle = (position.x / cx.bounds().width.max(1.)).clamp(0., 1.);
            cx.repaint();
        }
    }
    fn frame(&mut self, cx: &mut Update<'_, Self>, time: FrameTime) {
        if !self.running {
            return;
        }
        let dt = time.elapsed.as_secs_f32().min(0.04);
        self.phase += dt;
        self.ball.x += self.velocity.x * dt;
        self.ball.y += self.velocity.y * dt;
        if self.ball.x < 0.03 || self.ball.x > 0.97 {
            self.velocity.x = -self.velocity.x;
            self.ball.x = self.ball.x.clamp(0.03, 0.97);
        }
        if self.ball.y < 0.22 {
            self.velocity.y = self.velocity.y.abs();
        }
        if self.ball.y > 0.83 {
            if (self.ball.x - self.paddle).abs() < 0.18 {
                self.velocity.y = -self.velocity.y.abs();
            } else {
                self.ball = Point::new(0.5, 0.3);
            }
        }
        cx.repaint();
        cx.request_frame();
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        place(
            cx,
            self.toggle,
            Rect::new((c.max.width - 135.).max(0.), 0., 135., 42.),
        );
        if self.title.is_none() {
            self.title = Some(cx.paragraph(TextRequest {
                text: Arc::from("fire"),
                style: TextStyle { size: 56., font: 0 },
                width: None,
                revision: 0,
            }));
        }
        Metrics::new(c.max)
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let w = cx.bounds.width;
        let h = cx.bounds.height;
        cx.painter.rect(cx.bounds, 8., Color::hex(0x151c19).into());
        for i in 0..12 {
            let x = 12. + i as f32 * 8.;
            let y = 52.;
            let sway = (self.phase * 3. + i as f32 * 1.7).sin() * 5.;
            let height = 12. + (self.phase * 4. + i as f32).sin().abs() * 20.;
            cx.painter.path(
                &[
                    Path::Move(Point::new(x, y)),
                    Path::Curve(
                        Point::new(x - 8., y - 10.),
                        Point::new(x + sway, y - height),
                        Point::new(x + sway + 2., y - height - 8.),
                    ),
                    Path::Curve(
                        Point::new(x + 13., y - 10.),
                        Point::new(x + 9., y),
                        Point::new(x, y),
                    ),
                    Path::Close,
                ],
                Color::hex(0xf2924f).alpha(0.28).into(),
                None,
            );
        }
        if let Some(p) = &self.title {
            cx.painter.paragraph(
                p,
                Point::new(12., 27.),
                Brush::Linear {
                    start: Point::new(0., 30.),
                    end: Point::new(0., 90.),
                    from: Color::hex(0xffe0aa),
                    to: Color::hex(0xe97d42),
                },
            );
        }
        let paddle_w = (w * 0.23).clamp(45., 105.);
        let x = (self.paddle * w - paddle_w / 2.).clamp(0., (w - paddle_w).max(0.));
        cx.painter.rect(
            Rect::new(x, h - 21., paddle_w, 7.),
            3.5,
            Color::hex(0xc4d9a9).into(),
        );
        cx.painter.rect(
            Rect::new(self.ball.x * w - 5., self.ball.y * h + 24., 10., 10.),
            5.,
            Color::hex(0xf4b46e).into(),
        );
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::Canvas,
            label: "Paddle and fire canvas".into(),
            ..Semantics::default()
        }
    }
}

struct Studio {
    title: Child<Label>,
    subtitle: Child<Label>,
    status: Child<Label>,
    counters: Child<Panel<CounterPair>>,
    editor: Child<Panel<Editor>>,
    picker: Child<Panel<Picker>>,
    game: Child<Panel<Playground>>,
    footer: Child<Label>,
}
enum Message {
    Count(Count),
    Edit(EditorOutput),
    Pick(usize),
}
impl Data for Message {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Edit(v) => v.bytes(),
                _ => 0,
            }
    }
}
impl Studio {
    fn new() -> Element<Self> {
        Element::build(|c| {
            Self {
        title: c.add(label("Fire UI", 40., Color::hex(0xf0eee4))),
        subtitle: c.add(label("A small toolkit. Room to make it yours.", 15., Color::hex(0xa9b2a8))),
        status: c.add(label("STUDIO  /  PROTOTYPE", 11., Color::hex(0xc4d9a9))),
        counters: c.connect(Panel::new("Independent by design", "Two instances, each with its own state.", CounterPair::new()), |v| Message::Count(v.clone())),
        editor: c.connect(Panel::new("A place to think", "Select, edit, undo. Your text stays yours.", Element::leaf(Editor::new("Good tools leave room for the person using them.\n\nTry a sentence. Move a paragraph. Start again.\n\nCafé · Привет · Καλημέρα"))), |v| Message::Edit(v.clone())),
        picker: c.connect(Panel::new("A hundred thousand possibilities", "Search by number. Only visible rows are mounted.", Picker::new()), |v| Message::Pick(*v)),
        game: c.add(Panel::new("Beyond the usual controls", "Move the paddle. Press play to bring it to life.", Playground::new())),
        footer: c.add(label("Built with Fire UI  ·  Animations rest until you press play", 12., Color::hex(0x89988b))),
    }
        })
    }
}
impl Widget for Studio {
    type Command = Message;
    type Output = String;
    fn update(&mut self, cx: &mut Update<'_, Self>, message: Message) {
        let status = match message {
            Message::Count(Count::Left(n)) => format!("Left counter: {n}"),
            Message::Count(Count::Right(n)) => format!("Right counter: {n}"),
            Message::Pick(n) => format!("Selected material study {:05}", n + 1),
            Message::Edit(EditorOutput::Changed { text, .. }) => {
                format!("{} characters in your note", text.chars().count())
            }
            _ => return,
        };
        let _ = cx.send(self.footer, status.clone());
        let _ = cx.emit(status);
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let w = c.max.width;
        let margin = if w < 700. { 20. } else { 36. };
        let inner = (w - 2. * margin).max(0.);
        place(cx, self.title, Rect::new(margin, 26., inner, 52.));
        place(cx, self.subtitle, Rect::new(margin, 84., inner, 24.));
        place(cx, self.status, Rect::new(margin, 121., inner, 18.));
        let bottom = if w >= 850. {
            let left = inner * 0.54 - 10.;
            let right = inner - left - 20.;
            let x = margin + left + 20.;
            place(cx, self.editor, Rect::new(margin, 164., left, 315.));
            place(cx, self.game, Rect::new(margin, 499., left, 280.));
            place(cx, self.counters, Rect::new(x, 164., right, 235.));
            place(cx, self.picker, Rect::new(x, 419., right, 360.));
            799.
        } else {
            place(cx, self.editor, Rect::new(margin, 164., inner, 315.));
            place(cx, self.counters, Rect::new(margin, 499., inner, 235.));
            place(cx, self.game, Rect::new(margin, 754., inner, 280.));
            place(cx, self.picker, Rect::new(margin, 1054., inner, 360.));
            1434.
        };
        place(cx, self.footer, Rect::new(margin, bottom, inner, 24.));
        Metrics::new(c.constrain(Size::new(w, bottom + 48.)))
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        cx.painter.rect(cx.bounds, 0., Color::hex(0x171c19).into());
        cx.painter.rect(
            Rect::new(0., 0., cx.bounds.width, 3.),
            0.,
            Color::hex(0xc4d9a9).into(),
        );
    }
}
fn main() {
    let result = run_with(
        AppearanceScope::new(
            Scroll::new(Studio::new()),
            std::rc::Rc::new(Theme::default()),
        ),
        WindowOptions {
            title: "Fire UI Studio".into(),
            size: Size::new(1140., 880.),
            ..WindowOptions::default()
        },
        |output, _| println!("{output}"),
    );
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
