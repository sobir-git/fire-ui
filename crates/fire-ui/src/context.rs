use crate::{
    runtime::{Delivery, Mailbox},
    tree::Tree,
    widget::{identity, Id, OutputMap, Prepared},
    *,
};
use std::{any::Any, collections::BTreeMap, marker::PhantomData, rc::Rc, time::Duration};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Full,
    Stale,
    NotOwned,
    InvalidGeometry,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timer(pub(crate) Id);
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
    Anchor(Id, Option<Id>),
    Environment(Id, Rc<dyn Any>, bool),
}
pub(crate) struct Effects {
    pub tasks: BTreeMap<TaskSlot, Option<u64>>,
    pub mutations: Vec<Mutation>,
    pub frame: Option<bool>,
    pub timers: BTreeMap<Timer, Option<(Duration, Lifetime)>>,
    pub repaint: bool,
    pub layout: bool,
    pub stop: bool,
}
impl Effects {
    pub fn new() -> Self {
        Self {
            tasks: BTreeMap::new(),
            mutations: vec![],
            frame: None,
            timers: BTreeMap::new(),
            repaint: false,
            layout: false,
            stop: false,
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
    pub max_nodes: usize,
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
    fn mutate(&mut self, mutation: Mutation) -> Result<(), (Error, Mutation)> {
        if self.cleanup {
            return Err((Error::Stale, mutation));
        }
        if self.effects.mutations.len() >= 64 {
            return Err((Error::Full, mutation));
        }
        let mounts = if let Mutation::Insert(p) = &mutation {
            p.count
        } else {
            0
        };
        if self.tree.len() + self.mailbox.reserved_nodes + mounts > self.max_nodes {
            return Err((Error::Full, mutation));
        }
        let deferred = usize::from(self.notifying && self.effects.mutations.is_empty());
        if !self.mailbox.reserve(mounts + deferred, 0) {
            return Err((Error::Full, mutation));
        }
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
    pub fn environment<T: Any>(&self) -> Option<&T> {
        self.raw
            .tree()
            .get(self.raw.me())?
            .environment
            .as_ref()?
            .downcast_ref()
    }
    fn owned<C: Widget>(&self, child: Child<C>) -> Result<(), Error> {
        match self.raw.tree().get(child.id) {
            Some(n) if n.retiring => Err(Error::Stale),
            Some(n) if n.parent == Some(self.raw.me()) => Ok(()),
            Some(_) => Err(Error::NotOwned),
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
        let me = self.raw.me();
        let mapped = self
            .raw
            .tree()
            .get(me)
            .and_then(|n| n.output.as_ref())
            .and_then(|map| match map {
                OutputMap::Map(map) => Some(map(&output)),
                OutputMap::Forward => None,
            });
        let bytes = mapped
            .as_ref()
            .map_or_else(|| output.bytes(), |(_, bytes)| *bytes);
        if !self.raw.mailbox().reserve(1, bytes) {
            return Err((Error::Full, output));
        }
        let payload = mapped.map_or_else(
            || Box::new(output) as crate::widget::Payload,
            |(payload, _)| payload,
        );
        self.raw
            .mailbox()
            .push_reserved(Delivery::Output(me, payload), bytes);
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
    pub fn insert<C: Widget>(
        &mut self,
        element: Element<C>,
        map: impl Fn(&C::Output) -> W::Command + 'static,
    ) -> Result<Child<C>, (Error, Element<C>)> {
        let child = Child::new(element.prepared.id);
        let mut prepared = element.prepared;
        prepared.output = Some(OutputMap::Map(Box::new(move |p| {
            let command = map(p
                .downcast_ref::<C::Output>()
                .expect("private inserted output invariant"));
            let bytes = command.bytes();
            (Box::new(command), bytes)
        })));
        match self.raw.mutate(Mutation::Insert(prepared)) {
            Ok(()) => Ok(child),
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
    pub fn anchor<C: Widget, A: Widget>(
        &mut self,
        child: Child<C>,
        anchor: Option<Child<A>>,
    ) -> Result<(), Error> {
        self.owned(child)?;
        if let Some(a) = anchor {
            self.owned(a)?;
            if a.id == child.id {
                return Err(Error::InvalidGeometry);
            }
        }
        self.raw
            .mutate(Mutation::Anchor(child.id, anchor.map(|a| a.id)))
            .map_err(|(e, _)| e)
    }
    pub fn set_environment<C: Widget, T: Any>(
        &mut self,
        child: Child<C>,
        value: Rc<T>,
        layout: bool,
    ) -> Result<(), Error> {
        self.owned(child)?;
        self.raw
            .mutate(Mutation::Environment(child.id, value, layout))
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
        if self.raw.effects().tasks.len() >= 128 || (live.len() >= 64 && !live.contains_key(&slot))
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
        self.raw.effects().repaint = true
    }
    pub fn relayout(&mut self) {
        self.raw.effects().layout = true;
        self.repaint()
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
        if self.raw.effects().timers.len() >= 128 || (live.len() >= 64 && !live.contains(&timer)) {
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
