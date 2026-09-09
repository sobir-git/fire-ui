use crate::{
    widget::{Erased, Id, OutputMap, Prepared},
    *,
};
use std::{
    any::{Any, TypeId},
    collections::{BTreeMap, BTreeSet, HashMap},
    rc::Rc,
};
/// Ambient values for a subtree, keyed by type.
///
/// A widget reads a type; it never sees the others. Two owners publishing different
/// types to the same node no longer evict one another, which is what a single slot
/// did. Entry counts are tiny, so a linear scan beats hashing here.
#[derive(Default)]
pub struct Environment {
    entries: Vec<(TypeId, Rc<dyn Any>)>,
}
impl Environment {
    pub fn get<T: Any>(&self) -> Option<&T> {
        let wanted = TypeId::of::<T>();
        self.entries
            .iter()
            .find(|(id, _)| *id == wanted)
            .and_then(|(_, value)| value.downcast_ref())
    }
    /// The value stored for a type, without knowing the type statically.
    pub fn get_any(&self, type_id: TypeId) -> Option<Rc<dyn Any>> {
        self.entries
            .iter()
            .find(|(id, _)| *id == type_id)
            .map(|(_, value)| value.clone())
    }
    /// This map with `type_id` set to `value`, leaving every other type in place.
    pub fn with(&self, type_id: TypeId, value: Rc<dyn Any>) -> Rc<Self> {
        let mut entries = self.entries.clone();
        match entries.iter_mut().find(|(id, _)| *id == type_id) {
            Some(slot) => slot.1 = value,
            None => entries.push((type_id, value)),
        }
        Rc::new(Self { entries })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Geometry {
    pub transform: Transform,
    pub bounds: Rect,
    pub clip: Rect,
    pub visible: bool,
    pub revision: u64,
}
pub(crate) struct Node {
    pub timers: BTreeSet<Timer>,
    pub tasks: BTreeMap<TaskSlot, u64>,
    pub parent: Option<Id>,
    pub children: Vec<Id>,
    pub widget: Option<Box<dyn Erased>>,
    pub output: Option<OutputMap>,
    pub resized: bool,
    pub mounted: bool,
    pub retiring: bool,
    pub visible: bool,
    pub published: Option<bool>,
    pub published_anchor: Option<bool>,
    pub local: Transform,
    pub clip: bool,
    /// `None` for an ordinary child. `Some(None)` for an overlay placed
    /// against the window, `Some(Some(id))` for one that follows a node.
    pub anchor: Option<Option<Id>>,
    pub size: Size,
    pub cache: Option<(Constraints, TextRevision, Metrics)>,
    pub dirty: bool,
    pub geometry_dirty: bool,
    pub geometry: Geometry,
    /// Every ambient value this node and its descendants can read. Nodes share one
    /// map until something publishes a value of its own, so installing a value for a
    /// subtree usually allocates once for the whole subtree.
    pub environment: Rc<Environment>,
    /// Types this node publishes for itself. Inheritance of those types stops here;
    /// every other type still flows through.
    pub overrides: Vec<TypeId>,
}
#[derive(Default)]
pub(crate) struct Tree {
    slots: Vec<Option<Node>>,
    index: HashMap<Id, usize>,
    free: Vec<usize>,
    /// The window rectangle in effect for this layout pass.
    pub viewport: Rect,
    pub layout_count: u64,
    pub geometry_count: u64,
}
impl Tree {
    pub fn len(&self) -> usize {
        self.index.len()
    }
    pub fn get(&self, id: Id) -> Option<&Node> {
        self.slots.get(*self.index.get(&id)?)?.as_ref()
    }
    pub fn get_mut(&mut self, id: Id) -> Option<&mut Node> {
        self.slots.get_mut(*self.index.get(&id)?)?.as_mut()
    }
    pub fn live(&self, id: Id) -> bool {
        self.get(id).is_some_and(|n| !n.retiring)
    }
    pub fn ids(&self) -> impl Iterator<Item = Id> + '_ {
        self.index.keys().copied()
    }
    pub fn descendant(&self, mut id: Id, parent: Id) -> bool {
        loop {
            if id == parent {
                return true;
            }
            let Some(p) = self.get(id).and_then(|n| n.parent) else {
                return false;
            };
            id = p;
        }
    }
    pub fn active(&self, id: Id) -> bool {
        self.get(id).is_some_and(|n| {
            n.mounted
                && !n.retiring
                && n.visible
                && n.local.inverse().is_some()
                && n.parent.is_none_or(|p| self.active(p))
                && n.anchor.is_none_or(|a| a.is_none_or(|a| self.active(a)))
        })
    }
    pub fn insert(&mut self, parent: Option<Id>, prepared: Prepared, out: &mut Vec<Id>) {
        let Prepared {
            id,
            widget,
            children,
            output,
            anchor,
            ..
        } = prepared;
        let ids = children.iter().map(|n| n.id).collect();
        let environment = parent
            .and_then(|p| self.get(p))
            .map(|n| n.environment.clone())
            .unwrap_or_default();
        let node = Node {
            timers: BTreeSet::new(),
            tasks: BTreeMap::new(),
            parent,
            children: ids,
            widget: Some(widget),
            output,
            resized: false,
            mounted: false,
            retiring: false,
            visible: true,
            published: None,
            published_anchor: None,
            local: Transform::IDENTITY,
            clip: true,
            anchor,
            size: Size::ZERO,
            cache: None,
            dirty: true,
            geometry_dirty: true,
            geometry: Geometry::default(),
            environment,
            overrides: vec![],
        };
        let slot = self.free.pop().unwrap_or(self.slots.len());
        if slot == self.slots.len() {
            self.slots.push(Some(node))
        } else {
            self.slots[slot] = Some(node)
        }
        self.index.insert(id, slot);
        out.push(id);
        for child in children {
            self.insert(Some(id), child, out)
        }
    }
    pub fn discard(&mut self, id: Id) {
        if let Some(i) = self.index.remove(&id) {
            self.slots[i] = None;
            self.free.push(i)
        }
    }
    pub fn invalidate(&mut self, mut id: Id) {
        while let Some(n) = self.get_mut(id) {
            n.dirty = true;
            n.geometry_dirty = true;
            let Some(p) = n.parent else { break };
            id = p;
        }
    }
    pub fn geometry_dirty(&mut self, mut id: Id) {
        while let Some(n) = self.get_mut(id) {
            n.geometry_dirty = true;
            let Some(p) = n.parent else { break };
            id = p;
        }
    }
    pub fn measure(&mut self, id: Id, limits: Constraints, text: &mut dyn TextEngine) -> Metrics {
        let Some(n) = self.get(id) else {
            return Metrics::default();
        };
        if !n.mounted || !n.visible || n.retiring {
            return Metrics::default();
        }
        if !n.dirty {
            if let Some((previous, revision, metrics)) = n.cache {
                if limits == previous && revision == text.revision() {
                    return metrics;
                }
            }
        }
        let Some(mut widget) = self.get_mut(id).and_then(|n| n.widget.take()) else {
            return Metrics::default();
        };
        self.layout_count += 1;
        let mut metrics = widget.layout(
            &mut Layout {
                tree: self,
                me: id,
                text,
            },
            limits,
        );
        assert!(
            metrics.size.valid(),
            "Widget returned non-finite or negative layout size"
        );
        metrics.size = limits.constrain(metrics.size);
        if let Some(b) = metrics.baseline {
            assert!(
                b.is_finite() && b >= 0. && b <= metrics.size.height,
                "Invalid text baseline"
            )
        }
        let revision = text.revision();
        let n = self.get_mut(id).unwrap();
        n.widget = Some(widget);
        n.resized |= n.size != metrics.size;
        n.size = metrics.size;
        n.cache = Some((limits, revision, metrics));
        n.dirty = false;
        n.geometry_dirty = true;
        metrics
    }
    pub fn place(&mut self, id: Id, local: Transform, clip: bool, anchor: Option<Option<Id>>) {
        let n = self.get_mut(id).unwrap();
        if n.local != local || n.clip != clip || n.anchor != anchor {
            n.local = local;
            n.clip = clip;
            n.anchor = anchor;
            self.geometry_dirty(id)
        }
    }
    pub fn publish(
        &mut self,
        id: Id,
        parent: Transform,
        clip: Rect,
        visible: bool,
        revision: u64,
        force: bool,
    ) {
        let Some(n) = self.get(id) else { return };
        if !force && !n.geometry_dirty {
            return;
        }
        let transform = parent.compose(n.local);
        let visible =
            visible && n.visible && n.mounted && !n.retiring && transform.inverse().is_some();
        let bounds = transform.rect(Rect::from_size(n.size));
        let clip = if n.clip { clip.intersect(bounds) } else { clip };
        let changed = n.geometry.transform != transform
            || n.geometry.bounds != bounds
            || n.geometry.clip != clip
            || n.geometry.visible != visible;
        let children = n.children.clone();
        let n = self.get_mut(id).unwrap();
        n.geometry = Geometry {
            transform,
            bounds,
            clip,
            visible,
            revision: if changed {
                revision
            } else {
                n.geometry.revision
            },
        };
        n.geometry_dirty = false;
        self.geometry_count += 1;
        for child in children {
            if self.get(child).is_some_and(|n| n.anchor.is_none()) {
                self.publish(child, transform, clip, visible, revision, changed)
            }
        }
    }
}
/// Measurement and placement of owned children. It cannot dispatch commands or paint.
pub struct Layout<'a> {
    pub(crate) tree: &'a mut Tree,
    pub(crate) me: Id,
    pub(crate) text: &'a mut dyn TextEngine,
}

