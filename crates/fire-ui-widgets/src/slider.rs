use crate::theme;
use fire_ui::*;
use std::convert::Infallible;

#[derive(Clone, Copy, Debug)]
pub enum SliderCommand {
    Value(f32),
    Disabled(bool),
}
impl Data for SliderCommand {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}

/// A value chosen by dragging along a track.
///
/// Pointer drags capture, arrows move by one step, Page keys by ten, Home and End
/// jump to the ends. The value is published numerically, so assistive technology
/// announces and adjusts a quantity rather than a string.
pub struct Slider {
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    label: String,
    disabled: bool,
    dragging: Option<u32>,
    /// Knob diameter from the last layout, so pointer maths matches what is drawn.
    knob: f32,
    /// Room reserved at each end for the knob's halo.
    halo: f32,
}
impl Slider {
    /// A slider over `min..=max`, starting at `value`.
    pub fn new(label: impl Into<String>, value: f32, min: f32, max: f32) -> Element<Self> {
        let max = if max > min { max } else { min + 1. };
        Element::leaf(Self {
            value: value.clamp(min, max),
            min,
            max,
            step: (max - min) / 100.,
            label: label.into(),
            disabled: false,
            dragging: None,
            knob: 18.,
            halo: 5.,
        })
    }
    /// A slider that moves in fixed increments. `step` of zero is continuous.
    pub fn stepped(
        label: impl Into<String>,
        value: f32,
        min: f32,
        max: f32,
        step: f32,
    ) -> Element<Self> {
        let max = if max > min { max } else { min + 1. };
        Element::leaf(Self {
            value: value.clamp(min, max),
            min,
            max,
            step: step.max(0.),
            label: label.into(),
            disabled: false,
            dragging: None,
            knob: 18.,
            halo: 5.,
        })
    }
    pub fn value(&self) -> f32 {
        self.value
    }
    fn fraction(&self) -> f32 {
        ((self.value - self.min) / (self.max - self.min)).clamp(0., 1.)
    }
    fn quantise(&self, value: f32) -> f32 {
        let clamped = value.clamp(self.min, self.max);
        if self.step <= 0. {
            return clamped;
        }
        let steps = ((clamped - self.min) / self.step).round();
        (self.min + steps * self.step).clamp(self.min, self.max)
    }
    /// Applies a new value, emitting it when it actually changed.
    fn set(&mut self, cx: &mut Update<'_, Self>, value: f32) {
        let value = self.quantise(value);
        if value != self.value {
            self.value = value;
            let _ = cx.emit(value);
            cx.repaint()
        }
    }
    fn value_at(&self, cx: &Update<'_, Self>, x: f32) -> f32 {
        let travel = (cx.bounds().width - self.knob - 2. * self.halo).max(1.);
        let offset = (x - self.knob / 2. - self.halo) / travel;
        self.min + offset.clamp(0., 1.) * (self.max - self.min)
    }
    fn knob_for(t: &crate::Theme) -> f32 {
        (t.scale.font_size * 1.2).round()
    }
}
impl Widget for Slider {
    type Command = SliderCommand;
    type Output = f32;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: SliderCommand) {
        match command {
            SliderCommand::Value(value) => {
                let value = self.quantise(value);
                if value != self.value {
                    self.value = value;
                    cx.repaint()
                }
            }
            SliderCommand::Disabled(value) => {
                self.disabled = value;
                if let Some(pointer) = self.dragging.take() {
                    let _ = cx.release(pointer);
                }
                cx.repaint()
            }
        }
    }
    fn focusable(&self) -> bool {
        !self.disabled
    }
    fn cursor(&self, _: Point) -> Option<CursorIcon> {
        Some(if self.disabled {
            CursorIcon::Arrow
        } else {
            CursorIcon::Pointer
        })
    }
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        match event {
            Lifecycle::CaptureLost(_) => {
                self.dragging = None;
                cx.repaint()
            }
            Lifecycle::Hover(_) | Lifecycle::Focus(_) => cx.repaint(),
            _ => {}
        }
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if phase == Phase::Preview || self.disabled {
            return;
        }
        let span = self.max - self.min;
        let step = if self.step > 0. {
            self.step
        } else {
            span / 100.
        };
        match input {
            Input::Button {
                pointer,
                button: 1,
                down: true,
                position,
            } => {
                self.dragging = Some(*pointer);
                let _ = cx.focus();
                let _ = cx.capture(*pointer);
                let value = self.value_at(cx, position.x);
                self.set(cx, value);
                cx.stop()
            }
            Input::Pointer { position, .. } if self.dragging.is_some() => {
                let value = self.value_at(cx, position.x);
                self.set(cx, value);
                cx.stop()
            }
            Input::Button {
                pointer,
                button: 1,
                down: false,
                ..
            } if self.dragging == Some(*pointer) => {
                self.dragging = None;
                let _ = cx.release(*pointer);
                cx.repaint();
                cx.stop()
            }
            Input::Key {
                key, down: true, ..
            } => {
                let target = match key {
                    Key::Left | Key::Down => self.value - step,
                    Key::Right | Key::Up => self.value + step,
                    Key::PageDown => self.value - step * 10.,
                    Key::PageUp => self.value + step * 10.,
                    Key::Home => self.min,
                    Key::End => self.max,
                    _ => return,
                };
                self.set(cx, target);
                cx.stop()
            }
            _ => {}
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let t = theme(cx);
        self.knob = Self::knob_for(&t);
        self.halo = t.scale.halo;
        Metrics::new(c.constrain(Size::new(
            if c.max.width.is_finite() {
                c.max.width
            } else {
                t.scale.space(20.)
            },
            self.knob + 2. * self.halo,
        )))
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = crate::painted_theme(cx);
        let knob = Self::knob_for(&t);
        let halo = t.scale.halo;
        let height = (knob * 0.4).round();
        // The track stops short of both ends so the knob — and the halo around it —
        // stay inside the widget's own bounds at either extreme.
        let track = Rect::new(
            cx.bounds.x + knob / 2. + halo,
            cx.bounds.y + (cx.bounds.height - height) / 2.,
            (cx.bounds.width - knob - 2. * halo).max(0.),
            height,
        );
        let radius = height / 2.;
        cx.painter.rect(track, radius, t.color.sunken.into());
        cx.painter
            .stroke(track.inset(0.5), radius, t.scale.border, t.color.border);
        let filled = Rect::new(
            track.x,
            track.y,
            track.width * self.fraction(),
            track.height,
        );
        if filled.width > 0. && !self.disabled {
            cx.painter
                .rect(filled, radius, crate::accent_gradient(filled, &t));
        }
        let center = Point::new(
            track.x + track.width * self.fraction(),
            track.y + track.height / 2.,
        );
        let r = Rect::new(center.x - knob / 2., center.y - knob / 2., knob, knob);
        if !self.disabled && (cx.hovered || self.dragging.is_some()) {
            crate::glow(cx.painter, r, knob / 2., 10., t.color.accent.alpha(0.45));
        }
        cx.painter.rect(
            r,
            knob / 2.,
            if self.disabled {
                t.color.border_strong
            } else {
                t.color.foreground
            }
            .into(),
        );
        if cx.focused && !self.disabled {
            crate::focus_ring(cx.painter, r.inset(-2.), knob / 2. + 2., &t);
        }
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::Slider,
            label: self.label.clone(),
            value: Some(format!("{:.2}", self.value)),
            range: Some(fire_ui::Range {
                now: self.value,
                min: self.min,
                max: self.max,
                step: self.step,
            }),
            disabled: self.disabled,
            actions: vec![SemanticActionKind::Focus, SemanticActionKind::SetValue],
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
            SemanticAction::Focus => cx.focus().map_err(|_| SemanticError::Unavailable),
            SemanticAction::SetValue(value) => {
                let value: f32 = value
                    .trim()
                    .parse()
                    .map_err(|_| SemanticError::InvalidValue)?;
                if !value.is_finite() {
                    return Err(SemanticError::InvalidValue);
                }
                self.set(cx, value);
                Ok(())
            }
            _ => Err(SemanticError::Unsupported),
        }
    }
}

