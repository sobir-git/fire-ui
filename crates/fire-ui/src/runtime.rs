use crate::*;
use std::{collections::BTreeMap, time::Duration};

/// A headless host can drive this directly. A native host translates OS events.
pub struct Ui {
    pub root: Node,
    size: Size,
    focus: Option<WidgetId>,
    capture: Option<WidgetId>,
    hover: Option<WidgetId>,
    modals: Vec<(WidgetId, Option<WidgetId>)>,
    deadlines: BTreeMap<WidgetId, Duration>,
    layout_dirty: bool,
    paint_dirty: bool,
    messages: Vec<Message>,
    platform: Vec<PlatformRequest>,
    pub frames: u64,
    now: Duration,
    damage: Option<Rect>,
}
impl Ui {
    pub fn new(root: Node, size: Size) -> Self {
        Self {
            root,
            size,
            focus: None,
            capture: None,
            hover: None,
            modals: vec![],
            deadlines: BTreeMap::new(),
            layout_dirty: true,
            paint_dirty: true,
            messages: vec![],
            platform: vec![],
            frames: 0,
            now: Duration::ZERO,
            damage: None,
        }
    }
    pub fn focused(&self) -> Option<WidgetId> {
        self.focus
    }
    pub fn captured(&self) -> Option<WidgetId> {
        self.capture
    }
    pub fn needs_paint(&self) -> bool {
        self.paint_dirty
    }
    /// None means there is no scheduled work. The host should sleep indefinitely.
    pub fn next_deadline(&self) -> Option<Duration> {
        self.deadlines.values().copied().min()
    }
    pub fn invalidate(&mut self) {
        self.layout_dirty = true;
        self.repaint_all();
    }
    pub fn repaint_all(&mut self) {
        self.paint_dirty = true;
        self.damage = None;
    }
    pub fn damage(&self) -> Option<Rect> {
        self.paint_dirty
            .then_some(self.damage.unwrap_or(Rect::from_size(self.size)))
    }
    fn repaint_widget(&mut self, id: WidgetId, local: Option<Rect>) {
        if self.root.find(id).is_some_and(|n| !n.clip) {
            self.paint_dirty = true;
            self.damage = None;
            return;
        }
        if let Some(rect) = self.root.window_rect(id) {
            let rect = local
                .map(|r| r.translated(Point::new(rect.x, rect.y)))
                .unwrap_or(rect)
                .inset(-1.);
            if !self.paint_dirty {
                self.damage = Some(rect);
            } else if let Some(old) = self.damage {
                self.damage = Some(old.union(rect));
            }
            self.paint_dirty = true;
        }
    }
    pub fn resize(&mut self, size: Size) {
        if self.size != size {
            self.size = size;
            self.invalidate();
        }
    }
    pub fn messages(&mut self) -> impl Iterator<Item = Message> {
        std::mem::take(&mut self.messages).into_iter()
    }
    pub fn platform_requests(&mut self) -> impl Iterator<Item = PlatformRequest> {
        std::mem::take(&mut self.platform).into_iter()
    }
    pub fn edit<T: Widget>(&mut self, id: WidgetId, f: impl FnOnce(&mut T)) -> bool {
        let Some(widget) = self.root.find_mut(id).and_then(|n| n.widget_mut::<T>()) else {
            return false;
        };
        f(widget);
        self.invalidate();
        true
    }
    pub fn layout(&mut self, text: &mut dyn TextEngine) {
        if self.layout_dirty {
            self.root.layout(Constraints::tight(self.size), text);
            self.root.place(0., 0.);
            self.layout_dirty = false;
        }
        self.deadlines.retain(|id, _| self.root.active(*id));
        if self.capture.is_some_and(|id| !self.root.active(id)) {
            if let Some(id) = self.capture.take() {
                self.send(id, Event::Cancel, self.now, text);
            }
        }
        if self.focus.is_some_and(|id| !self.root.active(id)) {
            self.set_focus(None, self.now, text);
        }
        if self.hover.is_some_and(|id| !self.root.active(id)) {
            self.hover = None;
        }
    }
    /// Call layout after updates, then paint. Painting never performs hidden measurement.
    pub fn paint(&mut self, painter: &mut dyn Painter) {
        assert!(!self.layout_dirty, "Call Ui::layout before Ui::paint");
        self.root.paint_tree(painter, self.focus, self.hover, None);
        self.paint_dirty = false;
        self.damage = None;
        self.frames += 1;
    }
    /// For hosts with a persistent backing surface. Other hosts should use paint().
    pub fn paint_damage(&mut self, painter: &mut dyn Painter) {
        assert!(
            !self.layout_dirty,
            "Call Ui::layout before Ui::paint_damage"
        );
        let damage = self.damage();
        painter.save();
        if let Some(rect) = damage {
            painter.clip(rect);
        }
        self.root
            .paint_tree(painter, self.focus, self.hover, damage);
        painter.restore();
        self.paint_dirty = false;
        self.damage = None;
        self.frames += 1;
    }
    pub fn set_focus(&mut self, id: Option<WidgetId>, now: Duration, text: &mut dyn TextEngine) {
        let id = id.filter(|id| {
            self.root.active(*id)
                && self.root.find(*id).is_some_and(|n| n.widget.focusable())
                && self
                    .modals
                    .last()
                    .is_none_or(|(modal, _)| self.root.find(*modal).is_some_and(|n| n.active(*id)))
        });
        if id == self.focus {
            return;
        }
        let old = self.focus;
        self.focus = id;
        if let Some(old) = old {
            self.send(old, Event::Focus(false), now, text);
            self.repaint_widget(old, None);
        }
        if let Some(id) = id {
            self.send(id, Event::Focus(true), now, text);
            self.repaint_widget(id, None);
        }
    }
    pub fn push_modal(&mut self, id: WidgetId, now: Duration, text: &mut dyn TextEngine) {
        if !self.root.active(id) {
            return;
        }
        if let Some(capture) = self.capture.take() {
            self.send(capture, Event::Cancel, now, text);
        }
        self.modals.push((id, self.focus));
        let mut order = vec![];
        self.root.find(id).unwrap().focus_order(&mut order);
        self.set_focus(order.first().copied(), now, text);
        self.invalidate();
    }
    pub fn pop_modal(&mut self, now: Duration, text: &mut dyn TextEngine) {
        if let Some((_, focus)) = self.modals.pop() {
            self.set_focus(focus, now, text);
            self.invalidate();
        }
    }
    pub fn window_focus_lost(&mut self, now: Duration, text: &mut dyn TextEngine) {
        if let Some(id) = self.capture.take() {
            self.send(id, Event::Cancel, now, text);
        }
        self.set_focus(None, now, text);
    }
    pub fn dispatch(&mut self, event: Event, now: Duration, text: &mut dyn TextEngine) {
        self.layout(text);
        let target = if let Some(position) = event.position() {
            let hit = self.root.hit(position);
            if matches!(event, Event::PointerMove(_)) && self.capture.is_none() && hit != self.hover
            {
                if let Some(old) = self.hover {
                    self.send(old, Event::PointerLeave, now, text);
                    self.repaint_widget(old, None);
                }
                if let Some(id) = hit {
                    self.repaint_widget(id, None);
                }
                self.hover = hit;
            }
            let hit = self.capture.or(hit);
            if let Some((modal, _)) = self.modals.last() {
                hit.filter(|id| self.root.find(*modal).is_some_and(|n| n.active(*id)))
                    .or(Some(*modal))
            } else {
                hit
            }
        } else {
            self.focus.or(self.modals.last().map(|(id, _)| *id))
        };
        let handled = target.is_some_and(|id| self.send(id, event.clone(), now, text));
        if !handled {
            if let Event::Key {
                key: Key::Tab,
                modifiers,
            } = event
            {
                self.cycle_focus(modifiers.shift, now, text);
            }
        }
        if matches!(event, Event::PointerUp(_)) {
            self.capture = None;
        }
    }
    pub fn send(
        &mut self,
        target: WidgetId,
        event: Event,
        now: Duration,
        text: &mut dyn TextEngine,
    ) -> bool {
        self.now = now;
        self.layout(text);
        let mut requests = Requests::default();
        let (_, handled) = route(
            &mut self.root,
            target,
            &event,
            now,
            text,
            self.focus,
            &mut requests,
        );
        if requests.layout {
            self.invalidate();
        } else {
            for (id, rect) in requests.damage {
                self.repaint_widget(id, rect);
            }
        }
        if let Some(capture) = requests.capture {
            self.capture = capture;
        }
        if let Some((id, deadline)) = requests.deadline {
            if let Some(deadline) = deadline {
                self.deadlines.insert(id, deadline);
            } else {
                self.deadlines.remove(&id);
            }
        }
        self.messages.extend(requests.messages);
        self.platform.extend(requests.platform);
        if let Some(focus) = requests.focus {
            self.set_focus(Some(focus), now, text);
        }
        handled
    }
    pub fn advance(&mut self, now: Duration, text: &mut dyn TextEngine) {
        self.layout(text);
        let due: Vec<_> = self
            .deadlines
            .iter()
            .filter(|(_, t)| **t <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in due {
            self.deadlines.remove(&id);
            self.send(id, Event::Frame, now, text);
        }
    }
    fn cycle_focus(&mut self, reverse: bool, now: Duration, text: &mut dyn TextEngine) {
        let mut order = vec![];
        let root = self
            .modals
            .last()
            .and_then(|(id, _)| self.root.find(*id))
            .unwrap_or(&self.root);
        root.focus_order(&mut order);
        if order.is_empty() {
            return;
        }
        let current = order.iter().position(|id| Some(*id) == self.focus);
        let next = match (current, reverse) {
            (Some(i), false) => (i + 1) % order.len(),
            (Some(i), true) => (i + order.len() - 1) % order.len(),
            (None, false) => 0,
            (None, true) => order.len() - 1,
        };
        self.set_focus(Some(order[next]), now, text);
    }
}

fn route(
    node: &mut Node,
    target: WidgetId,
    event: &Event,
    now: Duration,
    text: &mut dyn TextEngine,
    focus: Option<WidgetId>,
    requests: &mut Requests,
) -> (bool, bool) {
    if node.hidden && !matches!(event, Event::Cancel | Event::Focus(false)) {
        return (false, false);
    }
    let local = if event.position().is_some() {
        std::borrow::Cow::Owned(event.local(Point::new(node.rect.x, node.rect.y)))
    } else {
        std::borrow::Cow::Borrowed(event)
    };
    let mut found = node.id == target;
    if !found {
        for child in &mut node.children {
            let (child_found, handled) = route(child, target, &local, now, text, focus, requests);
            if child_found {
                if handled {
                    return (true, true);
                }
                found = true;
                break;
            }
        }
    }
    if found {
        // Focus and timer notifications belong only to their target, never ancestors.
        if node.id != target
            && matches!(
                event,
                Event::Frame | Event::Focus(_) | Event::Cancel | Event::PointerLeave
            )
        {
            return (true, false);
        }
        let mut ctx = EventCtx {
            id: node.id,
            bounds: Rect::from_size(node.rect.size()),
            now,
            focused: focus == Some(node.id),
            text,
            requests,
            handled: false,
        };
        node.widget.event(&mut ctx, &local);
        (true, ctx.handled)
    } else {
        (false, false)
    }
}
