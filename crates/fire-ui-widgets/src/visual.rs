use crate::Theme;
use fire_ui::*;

/// A soft halo around `rect`, fading to nothing over `feather`.
///
/// The interior is filled too, so draw this before the thing it surrounds.
/// Parent clipping may trim the outer edge; a glow is never load-bearing.
pub fn glow(painter: &mut dyn Painter, rect: Rect, radius: f32, feather: f32, color: Color) {
    painter.rect(
        rect.inset(-feather),
        radius + feather,
        Brush::Box {
            rect,
            radius,
            feather,
            from: color,
            to: color.alpha(0.),
        },
    );
}

/// The keyboard focus affordance: a ring drawn inside the control's own bounds, so
/// it survives clipping by an ancestor.
///
/// This draws only the ring. A halo would have to be filled, and `glow` fills its
/// interior as well as its edge, so a control that wants one calls `glow` *before*
/// painting itself rather than washing its own surface afterwards.
pub fn focus_ring(painter: &mut dyn Painter, rect: Rect, radius: f32, theme: &Theme) {
    painter.stroke(rect.inset(1.), (radius - 1.).max(0.), 2., theme.color.focus);
}

/// A shadow cast straight down, for surfaces that float above the ground.
pub fn drop_shadow(painter: &mut dyn Painter, rect: Rect, radius: f32, depth: f32) {
    let cast = Rect::new(rect.x, rect.y + depth * 0.5, rect.width, rect.height);
    painter.rect(
        cast.inset(-depth * 2.),
        radius + depth * 2.,
        Brush::Box {
            rect: cast,
            radius,
            feather: depth * 2.,
            from: Color::hex(0x000000).alpha(0.45),
            to: Color::hex(0x000000).alpha(0.),
        },
    );
}

/// A vertical accent gradient, light at the top.
pub fn accent_gradient(rect: Rect, theme: &Theme) -> Brush {
    Brush::Linear {
        start: Point::new(rect.x, rect.y),
        end: Point::new(rect.x, rect.y + rect.height),
        from: theme.color.accent_light,
        to: theme.color.accent,
    }
}

/// The vertical scrollbar every scrolling surface draws, so a viewport and a list
/// look and behave the same.
///
/// The bar occupies its own strip: content is measured to the width beside it
/// rather than sliding underneath, which keeps it clickable and stops the content
/// below it from claiming the pointer.
#[derive(Clone, Copy, Debug)]
pub struct Scrollbar {
    pub track: Rect,
    pub thumb: Rect,
}
impl Scrollbar {
    /// The strip a scrollbar occupies when one is needed.
    pub fn width(theme: &Theme) -> f32 {
        theme.scale.space(1.5)
    }
    /// `None` when the content fits and no bar is needed.
    pub fn new(
        bounds: Rect,
        viewport: f32,
        extent: f32,
        offset: f32,
        theme: &Theme,
    ) -> Option<Self> {
        let overflow = extent - viewport;
        if overflow <= 0. || viewport <= 0. {
            return None;
        }
        let width = Self::width(theme);
        let pad = theme.scale.space(0.25);
        let track = Rect::new(
            bounds.x + bounds.width - width + pad,
            bounds.y + pad,
            (width - 2. * pad).max(1.),
            (bounds.height - 2. * pad).max(0.),
        );
        let ratio = (viewport / extent).clamp(0., 1.);
        let height = (track.height * ratio).max(28.).min(track.height);
        let travel = track.height - height;
        Some(Self {
            track,
            thumb: Rect::new(
                track.x,
                track.y + travel * (offset / overflow).clamp(0., 1.),
                track.width,
                height,
            ),
        })
    }
    /// The offset that puts the thumb's top edge at `y`.
    pub fn offset_for(&self, y: f32, overflow: f32) -> f32 {
        let travel = (self.track.height - self.thumb.height).max(1.);
        ((y - self.track.y) / travel).clamp(0., 1.) * overflow
    }
    /// Whether `position` is on the bar, and so not on the content beside it.
    pub fn contains(&self, position: Point) -> bool {
        self.track.contains(position)
    }
    /// Paints the bar. The thumb grows to the full track width and brightens while
    /// the pointer is on it, and takes the accent while it is being dragged, so the
    /// bar answers the pointer the way every other control does.
    pub fn paint(&self, painter: &mut dyn Painter, theme: &Theme, hovered: bool, dragging: bool) {
        let active = hovered || dragging;
        let radius = self.track.width / 2.;
        painter.rect(
            self.track,
            radius,
            theme
                .color
                .sunken
                .alpha(if active { 0.9 } else { 0.55 })
                .into(),
        );
        // At rest the thumb sits narrower than its track; hovering fills the track.
        let slim = (self.track.width * 0.28).min(3.);
        let thumb = if active {
            self.thumb
        } else {
            Rect::new(
                self.thumb.x + slim,
                self.thumb.y,
                (self.thumb.width - 2. * slim).max(1.),
                self.thumb.height,
            )
        };
        painter.rect(
            thumb,
            thumb.width / 2.,
            if dragging {
                theme.color.accent
            } else if hovered {
                theme.color.foreground
            } else {
                theme.color.border_strong
            }
            .into(),
        )
    }
}
