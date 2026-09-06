use crate::{
    context::{Effects, Mutation, RawUpdate},
    tree::Tree,
    widget::{Erased, Id, Payload},
    *,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
    marker::PhantomData,
    rc::Rc,
    time::Duration,
};
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub nodes: usize,
    pub messages: usize,
    pub message_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            nodes: 8192,
            messages: 16384,
            message_bytes: 4 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Work {
    pub ready: bool,
    pub frame: bool,
    pub deadline: Option<Duration>,
    pub paint: bool,
}
impl Work {
    pub fn is_idle(self) -> bool {
        !self.ready && !self.frame && !self.paint && self.deadline.is_none()
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub nodes: usize,
    pub queued: usize,
    pub queued_bytes: usize,
    pub layouts: u64,
    pub geometries: u64,
    pub painted: u64,
    pub stale: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct PasteToken {
    owner: Id,
    session: u64,
}
pub enum HostRequest {
    Copy(String),
    Paste(PasteToken),
}
pub(crate) enum Delivery {
    Mount(Id),
    Command(Id, Payload, Option<(Id, u64)>),
    Output(Id, Payload),
    Effects(Id, Effects),
    Clipboard(String),
    Paste(Id),
}
pub(crate) struct Mailbox {
    items: VecDeque<(Delivery, usize)>,
    capacity: usize,
    byte_limit: usize,
    reserved: usize,
    reserved_bytes: usize,
    pub reserved_nodes: usize,
    bytes: usize,
}
impl Mailbox {
    pub fn reserve(&mut self, count: usize, bytes: usize) -> bool {
        if count
            > self
                .capacity
                .saturating_sub(self.items.len() + self.reserved)
            || bytes
                > self
                    .byte_limit
                    .saturating_sub(self.bytes + self.reserved_bytes)
        {
            return false;
        }
        self.reserved += count;
        self.reserved_bytes += bytes;
        true
    }
    pub fn release(&mut self, count: usize, bytes: usize) {
        self.reserved -= count;
        self.reserved_bytes -= bytes
    }
    pub fn push_reserved(&mut self, delivery: Delivery, bytes: usize) {
        self.release(1, bytes);
        self.bytes += bytes;
        self.items.push_back((delivery, bytes));
    }
    fn pop(&mut self) -> Option<(Delivery, usize)> {
        let (item, bytes) = self.items.pop_front()?;
        self.bytes -= bytes;
        Some((item, bytes))
    }
}
#[derive(Clone, Debug)]
pub struct SemanticNode {
    pub id: u64,
    pub parent: Option<u64>,
    pub bounds: Rect,
    pub focused: bool,
    pub focusable: bool,
    pub semantics: Semantics,
}
/// One typed root. All widget storage and routing metadata are private.
pub struct Ui<W: Widget> {
    tree: Tree,
    root: Id,
    mailbox: Mailbox,
    limits: Limits,
    size: Size,
    now: Duration,
    last_frame: Duration,
    focus: Option<Id>,
    hover: Option<Id>,
    captures: BTreeMap<u32, Id>,
    buttons: BTreeSet<(u32, u16)>,
    pressed: BTreeSet<u32>,
    session: u64,
    window_focused: bool,
    window_saved_focus: Option<Id>,
    modals: Vec<(Id, Option<Id>)>,
    frames: VecDeque<Id>,
    frame_set: HashSet<Id>,
    timers: BTreeMap<(Id, Timer), (Duration, Lifetime, u64)>,
    timer_index: BTreeSet<(Duration, Id, Timer, u64)>,
    epoch: u64,
    paint_dirty: bool,
    geometry_revision: u64,
    semantic_revision: u64,
    painted: u64,
    stale: u64,
    marker: PhantomData<fn(W) -> W>,
}
impl<W: Widget> Ui<W> {
    pub fn new(element: Element<W>, size: Size, limits: Limits) -> Result<Self, Error> {
        if !size.valid() {
            return Err(Error::InvalidGeometry);
        }
        if element.prepared.count > limits.nodes || element.prepared.count > limits.messages {
            return Err(Error::Full);
        }
        let root = element.prepared.id;
        let mut tree = Tree::default();
        let mut mounts = vec![];
        tree.insert(None, element.prepared, &mut mounts);
        let mut mailbox = Mailbox {
            items: VecDeque::new(),
            capacity: limits.messages,
            byte_limit: limits.message_bytes,
            reserved: 0,
            reserved_bytes: 0,
            reserved_nodes: 0,
            bytes: 0,
        };
        assert!(mailbox.reserve(mounts.len(), 0));
        for id in mounts {
            mailbox.push_reserved(Delivery::Mount(id), 0)
        }
        Ok(Self {
            tree,
            root,
            mailbox,
            limits,
            size,
            now: Duration::ZERO,
            last_frame: Duration::ZERO,
            focus: None,
            hover: None,
            captures: BTreeMap::new(),
            buttons: BTreeSet::new(),
            pressed: BTreeSet::new(),
            session: 0,
            window_focused: true,
            window_saved_focus: None,
            modals: vec![],
            frames: VecDeque::new(),
            frame_set: HashSet::new(),
            timers: BTreeMap::new(),
            timer_index: BTreeSet::new(),
            epoch: 0,
            paint_dirty: true,
            geometry_revision: 0,
            semantic_revision: 0,
            painted: 0,
            stale: 0,
            marker: PhantomData,
        })
    }
    pub fn root(&self) -> &W {
        self.tree
            .get(self.root)
            .unwrap()
            .widget
            .as_ref()
            .unwrap()
            .state()
            .downcast_ref()
            .unwrap()
    }
    pub fn send(&mut self, command: W::Command) -> Result<(), (Error, W::Command)> {
        let bytes = command.bytes();
        if !self.mailbox.reserve(1, bytes) {
            return Err((Error::Full, command));
        }
        self.mailbox
            .push_reserved(Delivery::Command(self.root, Box::new(command), None), bytes);
        Ok(())
    }
    pub fn post<C: Widget>(
        &mut self,
        ticket: Ticket<C>,
        command: C::Command,
    ) -> Result<(), (Error, C::Command)> {
        if !self.tree.live(ticket.owner)
            || self
                .tree
                .get(ticket.owner)
                .and_then(|n| n.tasks.get(&ticket.slot))
                != Some(&ticket.epoch)
        {
            return Err((Error::Stale, command));
        }
        let bytes = command.bytes();
        if !self.mailbox.reserve(1, bytes) {
            return Err((Error::Full, command));
        }
        self.mailbox.push_reserved(
            Delivery::Command(
                ticket.owner,
                Box::new(command),
                Some((ticket.slot.0, ticket.epoch)),
            ),
            bytes,
        );
        Ok(())
    }
    pub fn next_work(&self) -> Work {
        Work {
            ready: !self.mailbox.items.is_empty(),
            frame: !self.frame_set.is_empty(),
            deadline: self.timer_index.first().map(|(at, ..)| *at),
            paint: self.paint_dirty,
        }
    }
    pub fn stats(&self) -> Stats {
        Stats {
            nodes: self.tree.len(),
            queued: self.mailbox.items.len() + self.mailbox.reserved,
            queued_bytes: self.mailbox.bytes + self.mailbox.reserved_bytes,
            layouts: self.tree.layout_count,
            geometries: self.tree.geometry_count,
            painted: self.painted,
            stale: self.stale,
        }
    }
    pub fn session(&self) -> u64 {
        self.session
    }
    pub fn focused<C: Widget>(&self, child: Child<C>) -> bool {
        self.focus == Some(child.id)
    }
    pub fn captured<C: Widget>(&self, pointer: u32, child: Child<C>) -> bool {
        self.captures.get(&pointer) == Some(&child.id)
    }
    pub fn geometry<C: Widget>(&self, child: Child<C>) -> Option<Geometry> {
        self.tree.get(child.id).map(|n| n.geometry)
    }
    pub fn pressed(&self, physical: u32) -> bool {
        self.pressed.contains(&physical)
    }
    pub fn set_environment<T: 'static>(&mut self, value: Rc<T>, layout: bool) {
        self.environment(self.root, value, layout)
    }
    fn environment(&mut self, id: Id, value: Rc<dyn std::any::Any>, layout: bool) {
        if let Some(n) = self.tree.get_mut(id) {
            n.environment_boundary = true;
        }
        self.inherit_environment(id, value, layout);
    }
    fn inherit_environment(&mut self, id: Id, value: Rc<dyn std::any::Any>, layout: bool) {
        let children = self
            .tree
            .get(id)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        if let Some(n) = self.tree.get_mut(id) {
            n.environment = Some(value.clone());
            if layout {
                self.tree.invalidate(id);
            }
        }
        for child in children {
            if self
                .tree
                .get(child)
                .is_some_and(|n| !n.environment_boundary)
            {
                self.inherit_environment(child, value.clone(), layout);
            }
        }
        self.paint_dirty = true;
    }
    fn invoke(
        &mut self,
        id: Id,
        cleanup: bool,
        notifying: bool,
        call: impl FnOnce(&mut dyn Erased, &mut RawUpdate<'_>),
    ) -> bool {
        if !cleanup && !self.tree.live(id) {
            return false;
        }
        let Some(mut widget) = self.tree.get_mut(id).and_then(|n| n.widget.take()) else {
            return false;
        };
        let mut cx = RawUpdate {
            me: id,
            tree: &self.tree,
            mailbox: &mut self.mailbox,
            effects: Effects::new(),
            now: self.now,
            focused: self.focus == Some(id),
            cleanup,
            notifying,
            max_nodes: self.limits.nodes,
            inserted: 0,
        };
        call(widget.as_mut(), &mut cx);
        self.semantic_revision += 1;
        let effects = cx.effects;
        let stop = effects.stop;
        self.tree.get_mut(id).unwrap().widget = Some(widget);
        if notifying && !effects.mutations.is_empty() {
            self.mailbox
                .push_reserved(Delivery::Effects(id, effects), 0)
        } else {
            self.apply_effects(id, effects)
        }
        stop
    }
    fn notice(&mut self, id: Id, event: Lifecycle) {
        let cleanup = self.tree.get(id).is_some_and(|n| n.retiring);
        self.invoke(id, cleanup, true, |w, cx| w.lifecycle(cx, event));
    }
    fn apply_effects(&mut self, id: Id, mut effects: Effects) {
        if effects.layout {
            self.tree.invalidate(id)
        }
        if effects.repaint {
            self.paint_dirty = true
        }
        for mutation in effects.mutations.drain(..) {
            self.mutate(id, mutation)
        }
        if let Some(n) = self.tree.get_mut(id).filter(|n| !n.retiring) {
            for (slot, epoch) in effects.tasks {
                if let Some(epoch) = epoch {
                    n.tasks.insert(slot, epoch);
                } else {
                    n.tasks.remove(&slot);
                }
            }
        }
        if let Some(wanted) = effects.frame {
            if wanted && self.tree.active(id) {
                if self.frame_set.insert(id) {
                    self.frames.push_back(id)
                }
            } else {
                self.frame_set.remove(&id);
                self.frames.retain(|k| *k != id)
            }
        }
        for (timer, request) in effects.timers {
            self.cancel_timer(id, timer);
            if let Some((at, life)) = request {
                if self.tree.live(id) && (life == Lifetime::Mounted || self.tree.active(id)) {
                    self.epoch += 1;
                    self.tree.get_mut(id).unwrap().timers.insert(timer);
                    self.timers.insert((id, timer), (at, life, self.epoch));
                    self.timer_index.insert((at, id, timer, self.epoch));
                }
            }
        }
    }
    fn mutate(&mut self, owner: Id, mutation: Mutation) {
        if !self.tree.live(owner) {
            if let Mutation::Insert(p) = mutation {
                self.mailbox.release(p.count, 0);
                self.mailbox.reserved_nodes -= p.count;
            }
            return;
        }
        match mutation {
            Mutation::Insert(p) => {
                self.mailbox.reserved_nodes -= p.count;
                let id = p.id;
                let mut mounts = vec![];
                self.tree.insert(Some(owner), p, &mut mounts);
                self.tree.get_mut(owner).unwrap().children.push(id);
                for id in mounts {
                    self.mailbox.push_reserved(Delivery::Mount(id), 0)
                }
                self.tree.invalidate(owner);
                self.paint_dirty = true
            }
            Mutation::Remove(id) => {
                if self.tree.get(id).is_some_and(|n| n.parent == Some(owner)) {
                    self.retire(id)
                }
            }
            Mutation::Show(id, visible) => {
                if let Some(n) = self.tree.get_mut(id) {
                    n.visible = visible;
                    self.tree.invalidate(id);
                    self.paint_dirty = true;
                    self.reconcile()
                }
            }
            Mutation::Focus(id) => {
                if self.eligible(id) && self.window_focused {
                    self.change_focus(Some(id))
                }
            }
            Mutation::Capture(pointer, true) => {
                if self.eligible(owner) && self.window_focused {
                    if let Some(old) = self.captures.insert(pointer, owner).filter(|k| *k != owner)
                    {
                        self.notice(old, Lifecycle::CaptureLost(pointer))
                    }
                }
            }
            Mutation::Capture(pointer, false) => {
                if self.captures.get(&pointer) == Some(&owner) {
                    self.captures.remove(&pointer);
                    self.notice(owner, Lifecycle::CaptureLost(pointer))
                }
            }
            Mutation::Modal(Some(id)) => {
                if self.tree.active(id)
                    && self
                        .modals
                        .last()
                        .is_none_or(|(m, _)| self.tree.descendant(id, *m))
                {
                    self.modals.push((id, self.focus));
                    self.reconcile();
                    let next = self.first_focus(id);
                    self.change_focus(next)
                }
            }
            Mutation::Modal(None) => {
                if self.modals.last().is_some_and(|(id, _)| {
                    self.tree.descendant(*id, owner) || self.tree.descendant(owner, *id)
                }) {
                    if let Some((_, saved)) = self.modals.pop() {
                        self.reconcile();
                        self.change_focus(saved.filter(|id| self.eligible(*id)).or_else(|| {
                            self.modals.last().and_then(|(id, _)| self.first_focus(*id))
                        }))
                    }
                }
            }
            Mutation::Anchor(id, anchor) => {
                let mut next = anchor;
                let mut valid = true;
                let mut seen = HashSet::new();
                while let Some(a) = next {
                    if a == id || !seen.insert(a) || self.tree.descendant(a, id) {
                        valid = false;
                        break;
                    }
                    next = self.tree.get(a).and_then(|n| n.anchor)
                }
                if valid
                    && self
                        .tree
                        .get(id)
                        .is_some_and(|n| n.parent == Some(owner) && !n.retiring)
                {
                    self.tree.get_mut(id).unwrap().anchor = anchor;
                    self.tree.invalidate(id);
                    self.reconcile()
                }
            }
            Mutation::Environment(id, value, layout) => self.environment(id, value, layout),
        }
    }
    fn cancel_timer(&mut self, id: Id, timer: Timer) {
        if let Some(n) = self.tree.get_mut(id) {
            n.timers.remove(&timer);
        }
        if let Some((at, _, epoch)) = self.timers.remove(&(id, timer)) {
            self.timer_index.remove(&(at, id, timer, epoch));
        }
    }
    fn eligible(&self, id: Id) -> bool {
        self.tree.active(id)
            && self
                .modals
                .last()
                .is_none_or(|(m, _)| self.tree.descendant(id, *m))
    }
    fn first_focus(&self, id: Id) -> Option<Id> {
        let n = self.tree.get(id)?;
        if !self.eligible(id) {
            return None;
        }
        if n.widget.as_ref().is_some_and(|w| w.focusable()) {
            return Some(id);
        }
        n.children.iter().find_map(|k| self.first_focus(*k))
    }
    fn change_focus(&mut self, next: Option<Id>) {
        let next = next.filter(|id| {
            self.window_focused
                && self.eligible(*id)
                && self
                    .tree
                    .get(*id)
                    .and_then(|n| n.widget.as_ref())
                    .is_some_and(|w| w.focusable())
        });
        if next == self.focus {
            return;
        }
        let previous = std::mem::replace(&mut self.focus, next);
        self.session += 1;
        self.pressed.clear();
        if let Some(id) = previous {
            self.notice(id, Lifecycle::CancelKeys);
            self.notice(id, Lifecycle::Focus(false))
        }
        if let Some(id) = next {
            self.notice(id, Lifecycle::Focus(true))
        }
        self.paint_dirty = true
    }
    fn reconcile(&mut self) {
        while self
            .modals
            .last()
            .is_some_and(|(id, _)| !self.tree.active(*id))
        {
            let (_, saved) = self.modals.pop().unwrap();
            self.change_focus(
                saved
                    .filter(|id| self.eligible(*id))
                    .or_else(|| self.modals.last().and_then(|(id, _)| self.first_focus(*id))),
            );
        }
        if self.focus.is_some_and(|id| {
            !self.eligible(id)
                || !self
                    .tree
                    .get(id)
                    .and_then(|n| n.widget.as_ref())
                    .is_some_and(|w| w.focusable())
        }) {
            self.change_focus(None)
        }
        if self.hover.is_some_and(|id| !self.eligible(id)) {
            if let Some(id) = self.hover.take() {
                self.notice(id, Lifecycle::Hover(false));
                self.paint_dirty = true
            }
        }
        let captures: Vec<_> = self
            .captures
            .iter()
            .filter(|(_, id)| !self.eligible(**id))
            .map(|(p, id)| (*p, *id))
            .collect();
        for (p, id) in captures {
            self.captures.remove(&p);
            self.notice(id, Lifecycle::CaptureLost(p))
        }
        let dead: Vec<_> = self
            .frame_set
            .iter()
            .filter(|id| !self.tree.active(**id))
            .copied()
            .collect();
        for id in dead {
            self.frame_set.remove(&id);
        }
        self.frames.retain(|id| self.frame_set.contains(id));
        let dead: Vec<_> = self
            .timers
            .iter()
            .filter(|((id, _), (_, life, _))| {
                !self.tree.live(*id) || (*life == Lifetime::Visible && !self.tree.active(*id))
            })
            .map(|(k, _)| *k)
            .collect();
        for (id, timer) in dead {
            self.cancel_timer(id, timer)
        }
        let ids: Vec<_> = self.tree.ids().collect();
        for id in ids {
            let visible = self.tree.active(id);
            let anchor = self
                .tree
                .get(id)
                .and_then(|n| n.anchor)
                .map(|a| self.tree.active(a));
            let Some(n) = self.tree.get_mut(id) else {
                continue;
            };
            if !n.mounted || n.retiring {
                continue;
            }
            let previous = n.published.replace(visible);
            let previous_anchor = std::mem::replace(&mut n.published_anchor, anchor);
            if previous.is_some_and(|p| p != visible) {
                self.notice(id, Lifecycle::Visibility(visible))
            }
            if let (Some(old), Some(new)) = (previous_anchor, anchor) {
                if old != new {
                    self.notice(id, Lifecycle::Anchor(new))
                }
            }
        }
    }
    fn retire(&mut self, id: Id) {
        fn collect(tree: &Tree, id: Id, out: &mut Vec<Id>) {
            if let Some(n) = tree.get(id) {
                for child in &n.children {
                    collect(tree, *child, out)
                }
                out.push(id)
            }
        }
        let parent = self.tree.get(id).and_then(|n| n.parent);
        let mut ids = vec![];
        collect(&self.tree, id, &mut ids);
        for k in &ids {
            self.tree.get_mut(*k).unwrap().retiring = true;
        }
        self.reconcile();
        for k in &ids {
            if self.tree.get(*k).is_some_and(|n| n.mounted) {
                self.invoke(*k, true, false, |w, cx| w.lifecycle(cx, Lifecycle::Unmount));
            }
        }
        for k in ids {
            self.tree.discard(k)
        }
        if let Some(p) = parent {
            if let Some(n) = self.tree.get_mut(p) {
                n.children.retain(|k| *k != id);
                self.tree.invalidate(p)
            }
        }
        self.paint_dirty = true;
        self.reconcile();
    }
    pub fn pump(
        &mut self,
        budget: usize,
        mut output: impl FnMut(W::Output),
        mut platform: impl FnMut(HostRequest),
    ) -> usize {
        let mut done = 0;
        while done < budget {
            let Some((item, bytes)) = self.mailbox.pop() else {
                break;
            };
            done += 1;
            match item {
                Delivery::Mount(id) => {
                    if let Some(n) = self.tree.get_mut(id) {
                        n.mounted = true;
                        n.geometry_dirty = true;
                        self.tree.invalidate(id);
                        let visible = self.tree.active(id);
                        let anchor = self
                            .tree
                            .get(id)
                            .and_then(|n| n.anchor)
                            .map(|a| self.tree.active(a));
                        let n = self.tree.get_mut(id).unwrap();
                        n.published = Some(visible);
                        n.published_anchor = anchor;
                        self.invoke(id, false, false, |w, cx| w.lifecycle(cx, Lifecycle::Mount));
                        self.reconcile()
                    }
                }
                Delivery::Command(id, payload, ticket) => {
                    if !self.tree.live(id)
                        || ticket.is_some_and(|(slot, epoch)| {
                            self.tree.get(id).and_then(|n| n.tasks.get(&TaskSlot(slot)))
                                != Some(&epoch)
                        })
                    {
                        self.stale += 1;
                        continue;
                    }
                    if !self.tree.get(id).unwrap().mounted {
                        assert!(self.mailbox.reserve(1, bytes));
                        self.mailbox
                            .push_reserved(Delivery::Command(id, payload, ticket), bytes);
                        continue;
                    }
                    self.invoke(id, false, false, |w, cx| w.update(cx, payload));
                }
                Delivery::Output(id, payload) => {
                    let Some(n) = self.tree.get(id).filter(|n| !n.retiring) else {
                        self.stale += 1;
                        continue;
                    };
                    if let Some(parent) = n.parent {
                        if n.output.is_some() {
                            assert!(self.mailbox.reserve(1, bytes));
                            self.mailbox
                                .push_reserved(Delivery::Command(parent, payload, None), bytes)
                        }
                    } else {
                        output(
                            *payload
                                .downcast::<W::Output>()
                                .expect("private root output type invariant"),
                        )
                    }
                }
                Delivery::Effects(id, effects) => self.apply_effects(id, effects),
                Delivery::Clipboard(text) => platform(HostRequest::Copy(text)),
                Delivery::Paste(id) => {
                    if self.focus == Some(id) {
                        platform(HostRequest::Paste(PasteToken {
                            owner: id,
                            session: self.session,
                        }))
                    }
                }
            }
        }
        done
    }
    pub fn paste(&mut self, token: PasteToken, text: String) {
        if self.focus == Some(token.owner) && self.session == token.session {
            self.route(
                token.owner,
                &Input::Text {
                    session: token.session,
                    text,
                },
                true,
            );
        }
    }
    pub fn resize(&mut self, size: Size) {
        if size.valid() && size != self.size {
            self.size = size;
            self.tree.invalidate(self.root);
            self.paint_dirty = true
        }
    }
    pub fn layout(&mut self, text: &mut dyn TextEngine) {
        // Clean input and paint passes do not traverse the ownership tree.
        if self.tree.get(self.root).is_some_and(|n| {
            !n.dirty
                && !n.geometry_dirty
                && n.cache
                    .is_some_and(|(_, revision, _)| revision == text.revision())
        }) {
            return;
        }

        self.tree
            .measure(self.root, Constraints::tight(self.size), text);
        self.geometry_revision += 1;
        self.semantic_revision += 1;
        let viewport = Rect::from_size(self.size);
        self.tree.publish(
            self.root,
            Transform::IDENTITY,
            viewport,
            true,
            self.geometry_revision,
            false,
        );
        let overlays: Vec<_> = self
            .tree
            .ids()
            .filter(|id| self.tree.get(*id).is_some_and(|n| n.anchor.is_some()))
            .collect();
        fn publish_overlay(
            tree: &mut Tree,
            id: Id,
            viewport: Rect,
            revision: u64,
            done: &mut HashSet<Id>,
        ) {
            if !done.insert(id) {
                return;
            }
            let Some(anchor) = tree.get(id).and_then(|n| n.anchor) else {
                return;
            };
            // An anchor may live inside another overlay, so publish its placement ancestors first.
            let mut ancestor = Some(anchor);
            while let Some(a) = ancestor {
                if tree.get(a).is_some_and(|n| n.anchor.is_some()) {
                    publish_overlay(tree, a, viewport, revision, done);
                    break;
                }
                ancestor = tree.get(a).and_then(|n| n.parent);
            }
            let transform = tree
                .get(anchor)
                .map_or(Transform::IDENTITY, |n| n.geometry.transform);
            let visible = tree.active(id);
            tree.publish(id, transform, viewport, visible, revision, true);
        }
        let mut done = HashSet::new();
        for id in overlays {
            publish_overlay(
                &mut self.tree,
                id,
                viewport,
                self.geometry_revision,
                &mut done,
            )
        }
        let resized: Vec<_> = self
            .tree
            .ids()
            .filter(|id| self.tree.get(*id).is_some_and(|n| n.resized && n.mounted))
            .collect();
        for id in resized {
            self.tree.get_mut(id).unwrap().resized = false;
            self.notice(id, Lifecycle::Resized);
        }
        self.reconcile();
    }
    pub fn advance(&mut self, now: Duration, budget: usize) {
        assert!(now >= self.now);
        self.now = now;
        let due: Vec<_> = self
            .timer_index
            .iter()
            .take_while(|(at, ..)| *at <= now)
            .take(budget)
            .copied()
            .collect();
        for (at, id, timer, epoch) in due {
            if self
                .timers
                .get(&(id, timer))
                .is_none_or(|v| v.0 != at || v.2 != epoch)
            {
                continue;
            }
            self.cancel_timer(id, timer);
            if self.tree.get(id).is_some_and(|n| n.mounted && !n.retiring) {
                self.invoke(id, false, false, |w, cx| w.timer(cx, timer));
            }
        }
    }
    pub fn frame(&mut self, now: Duration, budget: usize) {
        assert!(now >= self.now);
        self.now = now;
        let time = FrameTime {
            now,
            elapsed: now.saturating_sub(self.last_frame),
        };
        self.last_frame = now;
        let count = self.frames.len().min(budget);
        let selected: Vec<_> = self.frames.drain(..count).collect();
        for id in selected {
            if self.frame_set.remove(&id) && self.tree.active(id) {
                self.invoke(id, false, false, |w, cx| w.frame(cx, time));
            }
        }
    }
    pub fn window_visible(&mut self, visible: bool) {
        if self
            .tree
            .get(self.root)
            .is_none_or(|n| n.visible == visible)
        {
            return;
        }
        if !visible && self.focus.is_some() {
            self.window_saved_focus = self.focus;
        }
        self.tree.get_mut(self.root).unwrap().visible = visible;
        self.tree.invalidate(self.root);
        self.reconcile();
        if visible {
            let saved = self.window_saved_focus.take();
            self.change_focus(saved);
            self.repaint();
        }
    }
    pub fn window_focus(&mut self, focused: bool) {
        self.window_focused = focused;
        if focused {
            let saved = self.window_saved_focus.take();
            self.change_focus(saved.or(self.focus));
        }

        if !focused {
            if self.focus.is_some() {
                self.window_saved_focus = self.focus;
            }
            self.pressed.clear();
            self.change_focus(None);
            self.buttons.clear();
            let captures = std::mem::take(&mut self.captures);
            for (p, id) in captures {
                self.notice(id, Lifecycle::CaptureLost(p))
            }
        }
    }
    fn hit(&self, id: Id, p: Point) -> Option<Id> {
        let n = self.tree.get(id)?;
        if !self.eligible(id) {
            return None;
        }
        let local = n.geometry.transform.inverse()?.point(p);
        if n.clip && !Rect::from_size(n.size).contains(local) {
            return None;
        }
        for child in n.children.iter().rev() {
            if self.tree.get(*child).is_some_and(|n| n.anchor.is_none()) {
                if let Some(hit) = self.hit(*child, p) {
                    return Some(hit);
                }
            }
        }
        Rect::from_size(n.size).contains(local).then_some(id)
    }
    pub fn dispatch(&mut self, input: Input, text: &mut dyn TextEngine) {
        self.layout(text);
        if !self.window_focused {
            return;
        }
        if let Input::Text { session, .. } | Input::Preedit { session, .. } = &input {
            if *session != self.session {
                return;
            }
        }
        if let Input::Key { physical, down, .. } = &input {
            if *down {
                self.pressed.insert(*physical);
            } else {
                self.pressed.remove(physical);
            }
        }
        if let Input::Button {
            pointer,
            button,
            down: true,
            ..
        } = &input
        {
            self.buttons.insert((*pointer, *button));
        }
        let target = if let Some(position) = input.position() {
            let captured = match input {
                Input::Pointer { pointer, .. } | Input::Button { pointer, .. } => {
                    self.captures.get(&pointer).copied()
                }
                _ => None,
            };
            captured.or_else(|| {
                let mut overlays: Vec<_> = self
                    .tree
                    .ids()
                    .filter(|id| self.tree.get(*id).is_some_and(|n| n.anchor.is_some()))
                    .collect();
                overlays.sort();
                overlays
                    .iter()
                    .rev()
                    .find_map(|id| self.hit(*id, position))
                    .or_else(|| {
                        self.hit(
                            self.modals.last().map_or(self.root, |(id, _)| *id),
                            position,
                        )
                    })
            })
        } else {
            self.focus.or(self.modals.last().map(|(id, _)| *id))
        };
        if matches!(input, Input::Pointer { .. }) && self.hover != target {
            if let Some(old) = self.hover {
                self.notice(old, Lifecycle::Hover(false))
            }
            self.hover = target;
            if let Some(id) = target {
                self.notice(id, Lifecycle::Hover(true))
            }
            self.paint_dirty = true
        }
        let handled = target.is_some_and(|id| {
            self.route(
                id,
                &input,
                matches!(input, Input::Text { .. } | Input::Preedit { .. }),
            )
        });
        if !handled {
            if let Input::Key {
                key: Key::Tab,
                down: true,
                modifiers,
                ..
            } = &input
            {
                self.cycle_focus(modifiers.shift)
            }
        }
        if let Input::Button {
            pointer,
            button,
            down: false,
            ..
        } = input
        {
            self.buttons.remove(&(pointer, button));
            if !self.buttons.iter().any(|(p, _)| *p == pointer) {
                if let Some(id) = self.captures.remove(&pointer) {
                    self.notice(id, Lifecycle::CaptureLost(pointer))
                }
            }
        }
    }
    fn route(&mut self, target: Id, input: &Input, target_only: bool) -> bool {
        let mut path = vec![];
        let mut next = Some(target);
        while let Some(id) = next {
            path.push(id);
            next = self.tree.get(id).and_then(|n| n.parent)
        }
        let session = self.session;
        let keyboard = matches!(
            input,
            Input::Key { .. } | Input::Text { .. } | Input::Preedit { .. }
        );
        let mut route = vec![];
        if !target_only {
            route.extend(path.iter().skip(1).rev().map(|id| (*id, Phase::Preview)))
        }
        route.push((target, Phase::Target));
        if !target_only {
            route.extend(path.iter().skip(1).map(|id| (*id, Phase::Bubble)))
        }
        for (id, phase) in route {
            if keyboard && self.session != session {
                return true;
            }
            if !self.eligible(id) {
                continue;
            }
            let Some(transform) = self
                .tree
                .get(id)
                .and_then(|n| n.geometry.transform.inverse())
            else {
                continue;
            };
            let input = input.local(transform);
            if self.invoke(id, false, false, |w, cx| w.input(cx, phase, &input)) {
                return true;
            }
        }
        false
    }
    fn cycle_focus(&mut self, reverse: bool) {
        let mut order: Vec<_> = self
            .tree
            .ids()
            .filter(|id| {
                self.eligible(*id)
                    && self
                        .tree
                        .get(*id)
                        .and_then(|n| n.widget.as_ref())
                        .is_some_and(|w| w.focusable())
            })
            .collect();
        order.sort();
        if order.is_empty() {
            return;
        }
        let i = order.iter().position(|id| Some(*id) == self.focus);
        let next = match (i, reverse) {
            (Some(i), false) => (i + 1) % order.len(),
            (Some(i), true) => (i + order.len() - 1) % order.len(),
            (None, false) => 0,
            (None, true) => order.len() - 1,
        };
        self.change_focus(Some(order[next]));
    }
    pub fn repaint(&mut self) {
        self.paint_dirty = true
    }
    pub fn paint(&mut self, painter: &mut dyn Painter) {
        fn paint(
            tree: &Tree,
            id: Id,
            painter: &mut dyn Painter,
            focus: Option<Id>,
            hover: Option<Id>,
            absolute: bool,
            count: &mut u64,
        ) {
            let Some(n) = tree.get(id) else { return };
            if !n.geometry.visible || n.geometry.clip.width <= 0. || n.geometry.clip.height <= 0. {
                return;
            }
            painter.save();
            painter.transform(if absolute {
                n.geometry.transform
            } else {
                n.local
            });
            if n.clip {
                painter.clip(Rect::from_size(n.size))
            }
            if let Some(widget) = &n.widget {
                widget.paint(&mut Paint {
                    bounds: Rect::from_size(n.size),
                    focused: focus == Some(id),
                    hovered: hover == Some(id),
                    painter,
                    environment: n.environment.as_deref(),
                });
                *count += 1;
            }
            for child in &n.children {
                if tree.get(*child).is_some_and(|n| n.anchor.is_none()) {
                    paint(tree, *child, painter, focus, hover, false, count)
                }
            }
            painter.restore();
        }
        paint(
            &self.tree,
            self.root,
            painter,
            self.focus,
            self.hover,
            false,
            &mut self.painted,
        );
        let mut overlays: Vec<_> = self
            .tree
            .ids()
            .filter(|id| self.tree.get(*id).is_some_and(|n| n.anchor.is_some()))
            .collect();
        overlays.sort();
        for id in overlays {
            paint(
                &self.tree,
                id,
                painter,
                self.focus,
                self.hover,
                true,
                &mut self.painted,
            )
        }
        self.paint_dirty = false;
    }
    pub fn semantic_revision(&self) -> u64 {
        self.semantic_revision
    }
    pub fn semantics(&self) -> Vec<SemanticNode> {
        let mut ids: Vec<_> = self.tree.ids().collect();
        ids.sort();
        ids.into_iter()
            .filter(|id| self.eligible(*id))
            .filter_map(|id| {
                let n = self.tree.get(id)?;
                Some(SemanticNode {
                    id: id.0,
                    parent: n.parent.map(|p| p.0),
                    bounds: n.geometry.bounds.intersect(n.geometry.clip),
                    focused: self.focus == Some(id),
                    focusable: n.widget.as_ref()?.focusable(),
                    semantics: n.widget.as_ref()?.semantics(),
                })
            })
            .collect()
    }
    /// The focused editor's published caret rectangle, without allocating a semantic tree.
    pub fn ime_cursor(&self) -> Option<Rect> {
        let n = self.tree.get(self.focus?)?;
        let caret = n.widget.as_ref()?.ime_cursor()?;
        Some(n.geometry.transform.rect(caret).intersect(n.geometry.clip))
    }
    pub fn accessibility(&mut self, id: u64, action: SemanticAction) {
        let id = Id(id);
        if self.eligible(id) {
            self.invoke(id, false, false, |w, cx| w.accessibility(cx, action));
        }
    }
}

impl<W: Widget> Drop for Ui<W> {
    fn drop(&mut self) {
        // Closing a host has the same cancellation and lifecycle guarantees as subtree removal.
        self.retire(self.root);
    }
}
