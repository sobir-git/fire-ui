use crate::{theme, Theme};
use fire_ui::*;

/// What a surface is doing in the stack of things on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SurfaceStyle {
    /// A panel resting on the ground. The ordinary card.
    #[default]
    Card,
    /// A well that recedes: text fields, list bodies, canvases.
    Sunken,
    /// Grouping without decoration.
    Ghost,
    /// Above everything, with a shadow: menus, dialogs, popovers.
    Floating,
    /// A card carrying the accent, for the one thing that matters most.
    Accent,
}

/// Themed background, border and padding around any content.
///
/// Decoration is ordinary composition: the content keeps its own command and
/// output protocol, and the surface adds no behaviour of its own.
pub struct Surface<C: Widget> {
    child: Child<C>,
    style: SurfaceStyle,
    inset: Option<f32>,
    radius: Option<f32>,
}
impl<C: Widget> Surface<C> {
    pub fn new(content: Element<C>) -> Element<Self> {
        Self::styled(content, SurfaceStyle::default())
    }
    pub fn styled(content: Element<C>, style: SurfaceStyle) -> Element<Self> {
        surface(style).wrap(content)
    }
    fn insets(&self, t: &Theme) -> f32 {
        self.inset.unwrap_or(match self.style {
            SurfaceStyle::Ghost => 0.,
            _ => t.scale.space(2.5),
        })
    }
    fn corner(&self, t: &Theme) -> f32 {
        self.radius.unwrap_or(t.scale.panel_radius)
    }
}
/// Begin a surface whose padding or radius differs from the theme's choice:
/// `surface(SurfaceStyle::Sunken).inset(0.).wrap(content)`.
pub fn surface(style: SurfaceStyle) -> SurfaceBuilder {
    SurfaceBuilder {
        style,
        inset: None,
        radius: None,
    }
}

/// Configures a surface before it wraps content.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceBuilder {
    style: SurfaceStyle,
    inset: Option<f32>,
    radius: Option<f32>,
}
impl SurfaceBuilder {
    /// Override the padding the theme would choose.
    pub fn inset(mut self, inset: f32) -> Self {
        self.inset = Some(inset);
        self
    }
    /// Override the corner radius the theme would choose.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }
    pub fn wrap<C: Widget>(self, content: Element<C>) -> Element<Surface<C>> {
        Element::build(|children| Surface {
            child: children.bubble(content),
            style: self.style,
            inset: self.inset,
            radius: self.radius,
        })
    }
}
impl<C: Widget> Widget for Surface<C> {
    type Command = C::Command;
    type Output = C::Output;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: Self::Command) {
        let _ = cx.send(self.child, command);
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let inset = self
            .insets(&theme(cx))
            .max(0.)
            .min(c.max.width / 2.)
            .min(c.max.height / 2.);
        let m = cx.measure(self.child, c.inset(inset));
        cx.place(self.child, Point::new(inset, inset));
        Metrics {
            size: c.constrain(Size::new(
                m.size.width + 2. * inset,
                m.size.height + 2. * inset,
            )),
            baseline: m.baseline.map(|b| b + inset),
        }
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = crate::painted_theme(cx);
        let r = self.corner(&t);
        match self.style {
            SurfaceStyle::Ghost => {}
            SurfaceStyle::Floating => {
                crate::drop_shadow(cx.painter, cx.bounds, r, 10.);
                cx.painter.rect(cx.bounds, r, t.color.raised.into());
                cx.painter.stroke(
                    cx.bounds.inset(0.5),
                    r,
                    t.scale.border,
                    t.color.border_strong,
                );
            }
            SurfaceStyle::Sunken => {
                cx.painter.rect(cx.bounds, r, t.color.sunken.into());
                cx.painter
                    .stroke(cx.bounds.inset(0.5), r, t.scale.border, t.color.border);
            }
            SurfaceStyle::Accent => {
                crate::glow(cx.painter, cx.bounds, r, 18., t.color.accent.alpha(0.22));
                cx.painter
                    .rect(cx.bounds, r, crate::accent_gradient(cx.bounds, &t));
            }
            SurfaceStyle::Card => {
                cx.painter.rect(cx.bounds, r, t.color.surface.into());
                cx.painter
                    .stroke(cx.bounds.inset(0.5), r, t.scale.border, t.color.border);
            }
        }
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::Group,
            ..Semantics::default()
        }
    }
}
