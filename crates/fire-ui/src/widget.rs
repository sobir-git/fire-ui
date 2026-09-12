use crate::*;
use std::{
    any::Any,
    marker::PhantomData,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Id(pub u64);
pub(crate) fn identity() -> Id {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    Id(NEXT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
        .expect("identity space exhausted"))
}
/// An opaque, invariant typed child reference. It does not borrow widget state.
pub struct Child<W: Widget> {
    pub(crate) id: Id,
    marker: PhantomData<fn(W) -> W>,
}
impl<W: Widget> Copy for Child<W> {}
impl<W: Widget> Clone for Child<W> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<W: Widget> std::fmt::Debug for Child<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Child")
    }
}
impl<W: Widget> Child<W> {
    pub(crate) fn new(id: Id) -> Self {
        Self {
            id,
            marker: PhantomData,
        }
    }
}
/// Queued payloads report owned storage. Implementations must include heap allocations they own.
pub trait Data: Any {
    fn bytes(&self) -> usize;
}
impl Data for () {
    fn bytes(&self) -> usize {
        0
    }
}
impl Data for std::convert::Infallible {
    fn bytes(&self) -> usize {
        match *self {}
    }
}
impl Data for String {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.capacity()
    }
}
impl Data for usize {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}
impl Data for bool {
    fn bytes(&self) -> usize {
        1
    }
}
/// Fixed-size scalars cost their own size; the macro keeps the set consistent.
macro_rules! scalar_data {
    ($($t:ty),*) => {$(
        impl Data for $t {
            fn bytes(&self) -> usize {
                std::mem::size_of::<Self>()
            }
        }
    )*};
}
scalar_data!(f32, f64, u8, u16, u32, u64, i8, i16, i32, i64, isize, char);
#[derive(Clone, Copy, Debug)]
pub struct FrameTime {
    pub now: Duration,
    pub elapsed: Duration,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Generic,
    Button,
    Text,
    TextInput,
    List,
    ListItem,
    Dialog,
    Canvas,
    CheckBox,
    Radio,
    Switch,
    Slider,
    Progress,
    Menu,
    MenuItem,
    Tab,
    TabList,
    Heading,
    Link,
    Group,
}
#[derive(Clone, Debug)]
pub struct Semantics {
    pub role: Role,
    pub label: String,
    pub value: Option<String>,
    pub disabled: bool,
    pub selected: bool,
    /// Stable application-supplied identifier for inspection and tests.
    pub key: Option<String>,
    pub checked: Option<bool>,
    /// Numeric state for sliders and progress. Published separately from `value`
    /// so assistive technology can announce and adjust a real quantity.
    pub range: Option<Range>,
    pub actions: Vec<SemanticActionKind>,
    pub text: Option<TextSemantics>,
    /// Widget-local bounds for this node when it differs from the widget's
    /// own bounds, as with children a widget publishes below.
    pub bounds: Option<Rect>,
    /// Internal structure a widget publishes below itself, such as interactive
    /// regions inside a canvas-like widget. Actions on a child route back to
    /// the widget as [`SemanticAction::ActivateChild`], keyed by the child's
    /// `key`.
    /// Each child must have a unique, stable key within its owner. Unkeyed
    /// children are not published as independently actionable nodes.
    pub children: Vec<Semantics>,
}
impl Default for Semantics {
    fn default() -> Self {
        Self {
            role: Role::Generic,
            label: String::new(),
            value: None,
            disabled: false,
            selected: false,
            key: None,
            checked: None,
            range: None,
            actions: vec![],
            text: None,
            bounds: None,
            children: vec![],
        }
    }
}
/// A numeric quantity a widget reports, and optionally accepts through `SetValue`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub now: f32,
    pub min: f32,
    pub max: f32,
    /// Increment for one keyboard or assistive-technology step.
    pub step: f32,
}
/// Actions advertised by widgets, shared by assistive technology and automation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticActionKind {
    Focus,
    Activate,
    SetValue,
    ReplaceSelectedText,
    ReplaceText,
    SetSelection,
    ScrollBy,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticAction {
    Focus,
    Activate,
    SetValue(String),
    ReplaceSelectedText(String),
    /// Replace a UTF-8 byte range atomically, without changing selection first.
    /// Endpoints must be scalar boundaries; the resulting caret is a grapheme boundary.
    ReplaceText {
        start: usize,
        end: usize,
        text: String,
    },
    /// Move content by a logical displacement, using the same sign as wheel input.
    ScrollBy(Point),
    /// UTF-8 byte offsets, preserving selection direction.
    SetSelection {
        anchor: usize,
        caret: usize,
    },
    /// Activate a child node this widget published, identified by the child's
    /// `key`. The runtime translates activation of a published child into
    /// this action on the widget itself.
    ActivateChild {
        key: String,
    },
}
impl SemanticAction {
    pub fn kind(&self) -> SemanticActionKind {
        match self {
            Self::Focus => SemanticActionKind::Focus,
            Self::Activate | Self::ActivateChild { .. } => SemanticActionKind::Activate,
            Self::SetValue(_) => SemanticActionKind::SetValue,
            Self::ReplaceSelectedText(_) => SemanticActionKind::ReplaceSelectedText,
            Self::ReplaceText { .. } => SemanticActionKind::ReplaceText,
            Self::SetSelection { .. } => SemanticActionKind::SetSelection,
            Self::ScrollBy(_) => SemanticActionKind::ScrollBy,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Composition {
    pub text: String,
    /// UTF-8 byte offsets within the uncommitted text. None hides the IME caret.
    pub selection: Option<(usize, usize)>,
}
#[derive(Clone, Debug)]
pub struct TextSemantics {
    pub anchor: usize,
    pub caret: Caret,
    pub multiline: bool,
    pub composition: Option<Composition>,
    pub paragraph: Option<std::sync::Arc<Paragraph>>,
    /// Paragraph origin in widget coordinates.
    pub origin: Point,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticError {
    Unavailable,
    Unsupported,
    InvalidSelection,
    /// A `SetValue` payload that is not a value this widget accepts.
    InvalidValue,
    LimitReached,
}
/// All custom and built-in widgets use this protocol. No runtime nodes are public.
pub trait Widget: Any + Sized {
    type Command: Data;
    type Output: Data;
    fn update(&mut self, _cx: &mut Update<'_, Self>, _command: Self::Command) {}
    fn lifecycle(&mut self, _cx: &mut Update<'_, Self>, _event: Lifecycle) {}
    fn input(&mut self, _cx: &mut Update<'_, Self>, _phase: Phase, _input: &Input) {}
    fn frame(&mut self, _cx: &mut Update<'_, Self>, _time: FrameTime) {}
    fn timer(&mut self, _cx: &mut Update<'_, Self>, _timer: Timer) {}
    fn close_requested(&mut self, _cx: &mut Update<'_, Self>) -> bool {
        true
    }
    fn layout(&mut self, cx: &mut Layout<'_>, limits: Constraints) -> Metrics {
        cx.overlay(limits)
    }
    fn paint(&self, _cx: &mut Paint<'_>) {}
    fn focusable(&self) -> bool {
        false
    }
    fn cursor(&self, _position: Point) -> Option<CursorIcon> {
        None
    }
    fn ime_cursor(&self) -> Option<Rect> {
        None
    }
    fn semantics(&self) -> Semantics {
        Semantics::default()
    }
    fn accessibility(
        &mut self,
        cx: &mut Update<'_, Self>,
        action: SemanticAction,
    ) -> Result<(), SemanticError> {
        if action == SemanticAction::Focus {
            cx.focus().map_err(|_| SemanticError::Unavailable)
        } else {
            Err(SemanticError::Unsupported)
        }
    }
}
pub(crate) type Payload = Box<dyn Any>;
type Mapper = dyn Fn(&dyn Any) -> (Payload, usize);
pub(crate) enum OutputMap {
    Forward,
    /// A transparent decorator: the child's output is its owner's output, and is
    /// delivered past the owner without invoking its `update`.
    Bubble,
    Map(Box<Mapper>),
}
pub(crate) trait Erased {
    fn state(&self) -> &dyn Any;
    fn close_requested(&mut self, cx: &mut crate::context::RawUpdate<'_>) -> bool;
    fn update(&mut self, cx: &mut crate::context::RawUpdate<'_>, payload: Payload);
    fn lifecycle(&mut self, cx: &mut crate::context::RawUpdate<'_>, event: Lifecycle);
    fn input(&mut self, cx: &mut crate::context::RawUpdate<'_>, phase: Phase, input: &Input);
    fn frame(&mut self, cx: &mut crate::context::RawUpdate<'_>, time: FrameTime);
    fn timer(&mut self, cx: &mut crate::context::RawUpdate<'_>, timer: Timer);
    fn layout(&mut self, cx: &mut Layout<'_>, limits: Constraints) -> Metrics;
    fn paint(&self, cx: &mut Paint<'_>);
    fn focusable(&self) -> bool;
    fn cursor(&self, position: Point) -> Option<CursorIcon>;
    fn ime_cursor(&self) -> Option<Rect>;
    fn semantics(&self) -> Semantics;
    fn accessibility(
        &mut self,
        cx: &mut crate::context::RawUpdate<'_>,
        action: SemanticAction,
    ) -> Result<(), SemanticError>;
}
impl<W: Widget> Erased for W {
    fn close_requested(&mut self, cx: &mut crate::context::RawUpdate<'_>) -> bool {
        self.close_requested(&mut cx.typed())
    }
    fn state(&self) -> &dyn Any {
        self
    }
    fn update(&mut self, cx: &mut crate::context::RawUpdate<'_>, payload: Payload) {
        self.update(
            &mut cx.typed(),
            *payload
                .downcast::<W::Command>()
                .expect("private command type invariant"),
        );
    }
    fn lifecycle(&mut self, cx: &mut crate::context::RawUpdate<'_>, event: Lifecycle) {
        self.lifecycle(&mut cx.typed(), event)
    }
    fn input(&mut self, cx: &mut crate::context::RawUpdate<'_>, phase: Phase, input: &Input) {
        self.input(&mut cx.typed(), phase, input)
    }
    fn frame(&mut self, cx: &mut crate::context::RawUpdate<'_>, time: FrameTime) {
        self.frame(&mut cx.typed(), time)
    }
    fn timer(&mut self, cx: &mut crate::context::RawUpdate<'_>, timer: Timer) {
        self.timer(&mut cx.typed(), timer)
    }
    fn layout(&mut self, cx: &mut Layout<'_>, limits: Constraints) -> Metrics {
        self.layout(cx, limits)
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        self.paint(cx)
    }
    fn focusable(&self) -> bool {
        self.focusable()
    }
    fn cursor(&self, position: Point) -> Option<CursorIcon> {
        self.cursor(position)
    }
    fn ime_cursor(&self) -> Option<Rect> {
        self.ime_cursor()
    }
    fn semantics(&self) -> Semantics {
        self.semantics()
    }
    fn accessibility(
        &mut self,
        cx: &mut crate::context::RawUpdate<'_>,
        action: SemanticAction,
    ) -> Result<(), SemanticError> {
        self.accessibility(&mut cx.typed(), action)
    }
}
pub(crate) struct Prepared {
    pub id: Id,
    pub widget: Box<dyn Erased>,
    pub children: Vec<Prepared>,
    pub output: Option<OutputMap>,
    pub count: usize,
    /// Set when the node is to be born an overlay, which cannot be arranged after
    /// insertion: the handle is not yet owned when the inserting callback runs.
    pub anchor: Option<Option<Id>>,
}
pub struct Element<W: Widget> {
    pub(crate) prepared: Prepared,
    marker: PhantomData<fn(W) -> W>,
}
impl<W: Widget> Element<W> {
    pub fn leaf(widget: W) -> Self {
        Self::build(|_| widget)
    }
    pub fn build(build: impl FnOnce(&mut Children<W>) -> W) -> Self {
        let mut children = Children {
            nodes: vec![],
            marker: PhantomData,
        };
        let widget = build(&mut children);
        let count = 1 + children.nodes.iter().map(|n| n.count).sum::<usize>();
        Self {
            prepared: Prepared {
                id: identity(),
                widget: Box::new(widget),
                children: children.nodes,
                output: None,
                count,
                anchor: None,
            },
            marker: PhantomData,
        }
    }
    pub(crate) fn from_prepared(prepared: Prepared) -> Self {
        Self {
            prepared,
            marker: PhantomData,
        }
    }
}
pub struct Children<P: Widget> {
    nodes: Vec<Prepared>,
    marker: PhantomData<fn(P) -> P>,
}
impl<P: Widget> Children<P> {
    pub fn add<W: Widget<Output = std::convert::Infallible>>(
        &mut self,
        element: Element<W>,
    ) -> Child<W> {
        self.push(element.prepared)
    }
    pub fn discard<W: Widget>(&mut self, element: Element<W>) -> Child<W> {
        self.push(element.prepared)
    }
    pub fn connect<W: Widget>(
        &mut self,
        element: Element<W>,
        map: impl Fn(&W::Output) -> P::Command + 'static,
    ) -> Child<W> {
        let mut node = element.prepared;
        node.output = Some(OutputMap::Map(Box::new(move |p| {
            let command = map(p
                .downcast_ref::<W::Output>()
                .expect("private output type invariant"));
            let bytes = command.bytes();
            (Box::new(command), bytes)
        })));
        self.push(node)
    }
    pub fn forward<W: Widget<Output = P::Command>>(&mut self, element: Element<W>) -> Child<W> {
        let mut node = element.prepared;
        node.output = Some(OutputMap::Forward);
        self.push(node)
    }
    /// Adopt a child whose output *is* this widget's output.
    ///
    /// The payload travels straight to this widget's own owner without passing
    /// through `update`, which leaves `Command` free to address the child. This is
    /// what makes a decorator — padding, alignment, a themed surface — transparent
    /// in both directions instead of only upwards.
    pub fn bubble<W: Widget<Output = P::Output>>(&mut self, element: Element<W>) -> Child<W> {
        let mut node = element.prepared;
        node.output = Some(OutputMap::Bubble);
        self.push(node)
    }
    fn push<W: Widget>(&mut self, node: Prepared) -> Child<W> {
        let child = Child::new(node.id);
        self.nodes.push(node);
        child
    }
}
/// A child reference erased only for layout. It cannot send or receive payloads.
#[derive(Clone, Copy, Debug)]
pub struct LayoutChild(pub(crate) Id);
impl<W: Widget> From<Child<W>> for LayoutChild {
    fn from(child: Child<W>) -> Self {
        Self(child.id)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskSlot(pub(crate) Id);
impl Default for TaskSlot {
    fn default() -> Self {
        Self::new()
    }
}
impl TaskSlot {
    pub fn new() -> Self {
        Self(identity())
    }
}
pub struct Ticket<W: Widget> {
    pub(crate) owner: Id,
    pub(crate) slot: TaskSlot,
    pub(crate) epoch: u64,
    marker: PhantomData<fn(W) -> W>,
}
impl<W: Widget> Copy for Ticket<W> {}
impl<W: Widget> Clone for Ticket<W> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<W: Widget> std::fmt::Debug for Ticket<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ticket")
    }
}
impl<W: Widget> Data for Ticket<W> {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}
impl<W: Widget> Ticket<W> {
    pub(crate) fn new(owner: Id, slot: TaskSlot, epoch: u64) -> Self {
        Self {
            owner,
            slot,
            epoch,
            marker: PhantomData,
        }
    }
}
