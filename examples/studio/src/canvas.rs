use crate::kit::*;
use crate::Event;
use fire_ui::*;
use fire_ui_widgets::*;
use std::sync::Arc;

/// A custom widget using exactly the interfaces a built-in control uses: it owns
/// state, handles pointer input, asks for frames while it is moving, and stops.
struct Playground {
    running: bool,
    paddle: f32,
    ball: Point,
    velocity: Point,
    phase: f32,
    title: Option<Arc<Paragraph>>,
}
impl Playground {
    fn new() -> Element<Self> {
        Element::leaf(Self {
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
    /// Whether the animation is running.
    type Command = bool;
    type Output = std::convert::Infallible;
    fn update(&mut self, cx: &mut Update<'_, Self>, running: bool) {
        self.running = running;
        if running {
            cx.request_frame()
        } else {
            cx.cancel_frame()
        }
        cx.repaint()
    }
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, e: Lifecycle) {
        if e == Lifecycle::Visibility(true) && self.running {
            cx.request_frame()
        }
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if phase == Phase::Preview {
            return;
        }
        if let Input::Pointer { position, .. } = input {
            self.paddle = (position.x / cx.bounds().width.max(1.)).clamp(0., 1.);
            cx.repaint()
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
            self.ball.x = self.ball.x.clamp(0.03, 0.97)
        }
        if self.ball.y < 0.12 {
            self.velocity.y = self.velocity.y.abs()
        }
        if self.ball.y > 0.88 {
            if (self.ball.x - self.paddle).abs() < 0.18 {
                self.velocity.y = -self.velocity.y.abs()
            } else {
                self.ball = Point::new(0.5, 0.3)
            }
        }
        cx.repaint();
        cx.request_frame()
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        if self.title.is_none() {
            self.title = Some(cx.paragraph(TextRequest {
                text: Arc::from("fire"),
                style: TextStyle {
                    size: theme(cx).size(TextRole::Display) * 1.5,
                    font: 0,
                },
                width: None,
                revision: 0,
            }))
        }
        Metrics::new(c.constrain(Size::new(c.max.width, 300.)))
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = painted_theme(cx);
        let b = cx.bounds;
        // Embers rising from the floor of the canvas.
        for i in 0..14 {
            let x = b.x + 16. + i as f32 * (b.width - 32.).max(0.) / 14.;
            let base = b.y + b.height - 24.;
            let sway = (self.phase * 3. + i as f32 * 1.7).sin() * 6.;
            let height = 18. + (self.phase * 4. + i as f32).sin().abs() * 34.;
            cx.painter.path(
                &[
                    Path::Move(Point::new(x, base)),
                    Path::Curve(
                        Point::new(x - 9., base - 12.),
                        Point::new(x + sway, base - height),
                        Point::new(x + sway + 2., base - height - 9.),
                    ),
                    Path::Curve(
                        Point::new(x + 15., base - 12.),
                        Point::new(x + 10., base),
                        Point::new(x, base),
                    ),
                    Path::Close,
                ],
                t.color.accent.alpha(0.20).into(),
                None,
            )
        }
        if let Some(p) = &self.title {
            cx.painter.paragraph(
                p,
                Point::new(b.x + 20., b.y + 14.),
                Brush::Linear {
                    start: Point::new(0., b.y + 14.),
                    end: Point::new(0., b.y + 14. + p.size.height),
                    from: t.color.accent_light,
                    to: t.color.accent,
                },
            )
        }
        let paddle_w = (b.width * 0.22).clamp(50., 120.);
        let x =
            b.x + (self.paddle * b.width - paddle_w / 2.).clamp(0., (b.width - paddle_w).max(0.));
        let paddle = Rect::new(x, b.y + b.height - 18., paddle_w, 7.);
        cx.painter.rect(paddle, 3.5, t.color.success.into());
        let ball = Rect::new(
            b.x + self.ball.x * b.width - 6.,
            b.y + self.ball.y * b.height - 6.,
            12.,
            12.,
        );
        glow(cx.painter, ball, 6., 14., t.color.accent.alpha(0.6));
        cx.painter.rect(ball, 6., t.color.accent_light.into());
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::Canvas,
            label: "Paddle and ember canvas".into(),
            value: Some(if self.running { "running" } else { "paused" }.into()),
            ..Semantics::default()
        }
    }
}

/// Custom drawing, animation that sleeps when idle, and direct pointer response.
pub struct Canvas {
    heading: Child<Label>,
    caption: Child<Label>,
    play: Child<Switch>,
    board: Child<Surface<Playground>>,
    note: Child<Label>,
    running: bool,
}
impl Canvas {
    pub fn new() -> Element<Self> {
        Element::build(|children| Self {
            heading: children.add(text("Beyond the usual controls", TextRole::Heading)),
            caption: children.add(muted_paragraph(
                "A canvas is an ordinary widget. Move the pointer to steer the paddle. \
                 While the animation is off, the event loop sleeps and the process uses \
                 no CPU at all.",
                TextRole::Small,
            )),
            play: children.connect(Switch::new("Animate"), |v| *v),
            board: children.discard(
                surface(SurfaceStyle::Sunken)
                    .inset(0.)
                    .wrap(Playground::new()),
            ),
            note: children.add(muted_paragraph(
                "The ember glow uses the same box gradient the focus rings do.",
                TextRole::Small,
            )),
            running: false,
        })
    }
}
impl Widget for Canvas {
    type Command = bool;
    type Output = Event;
    fn update(&mut self, cx: &mut Update<'_, Self>, running: bool) {
        self.running = running;
        let _ = cx.send(self.board, running);
        let _ = cx.emit(Event::Status(
            if running { "Animating" } else { "Paused" }.into(),
        ));
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let unit = gap(cx, 1.);
        column(
            cx,
            c,
            Flow::gap(unit * 1.5).align(Align::Stretch),
            &[
                Entry::natural(self.heading),
                Entry::natural(self.caption),
                Entry::natural(self.play),
                Entry::natural(self.board),
                Entry::natural(self.note),
            ],
        )
    }
}
