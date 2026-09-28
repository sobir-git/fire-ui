use crate::{
    runtime::{Delivery, Mailbox},
    tree::Tree,
    widget::{identity, Id, OutputMap, Prepared},
    *,
};
use std::{any::Any, collections::BTreeMap, marker::PhantomData, rc::Rc, time::Duration};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Unsupported,
    Full,
    Stale,
    NotOwned,
    InvalidGeometry,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timer(pub(crate) Id);

/// Where an overlay is placed, or `None` for an ordinary child.
pub enum Anchor {
    /// Not an overlay: laid out and clipped by its owner like any other child.
    None,
    /// Positioned against the window's origin and clipped only by the window.
    Window,
    /// Positioned against another child, and clipped only by the window.
    To(LayoutChild),
}
impl Anchor {
    /// Anchor an overlay to a child owned by the same widget.
    pub fn to<A: Widget>(child: Child<A>) -> Self {
        Self::To(child.into())
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}
impl Timer {
    pub fn new() -> Self {
        Self(identity())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifetime {
    Visible,
    Mounted,
}
pub(crate) enum Mutation {
    Remove(Id),
    Show(Id, bool),
    Insert(Prepared),
    Focus(Id),
    Capture(u32, bool),
    Modal(Option<Id>),
    Anchor(Id, Option<Option<Id>>),
    Environment(Id, std::any::TypeId, Rc<dyn Any>, bool),
    /// A command for a child inserted by the same callback, kept in callback order.
    Send(Id, crate::widget::Payload, usize),
}
pub(crate) struct Effects {
    pub tasks: BTreeMap<TaskSlot, Option<u64>>,
    pub mutations: Vec<Mutation>,
    pub frame: Option<bool>,
    pub timers: BTreeMap<Timer, Option<(Duration, Lifetime)>>,
    pub repaint: bool,
    pub damage: Option<Rect>,
    pub layout: bool,
    pub semantics: bool,
    pub emitted: bool,
    pub stop: bool,
    /// Children inserted by this callback, usable before the insertion commits.
    pub pending: Vec<Id>,
    /// A mailbox slot is held to run the non-insert mutations after new mounts.
    pub split: bool,
}
impl Effects {
    pub fn new() -> Self {
        Self {
            tasks: BTreeMap::new(),
            mutations: vec![],
            frame: None,
            timers: BTreeMap::new(),
            repaint: false,
            damage: None,
            layout: false,
            semantics: false,
            emitted: false,
            stop: false,
            pending: vec![],
            split: false,
        }
    }
}
pub(crate) struct RawUpdate<'a> {
    pub me: Id,
    pub tree: &'a Tree,
    pub mailbox: &'a mut Mailbox,
    pub effects: Effects,
    pub now: Duration,
    pub focused: bool,
    pub cleanup: bool,
    pub notifying: bool,
    pub limits: Limits,
    pub inserted: usize,
}
impl RawUpdate<'_> {
    pub fn typed<W: Widget>(&mut self) -> Update<'_, W> {
        Update {
            raw: self,
            marker: PhantomData,
        }
    }
}
// Two lifetimes keep the short callback borrow separate from the runtime borrow.
pub struct Update<'a, W: Widget> {
    raw: &'a mut dyn UpdateAccess,
    marker: PhantomData<fn(W) -> W>,
}
trait UpdateAccess {
    fn me(&self) -> Id;
    fn tree(&self) -> &Tree;
    fn mailbox(&mut self) -> &mut Mailbox;
    fn effects(&mut self) -> &mut Effects;
    fn now(&self) -> Duration;
    fn focused(&self) -> bool;
    fn cleanup(&self) -> bool;
    fn pending(&self, id: Id) -> bool;
    fn limits(&self) -> Limits;
    fn mutate(&mut self, mutation: Mutation) -> Result<(), (Error, Mutation)>;
}
impl UpdateAccess for RawUpdate<'_> {
    fn me(&self) -> Id {
        self.me
    }
    fn tree(&self) -> &Tree {
        self.tree
    }
    fn mailbox(&mut self) -> &mut Mailbox {
        self.mailbox
    }
    fn effects(&mut self) -> &mut Effects {
        &mut self.effects
    }
    fn now(&self) -> Duration {
        self.now
    }
    fn focused(&self) -> bool {
        self.focused
    }
    fn cleanup(&self) -> bool {
        self.cleanup
    }
    fn pending(&self, id: Id) -> bool {
        self.effects.pending.contains(&id)
    }
    fn limits(&self) -> Limits {
        self.limits
    }
    fn mutate(&mut self, mutation: Mutation) -> Result<(), (Error, Mutation)> {
        if self.cleanup {
            return Err((Error::Stale, mutation));
        }
        if self.effects.mutations.len() >= self.limits.mutations_per_callback {
            return Err((Error::Full, mutation));
        }
        let (mounts, bytes) = match &mutation {
            Mutation::Insert(p) => (p.count, 0),
            Mutation::Send(_, _, bytes) => (0, *bytes),
            _ => (0, 0),
        };
        if self.tree.len() + self.mailbox.reserved_nodes + mounts > self.limits.nodes {
            return Err((Error::Full, mutation));
        }
        let deferred = usize::from(self.notifying && self.effects.mutations.is_empty());
        // Inserts commit first; everything else waits for the new children to mount.
        let inserting = matches!(mutation, Mutation::Insert(_));
        let split = !self.effects.split
            && self
                .effects
                .mutations
                .iter()
                .any(|m| matches!(m, Mutation::Insert(_)) != inserting);
        let sends = usize::from(matches!(mutation, Mutation::Send(..)));
        if !self
            .mailbox
            .reserve(mounts + deferred + usize::from(split) + sends, bytes)
        {
            return Err((Error::Full, mutation));
        }
        self.effects.split |= split;
        self.inserted += mounts;
        self.mailbox.reserved_nodes += mounts;
        self.effects.mutations.push(mutation);
        Ok(())
    }
}
impl<W: Widget> Update<'_, W> {
    pub fn now(&self) -> Duration {
        self.raw.now()
    }
    pub fn focused(&self) -> bool {
        self.raw.focused()
    }
    pub fn bounds(&self) -> Rect {
        Rect::from_size(self.raw.tree().get(self.raw.me()).unwrap().size)
    }
    /// This widget's rectangle in window coordinates, from the last published
    /// geometry. Popovers and drag feedback need it to position themselves against
    /// the window rather than against their owner.
    pub fn window_bounds(&self) -> Rect {
        self.raw
            .tree()
            .get(self.raw.me())
            .map(|n| n.geometry.bounds)
            .unwrap_or_default()
    }
    pub fn environment<T: Any>(&self) -> Option<&T> {
        self.raw.tree().get(self.raw.me())?.environment.get::<T>()
    }
    fn owned<C: Widget>(&self, child: Child<C>) -> Result<(), Error> {
        self.owned_id(child.id)
    }
    fn owned_id(&self, id: Id) -> Result<(), Error> {
        match self.raw.tree().get(id) {
            Some(n) if n.retiring => Err(Error::Stale),
            Some(n) if n.parent == Some(self.raw.me()) => Ok(()),
            Some(_) => Err(Error::NotOwned),
            None if self.raw.pending(id) => Ok(()),
            None => Err(Error::Stale),
        }
    }
    pub fn send<C: Widget>(
        &mut self,
        child: Child<C>,
        command: C::Command,
    ) -> Result<(), (Error, C::Command)> {
        if let Err(e) = self.owned(child) {
            return Err((e, command));
        }
        if self.raw.cleanup() {
            return Err((Error::Stale, command));
        }
        let bytes = command.bytes();
        if self.raw.pending(child.id) {
            return match self
                .raw
                .mutate(Mutation::Send(child.id, Box::new(command), bytes))
            {
                Ok(()) => Ok(()),
                Err((e, Mutation::Send(_, payload, _))) => Err((
                    e,
                    *payload
                        .downcast()
                        .expect("private pending command type invariant"),
                )),
                Err(_) => unreachable!(),
            };
        }
        if !self.raw.mailbox().reserve(1, bytes) {
            return Err((Error::Full, command));
        }
        self.raw
            .mailbox()
            .push_reserved(Delivery::Command(child.id, Box::new(command), None), bytes);
        Ok(())
    }
    pub fn emit(&mut self, output: W::Output) -> Result<(), (Error, W::Output)> {
        if self.raw.cleanup() {
            return Err((Error::Stale, output));
        }
        let mut id = self.raw.me();
        let mut bytes = output.bytes();
        let mut mapped = None;
        // A bubble can pass through decorators before its owner's map expands
        // the payload. Reserve that final cost while the original is still owned.
        loop {
            let Some(node) = self.raw.tree().get(id) else {
                break;
            };
            match node.output.as_ref() {
                Some(OutputMap::Map(map)) => {
                    let (payload, cost) = map(&output);
                    mapped = Some(payload);
                    bytes = cost;
                    break;
                }
                Some(OutputMap::Bubble) => {
                    let Some(parent) = node.parent else { break };
                    id = parent;
                }
                Some(OutputMap::Forward) | None => break,
            }
        }
        if !self.raw.mailbox().reserve(1, bytes) {
            return Err((Error::Full, output));
        }
        let payload = mapped.unwrap_or_else(|| Box::new(output) as crate::widget::Payload);
        self.raw
            .mailbox()
            .push_reserved(Delivery::Output(id, payload), bytes);
        self.raw.effects().emitted = true;
        Ok(())
    }
    pub fn remove<C: Widget>(&mut self, child: Child<C>) -> Result<(), Error> {
        self.owned(child)?;
        self.raw
            .mutate(Mutation::Remove(child.id))
            .map_err(|(e, _)| e)
    }
    pub fn show<C: Widget>(&mut self, child: Child<C>, show: bool) -> Result<(), Error> {
        self.owned(child)?;
        self.raw
            .mutate(Mutation::Show(child.id, show))
            .map_err(|(e, _)| e)
    }
    /// Insert an ordinary child; see [`Update::insert_at`].
    pub fn insert<C: Widget>(
        &mut self,
        element: Element<C>,
        map: impl Fn(&C::Output) -> W::Command + 'static,
    ) -> Result<Child<C>, (Error, Element<C>)> {
        self.insert_at(element, Anchor::None, map)
    }
    /// Insert a child, returning a handle usable immediately.
    ///
    /// The insertion commits when the callback returns. Commands and structural
    /// requests naming the new child in the same callback run after it mounts, so an
    /// owner can insert a popover, make it modal and focus it in one step.
    ///
    /// `anchor` makes the child an overlay from the moment it exists, which is how a
    /// menu, popover or tooltip escapes the clipping of the panel that opened it.
    pub fn insert_at<C: Widget>(
        &mut self,
        element: Element<C>,
        anchor: Anchor,
        map: impl Fn(&C::Output) -> W::Command + 'static,
    ) -> Result<Child<C>, (Error, Element<C>)> {
        let anchor = match anchor {
            Anchor::None => None,
            Anchor::Window => Some(None),
            Anchor::To(a) => {
                if let Err(e) = self.owned_id(a.0) {
                    return Err((e, element));
                }
                Some(Some(a.0))
            }
        };
        let child = Child::new(element.prepared.id);
        let mut prepared = element.prepared;
        prepared.anchor = anchor;
        prepared.output = Some(OutputMap::Map(Box::new(move |p| {
            let command = map(p
                .downcast_ref::<C::Output>()
                .expect("private inserted output invariant"));
            let bytes = command.bytes();
            (Box::new(command), bytes)
        })));
        match self.raw.mutate(Mutation::Insert(prepared)) {
            Ok(()) => {
                self.raw.effects().pending.push(child.id);
                Ok(child)
            }
            Err((e, Mutation::Insert(p))) => Err((e, Element::from_prepared(p))),
            _ => unreachable!(),
        }
    }
    pub fn focus(&mut self) -> Result<(), Error> {
        let me = self.raw.me();
        self.raw.mutate(Mutation::Focus(me)).map_err(|(e, _)| e)
    }
    pub fn focus_child<C: Widget>(&mut self, child: Child<C>) -> Result<(), Error> {
        self.owned(child)?;
        self.raw
            .mutate(Mutation::Focus(child.id))
            .map_err(|(e, _)| e)
    }
    pub fn capture(&mut self, pointer: u32) -> Result<(), Error> {
        self.raw
            .mutate(Mutation::Capture(pointer, true))
            .map_err(|(e, _)| e)
    }
    pub fn release(&mut self, pointer: u32) -> Result<(), Error> {
        self.raw
            .mutate(Mutation::Capture(pointer, false))
            .map_err(|(e, _)| e)
    }
    pub fn open_modal<C: Widget>(&mut self, child: Child<C>) -> Result<(), Error> {
        self.owned(child)?;
        self.raw
            .mutate(Mutation::Modal(Some(child.id)))
            .map_err(|(e, _)| e)
    }
    pub fn close_modal(&mut self) -> Result<(), Error> {
        self.raw.mutate(Mutation::Modal(None)).map_err(|(e, _)| e)
    }
    /// Lift a child out of the ordinary layout into an overlay, or put it back.
    ///
    /// An overlay is placed against its anchor rather than against this widget, and
    /// is clipped by the window instead of by any ancestor. That is what lets a menu
    /// or a popover leave the scrolling panel that opened it.
    pub fn anchor<C: Widget>(&mut self, child: Child<C>, anchor: Anchor) -> Result<(), Error> {
        self.owned(child)?;
        let anchor = match anchor {
            Anchor::None => None,
            Anchor::Window => Some(None),
            Anchor::To(a) => {
                self.owned_id(a.0)?;
                if a.0 == child.id {
                    return Err(Error::InvalidGeometry);
                }
                Some(Some(a.0))
            }
        };
        self.raw
            .mutate(Mutation::Anchor(child.id, anchor))
            .map_err(|(e, _)| e)
    }
    /// Publish an ambient value of type `T` for a child and everything beneath it.
    ///
    /// Only `T` is replaced: a child can hold a value from its owner and still read
    /// every other type its ancestors installed. Inheritance of `T` alone stops at
    /// that child, so an ancestor changing `T` later will not overwrite this.
    pub fn set_environment<C: Widget, T: Any>(
        &mut self,
        child: Child<C>,
        value: Rc<T>,
        layout: bool,
    ) -> Result<(), Error> {
        self.owned(child)?;
        self.raw
            .mutate(Mutation::Environment(
                child.id,
                std::any::TypeId::of::<T>(),
                value,
                layout,
            ))
            .map_err(|(e, _)| e)
    }
    pub fn replace_task(&mut self, slot: TaskSlot) -> Result<Ticket<W>, Error> {
        if self.raw.cleanup() {
            return Err(Error::Stale);
        }
        let mut live = self.raw.tree().get(self.raw.me()).unwrap().tasks.clone();
        for (slot, epoch) in &self.raw.effects().tasks {
            if let Some(epoch) = epoch {
                live.insert(*slot, *epoch);
            } else {
                live.remove(slot);
            }
        }
        let limits = self.raw.limits();
        if self.raw.effects().tasks.len() >= limits.task_changes_per_callback
            || (live.len() >= limits.tasks_per_node && !live.contains_key(&slot))
        {
            return Err(Error::Full);
        }
        let epoch = identity().0;
        let ticket = Ticket::new(self.raw.me(), slot, epoch);
        self.raw.effects().tasks.insert(slot, Some(epoch));
        Ok(ticket)
    }
    pub fn cancel_task(&mut self, slot: TaskSlot) {
        if self
            .raw
            .tree()
            .get(self.raw.me())
            .is_some_and(|n| n.tasks.contains_key(&slot))
            || self.raw.effects().tasks.contains_key(&slot)
        {
            self.raw.effects().tasks.insert(slot, None);
        }
    }
    pub fn repaint(&mut self) {
        let effects = self.raw.effects();
        effects.repaint = true;
        effects.damage = None;
    }
    /// Invalidate local pixels, including the old and new extent of anything that moved.
    pub fn repaint_rect(&mut self, rect: Rect) {
        if !rect.x.is_finite() || !rect.y.is_finite() || !rect.size().valid() {
            return;
        }
        let effects = self.raw.effects();
        effects.damage = if effects.repaint {
            effects.damage.map(|old| old.union(rect))
        } else {
            Some(rect)
        };
        effects.repaint = true;
    }
    pub fn relayout(&mut self) {
        self.raw.effects().layout = true;
        self.repaint()
    }
    /// Publish a semantic change from a timer or frame callback that only repaints.
    /// Other callbacks automatically publish semantics when they request paint,
    /// layout, a mutation, or emit an output.
    pub fn semantics_changed(&mut self) {
        self.raw.effects().semantics = true;
    }
    pub fn stop(&mut self) {
        self.raw.effects().stop = true
    }
    pub fn request_frame(&mut self) {
        if !self.raw.cleanup() {
            self.raw.effects().frame = Some(true)
        }
    }
    pub fn cancel_frame(&mut self) {
        self.raw.effects().frame = Some(false)
    }
    pub fn after(&mut self, timer: Timer, delay: Duration) -> Result<(), Error> {
        self.after_with_lifetime(timer, delay, Lifetime::Visible)
    }
    pub fn after_with_lifetime(
        &mut self,
        timer: Timer,
        delay: Duration,
        life: Lifetime,
    ) -> Result<(), Error> {
        if self.raw.cleanup() {
            return Err(Error::Stale);
        }
        let mut live = self.raw.tree().get(self.raw.me()).unwrap().timers.clone();
        for (t, request) in &self.raw.effects().timers {
            if request.is_some() {
                live.insert(*t);
            } else {
                live.remove(t);
            }
        }
        let limits = self.raw.limits();
        if self.raw.effects().timers.len() >= limits.timer_changes_per_callback
            || (live.len() >= limits.timers_per_node && !live.contains(&timer))
        {
            return Err(Error::Full);
        }
        let at = self.now().saturating_add(delay);
        self.raw.effects().timers.insert(timer, Some((at, life)));
        Ok(())
    }
    pub fn cancel_timer(&mut self, timer: Timer) {
        if self
            .raw
            .tree()
            .get(self.raw.me())
            .is_some_and(|n| n.timers.contains(&timer))
            || self.raw.effects().timers.contains_key(&timer)
        {
            self.raw.effects().timers.insert(timer, None);
        }
    }
    pub fn window(&mut self, action: crate::WindowAction) -> Result<(), Error> {
        if self.raw.cleanup() {
            return Err(Error::Stale);
        }
        if self
            .raw
            .tree()
            .get(self.raw.me())
            .is_some_and(|n| n.parent.is_some())
        {
            return Err(Error::NotOwned);
        }
        if !self.raw.mailbox().reserve(1, 0) {
            return Err(Error::Full);
        }
        self.raw
            .mailbox()
            .push_reserved(Delivery::Window(action), 0);
        Ok(())
    }
    pub fn close_window(&mut self) -> Result<(), Error> {
        if self.raw.cleanup() {
            return Err(Error::Stale);
        }
        if self
            .raw
            .tree()
            .get(self.raw.me())
            .is_some_and(|n| n.parent.is_some())
        {
            return Err(Error::NotOwned);
        }
        if !self.raw.mailbox().reserve(1, 0) {
            return Err(Error::Full);
        }
        self.raw.mailbox().push_reserved(Delivery::Close, 0);
        Ok(())
    }
    pub fn copy(&mut self, text: String) -> Result<(), (Error, String)> {
        if !self.raw.mailbox().clipboard_enabled {
            return Err((Error::Unsupported, text));
        }
        if self.raw.cleanup() {
            return Err((Error::Stale, text));
        }
        let bytes = text.capacity();
        if !self.raw.mailbox().reserve(1, bytes) {
            return Err((Error::Full, text));
        }
        self.raw
            .mailbox()
            .push_reserved(Delivery::Clipboard(text), bytes);
        Ok(())
    }
    pub fn paste(&mut self) -> Result<(), Error> {
        if !self.raw.mailbox().clipboard_enabled {
            return Err(Error::Unsupported);
        }
        if self.raw.cleanup() {
            return Err(Error::Stale);
        }
        if !self.raw.mailbox().reserve(1, 0) {
            return Err(Error::Full);
        }
        let me = self.raw.me();
        self.raw.mailbox().push_reserved(Delivery::Paste(me), 0);
        Ok(())
    }
}
