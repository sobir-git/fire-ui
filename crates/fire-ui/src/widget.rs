use crate::*;
use std::{
    any::Any,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WidgetId(u64);
impl Default for WidgetId {
    fn default() -> Self {
        Self::new()
    }
}
impl WidgetId {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Backspace,
    Delete,
    Enter,
    Escape,
    Tab,
    Character(char),
}
#[derive(Clone, Debug)]
pub enum Event {
    PointerDown(Point),
    PointerUp(Point),
    PointerMove(Point),
    PointerLeave,
    Scroll { position: Point, delta: Point },
    Key { key: Key, modifiers: Modifiers },
    Text(String),
    Paste(String),
    Composition(String),
    Focus(bool),
    Cancel,
    Frame,
    User(UserEvent),
}
#[derive(Clone)]
pub struct UserEvent(pub std::rc::Rc<dyn Any>);
impl std::fmt::Debug for UserEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UserEvent")
    }
}
impl Event {
    pub fn user<T: Any>(value: T) -> Self {
        Self::User(UserEvent(std::rc::Rc::new(value)))
    }
    pub fn position(&self) -> Option<Point> {
        match self {
            Self::PointerDown(p) | Self::PointerUp(p) | Self::PointerMove(p) => Some(*p),
            Self::Scroll { position, .. } => Some(*position),
            _ => None,
        }
    }
    pub(crate) fn local(&self, offset: Point) -> Self {
        let p = |p: Point| Point::new(p.x - offset.x, p.y - offset.y);
        match self {
            Self::PointerDown(v) => Self::PointerDown(p(*v)),
            Self::PointerUp(v) => Self::PointerUp(p(*v)),
            Self::PointerMove(v) => Self::PointerMove(p(*v)),
            Self::Scroll { position, delta } => Self::Scroll {
                position: p(*position),
                delta: *delta,
            },
            _ => self.clone(),
        }
    }
}

pub struct Message {
    pub source: WidgetId,
    pub value: Box<dyn Any>,
}
pub enum PlatformRequest {
    Copy(String),
    Paste(WidgetId),
}
#[derive(Default)]
pub(crate) struct Requests {
    pub layout: bool,
    pub focus: Option<WidgetId>,
    pub capture: Option<Option<WidgetId>>,
    pub deadline: Option<(WidgetId, Option<Duration>)>,
    pub messages: Vec<Message>,
    pub platform: Vec<PlatformRequest>,
    pub damage: Vec<(WidgetId, Option<Rect>)>,
}

pub struct EventCtx<'a> {
    pub id: WidgetId,
    pub bounds: Rect,
    pub now: Duration,
    pub focused: bool,
    pub text: &'a mut dyn TextEngine,
    pub(crate) requests: &'a mut Requests,
    pub(crate) handled: bool,
}
impl EventCtx<'_> {
    pub fn handle(&mut self) {
        self.handled = true;
    }
    pub fn repaint(&mut self) {
        self.requests.damage.push((self.id, None));
    }
    /// Invalidate a local rectangle when the rest of this widget is unchanged.
    pub fn repaint_rect(&mut self, rect: Rect) {
        self.requests.damage.push((self.id, Some(rect)));
    }
    pub fn relayout(&mut self) {
        self.requests.layout = true;
        self.repaint();
    }
    pub fn focus(&mut self) {
        self.requests.focus = Some(self.id);
    }
    pub fn capture_pointer(&mut self) {
        self.requests.capture = Some(Some(self.id));
    }
    pub fn release_pointer(&mut self) {
        self.requests.capture = Some(None);
    }
    pub fn request_frame_after(&mut self, delay: Duration) {
        self.requests.deadline = Some((
            self.id,
            Some(self.now + delay.max(Duration::from_millis(1))),
        ));
    }
    pub fn cancel_frame(&mut self) {
        self.requests.deadline = Some((self.id, None));
    }
    pub fn emit<T: Any>(&mut self, value: T) {
        self.requests.messages.push(Message {
            source: self.id,
            value: Box::new(value),
        });
    }
    pub fn copy(&mut self, text: String) {
        self.requests.platform.push(PlatformRequest::Copy(text));
    }
    pub fn paste(&mut self) {
        self.requests.platform.push(PlatformRequest::Paste(self.id));
    }
}

pub struct PaintCtx<'a> {
    pub bounds: Rect,
    pub focused: bool,
    pub hovered: bool,
    pub painter: &'a mut dyn Painter,
}

