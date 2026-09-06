use fire_ui::*;
use fire_ui_widgets::Theme;
use std::time::Duration;

pub struct FireText {
    theme: Theme,
    running: bool,
    time: f32,
}
impl FireText {
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            running: false,
            time: 0.,
        }
    }
}
impl Widget for FireText {
    fn layout(&mut self, _: &mut [Node], c: Constraints, _: &mut dyn TextEngine) -> Size {
        c.max
    }
    fn event(&mut self, ctx: &mut EventCtx<'_>, event: &Event) {
        match event {
            Event::PointerDown(_)
            | Event::Key {
                key: Key::Character(' '),
                ..
            } => {
                self.running = !self.running;
                if self.running {
                    ctx.request_frame_after(Duration::from_millis(16));
                } else {
                    ctx.cancel_frame();
                }
                ctx.repaint();
                ctx.handle();
            }
            Event::Frame if self.running => {
                self.time = ctx.now.as_secs_f32();
                ctx.repaint();
                ctx.request_frame_after(Duration::from_millis(16));
            }
            _ => {}
        }
    }
    fn paint(&self, ctx: &mut PaintCtx<'_>) {
        let w = ctx.bounds.width;
        let h = ctx.bounds.height;
        ctx.painter
            .rect(ctx.bounds, 6., Color::hex(0x251d17).into());
        for i in 0..32 {
            let seed = i as f32 * 1.618;
            let age = (self.time * 0.45 + seed).fract();
            let x = 16. + (seed * 39.3 + self.time.sin() * i as f32 * 0.2) % (w - 32.);
            let y = h - 24. - age * (h - 25.);
            let size = 1. + (i % 3) as f32;
            ctx.painter.rect(
                Rect::new(x, y, size, size * 2.),
                size,
                Color::hex(0xf49c45).alpha((1. - age) * 0.55).into(),
            );
        }
        ctx.painter.text(
            "FIRE",
            Point::new(26., 43.),
            TextStyle { size: 66., font: 0 },
            Brush::Linear {
                start: Point::new(0., 35.),
                end: Point::new(0., 120.),
                from: Color::hex(0xffefc2),
                to: Color::hex(0xec693a),
            },
        );
        ctx.painter.text(
            if self.running {
                "CUSTOM PAINT / LIVE"
            } else {
                "CUSTOM PAINT / PAUSED"
            },
            Point::new(28., 136.),
            TextStyle { size: 10., font: 0 },
            self.theme.accent.into(),
        );
    }
}

pub struct Rally {
    theme: Theme,
    running: bool,
    ball: Point,
    velocity: Point,
    paddle: f32,
    size: Size,
    last: Duration,
    score: u32,
}
impl Rally {
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            running: false,
            ball: Point::new(140., 50.),
            velocity: Point::new(100., 135.),
            paddle: 140.,
            size: Size::default(),
            last: Duration::ZERO,
            score: 0,
        }
    }
}
impl Widget for Rally {
    fn focusable(&self) -> bool {
        true
    }
    fn layout(&mut self, _: &mut [Node], c: Constraints, _: &mut dyn TextEngine) -> Size {
        self.size = c.max;
        self.ball.x = self.ball.x.clamp(8., c.max.width.max(16.) - 8.);
        self.ball.y = self.ball.y.clamp(8., c.max.height.max(16.) - 8.);
        c.max
    }
    fn event(&mut self, ctx: &mut EventCtx<'_>, event: &Event) {
        match event {
            Event::PointerDown(_)
            | Event::Key {
                key: Key::Character(' '),
                ..
            } => {
                ctx.focus();
                self.running = !self.running;
                self.last = ctx.now;
                if self.running {
                    ctx.request_frame_after(Duration::from_millis(16));
                } else {
                    ctx.cancel_frame();
                }
                ctx.repaint();
                ctx.handle();
            }
            Event::PointerMove(p) => {
                self.paddle = p.x.clamp(28., self.size.width.max(56.) - 28.);
                ctx.repaint();
            }
            Event::Key {
                key: Key::Left | Key::Right,
                ..
            } => {
                let delta = if matches!(event, Event::Key { key: Key::Left, .. }) {
                    -24.
                } else {
                    24.
                };
                self.paddle = (self.paddle + delta).clamp(28., self.size.width.max(56.) - 28.);
                ctx.repaint();
                ctx.handle();
            }
            Event::Frame if self.running => {
                let dt = ctx.now.saturating_sub(self.last).as_secs_f32().min(0.04);
                self.last = ctx.now;
                self.ball.x += self.velocity.x * dt;
                self.ball.y += self.velocity.y * dt;
                if self.ball.x < 8. {
                    self.ball.x = 8.;
                    self.velocity.x = self.velocity.x.abs();
                }
                if self.ball.x > self.size.width - 8. {
                    self.ball.x = self.size.width - 8.;
                    self.velocity.x = -self.velocity.x.abs();
                }
                if self.ball.y < 32. {
                    self.ball.y = 32.;
                    self.velocity.y = self.velocity.y.abs();
                }
                let paddle_y = self.size.height - 24.;
                if self.ball.y >= paddle_y - 7.
                    && self.velocity.y > 0.
                    && (self.ball.x - self.paddle).abs() < 35.
                {
                    self.ball.y = paddle_y - 7.;
                    self.velocity.y = -self.velocity.y.abs();
                    self.score += 1;
                }
                if self.ball.y > self.size.height {
                    self.running = false;
                    self.ball = Point::new(self.size.width * 0.5, 50.);
                    self.score = 0;
                }
                if self.running {
                    ctx.request_frame_after(Duration::from_millis(16));
                }
                ctx.repaint();
            }
            _ => {}
        }
    }
    fn paint(&self, ctx: &mut PaintCtx<'_>) {
        let t = self.theme;
        ctx.painter.rect(ctx.bounds, 6., t.panel.into());
        ctx.painter.stroke(
            ctx.bounds.inset(0.5),
            6.,
            1.,
            if ctx.focused { t.accent } else { t.border },
        );
        ctx.painter.text(
            &format!("RALLY   /   {:02}", self.score),
            Point::new(14., 12.),
            TextStyle { size: 11., font: 0 },
            t.muted.into(),
        );
        ctx.painter.rect(
            Rect::new(self.ball.x - 6., self.ball.y - 6., 12., 12.),
            6.,
            t.accent.into(),
        );
        ctx.painter.rect(
            Rect::new(self.paddle - 28., ctx.bounds.height - 24., 56., 5.),
            2.5,
            t.foreground.into(),
        );
        if !self.running {
            ctx.painter.text(
                "CLICK TO PLAY",
                Point::new(70., ctx.bounds.height * 0.52),
                TextStyle { size: 13., font: 0 },
                t.foreground.into(),
            );
        }
    }
}