impl Layout<'_> {
    pub fn measure<W: Widget>(&mut self, child: Child<W>, limits: Constraints) -> Metrics {
        assert!(
            self.tree
                .get(child.id)
                .is_some_and(|n| n.parent == Some(self.me)),
            "Layout can only measure owned children"
        );
        self.tree.measure(child.id, limits, self.text)
    }
    pub fn place<W: Widget>(&mut self, child: Child<W>, position: Point) {
        self.transform(child, Transform::translate(position.x, position.y), true)
    }
    pub fn transform<W: Widget>(&mut self, child: Child<W>, transform: Transform, clip: bool) {
        assert!(self
            .tree
            .get(child.id)
            .is_some_and(|n| n.parent == Some(self.me)));
        let anchor = self.tree.get(child.id).unwrap().anchor;
        self.tree.place(child.id, transform, clip, anchor)
    }
    pub fn paragraph(&mut self, request: TextRequest) -> std::sync::Arc<Paragraph> {
        self.text.layout(request)
    }
    pub fn text_revision(&self) -> TextRevision {
        self.text.revision()
    }
    /// The window's rectangle. Window-anchored overlays measure against it.
    pub fn viewport(&self) -> Rect {
        self.tree.viewport
    }
    pub fn environment<T: Any>(&self) -> Option<&T> {
        self.tree.get(self.me)?.environment.get::<T>()
    }
    pub fn overlay(&mut self, limits: Constraints) -> Metrics {
        let children = self.tree.get(self.me).unwrap().children.clone();
        let mut size = Size::ZERO;
        for child in children {
            let metrics = self.tree.measure(child, limits, self.text);
            let anchor = self.tree.get(child).and_then(|n| n.anchor);
            self.tree.place(child, Transform::IDENTITY, true, anchor);
            size.width = size.width.max(metrics.size.width);
            size.height = size.height.max(metrics.size.height)
        }
        Metrics::new(limits.constrain(size))
    }
}
pub struct Paint<'a> {
    pub bounds: Rect,
    pub focused: bool,
    /// True when this widget or an owned descendant is hovered in the active modal scope.
    pub hovered: bool,
    pub painter: &'a mut dyn Painter,
    pub(crate) environment: Option<&'a Environment>,
}
impl Paint<'_> {
    pub fn environment<T: Any>(&self) -> Option<&T> {
        self.environment?.get::<T>()
    }
}
impl Layout<'_> {
    pub fn measure_child(&mut self, child: LayoutChild, limits: Constraints) -> Metrics {
        assert!(self
            .tree
            .get(child.0)
            .is_some_and(|n| n.parent == Some(self.me)));
        self.tree.measure(child.0, limits, self.text)
    }
    pub fn place_child(&mut self, child: LayoutChild, position: Point) {
        assert!(self
            .tree
            .get(child.0)
            .is_some_and(|n| n.parent == Some(self.me)));
        let n = self.tree.get(child.0).unwrap();
        self.tree.place(
            child.0,
            Transform::translate(position.x, position.y),
            n.clip,
            n.anchor,
        )
    }
}