/// Implemented by both low-level custom widgets and reusable composition layers.
/// Containers lay out their children; the runtime handles traversal and routing.
pub trait Widget: Any {
    fn layout(
        &mut self,
        children: &mut [Node],
        constraints: Constraints,
        text: &mut dyn TextEngine,
    ) -> Size;
    fn event(&mut self, _ctx: &mut EventCtx<'_>, _event: &Event) {}
    fn paint(&self, _ctx: &mut PaintCtx<'_>) {}
    fn focusable(&self) -> bool {
        false
    }
}

pub struct Node {
    pub id: WidgetId,
    pub rect: Rect,
    pub children: Vec<Node>,
    pub hidden: bool,
    pub clip: bool,
    pub widget: Box<dyn Widget>,
}
impl Node {
    pub fn window_rect(&self, id: WidgetId) -> Option<Rect> {
        if self.id == id {
            return Some(self.rect);
        }
        self.children
            .iter()
            .find_map(|n| n.window_rect(id))
            .map(|r| r.translated(Point::new(self.rect.x, self.rect.y)))
    }
    pub fn new(widget: impl Widget) -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            children: vec![],
            hidden: false,
            clip: true,
            widget: Box::new(widget),
        }
    }
    pub fn with_id(mut self, id: WidgetId) -> Self {
        self.id = id;
        self
    }
    pub fn child(mut self, child: Node) -> Self {
        self.children.push(child);
        self
    }
    pub fn children(mut self, children: impl IntoIterator<Item = Node>) -> Self {
        self.children.extend(children);
        self
    }
    pub fn layout(&mut self, constraints: Constraints, text: &mut dyn TextEngine) -> Size {
        if self.hidden {
            self.rect.width = 0.;
            self.rect.height = 0.;
            return Size::default();
        }
        let size = constraints.constrain(self.widget.layout(&mut self.children, constraints, text));
        self.rect.width = size.width;
        self.rect.height = size.height;
        size
    }
    pub fn place(&mut self, x: f32, y: f32) {
        self.rect.x = x;
        self.rect.y = y;
    }
    pub fn find(&self, id: WidgetId) -> Option<&Node> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|n| n.find(id))
    }
    pub fn find_mut(&mut self, id: WidgetId) -> Option<&mut Node> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter_mut().find_map(|n| n.find_mut(id))
    }
    pub fn widget_mut<T: Widget>(&mut self) -> Option<&mut T> {
        (&mut *self.widget as &mut dyn Any).downcast_mut()
    }
    pub fn widget<T: Widget>(&self) -> Option<&T> {
        (&*self.widget as &dyn Any).downcast_ref()
    }
    pub(crate) fn active(&self, id: WidgetId) -> bool {
        !self.hidden && (self.id == id || self.children.iter().any(|c| c.active(id)))
    }
    pub(crate) fn focus_order(&self, out: &mut Vec<WidgetId>) {
        if self.hidden {
            return;
        }
        if self.widget.focusable() {
            out.push(self.id);
        }
        for c in &self.children {
            c.focus_order(out);
        }
    }
    pub(crate) fn hit(&self, point: Point) -> Option<WidgetId> {
        if self.hidden {
            return None;
        }
        let inside = self.rect.contains(point);
        if self.clip && !inside {
            return None;
        }
        let local = Point::new(point.x - self.rect.x, point.y - self.rect.y);
        for child in self.children.iter().rev() {
            if let Some(id) = child.hit(local) {
                return Some(id);
            }
        }
        inside.then_some(self.id)
    }
    pub(crate) fn paint_tree(
        &self,
        painter: &mut dyn Painter,
        focus: Option<WidgetId>,
        hover: Option<WidgetId>,
        damage: Option<Rect>,
    ) {
        if self.hidden {
            return;
        }
        if self.clip && damage.is_some_and(|r| !self.rect.intersects(r)) {
            return;
        }
        let damage = damage.map(|r| r.translated(Point::new(-self.rect.x, -self.rect.y)));
        painter.save();
        painter.translate(Point::new(self.rect.x, self.rect.y));
        let bounds = Rect::from_size(self.rect.size());
        if self.clip {
            painter.clip(bounds);
        }
        self.widget.paint(&mut PaintCtx {
            bounds,
            focused: focus == Some(self.id),
            hovered: hover == Some(self.id),
            painter,
        });
        for child in &self.children {
            child.paint_tree(painter, focus, hover, damage);
        }
        painter.restore();
    }
}