/// A read-only bar showing how far along something is.
pub struct Progress {
    fraction: f32,
    label: String,
}
impl Progress {
    pub fn new(label: impl Into<String>, fraction: f32) -> Element<Self> {
        Element::leaf(Self {
            fraction: fraction.clamp(0., 1.),
            label: label.into(),
        })
    }
    pub fn fraction(&self) -> f32 {
        self.fraction
    }
}
impl Widget for Progress {
    /// The completed fraction, from zero to one.
    type Command = f32;
    type Output = Infallible;
    fn update(&mut self, cx: &mut Update<'_, Self>, fraction: f32) {
        let fraction = fraction.clamp(0., 1.);
        if fraction != self.fraction {
            self.fraction = fraction;
            cx.repaint()
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let t = theme(cx);
        Metrics::new(c.constrain(Size::new(
            if c.max.width.is_finite() {
                c.max.width
            } else {
                t.scale.space(20.)
            },
            (t.scale.font_size * 0.5).round(),
        )))
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = crate::painted_theme(cx);
        let radius = cx.bounds.height / 2.;
        cx.painter.rect(cx.bounds, radius, t.color.sunken.into());
        let filled = Rect::new(
            cx.bounds.x,
            cx.bounds.y,
            cx.bounds.width * self.fraction,
            cx.bounds.height,
        );
        if filled.width > 0. {
            cx.painter
                .rect(filled, radius, crate::accent_gradient(filled, &t));
        }
        cx.painter
            .stroke(cx.bounds.inset(0.5), radius, t.scale.border, t.color.border);
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::Progress,
            label: self.label.clone(),
            value: Some(format!("{}%", (self.fraction * 100.).round())),
            range: Some(fire_ui::Range {
                now: self.fraction,
                min: 0.,
                max: 1.,
                step: 0.01,
            }),
            ..Semantics::default()
        }
    }
}
