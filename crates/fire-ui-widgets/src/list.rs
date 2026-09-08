use fire_ui::*;
use std::{collections::HashMap, hash::Hash, rc::Rc};

/// Inherited by row content so custom rows can style their current selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListRowState {
    pub selected: bool,
}
#[derive(Clone, Debug)]
pub enum ListCommand<K, O> {
    Keys(Vec<K>),
    Navigate(i32),
    Activate,
    ScrollTo(K),
    Row(K, O),
}
impl<K: Data, O: Data> Data for ListCommand<K, O> {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Keys(keys) => keys.iter().map(Data::bytes).sum(),
                Self::Navigate(_) | Self::Activate => 0,
                Self::ScrollTo(k) => k.bytes(),
                Self::Row(k, o) => k.bytes() + o.bytes(),
            }
    }
}
#[derive(Clone, Debug)]
pub enum ListOutput<K, O> {
    Selected(K),
    Row(K, O),
}
impl<K: Data, O: Data> Data for ListOutput<K, O> {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Selected(k) => k.bytes(),
                Self::Row(k, o) => k.bytes() + o.bytes(),
            }
    }
}
/// A fixed-height virtual list. Row content is supplied by the consumer through ordinary widgets.
pub struct VirtualList<K, R, F>
where
    K: Data + Clone + Eq + Hash,
    R: Widget<Output: Clone>,
    F: Fn(&K) -> Element<R> + 'static,
{
    keys: Vec<K>,
    indices: HashMap<K, usize>,
    rows: Vec<(K, Child<R>)>,
    factory: F,
    row_height: f32,
    offset: f32,
    height: f32,
    selected: Option<K>,
    pending: bool,
    hover_selection: bool,
    pressed: Option<u32>,
    row_states: HashMap<K, bool>,
    /// Pointer dragging the scrollbar, and where on the thumb it took hold.
    bar_drag: Option<(u32, f32)>,
    /// Whether the pointer is resting on the scrollbar.
    bar_hover: bool,
    /// Viewport width from the last layout, for cursor resolution.
    width: f32,
    /// Theme from the last layout, so input can reproduce the bar's geometry.
    theme: crate::Theme,
}
impl<K, R, F> VirtualList<K, R, F>
where
    K: Data + Clone + Eq + Hash,
    R: Widget<Output: Clone>,
    F: Fn(&K) -> Element<R> + 'static,
{
    pub fn new(keys: Vec<K>, row_height: f32, factory: F) -> Self {
        assert!(row_height.is_finite() && row_height > 0.);
        let indices = Self::index(&keys);
        Self {
            keys,
            indices,
            rows: vec![],
            factory,
            row_height,
            offset: 0.,
            height: 0.,
            selected: None,
            pending: false,
            hover_selection: false,
            pressed: None,
            row_states: HashMap::new(),
            bar_drag: None,
            bar_hover: false,
            width: 0.,
            theme: crate::Theme::default(),
        }
    }
    pub fn select_on_hover(mut self, enabled: bool) -> Self {
        self.hover_selection = enabled;
        self
    }
    fn index(keys: &[K]) -> HashMap<K, usize> {
        let mut indices = HashMap::with_capacity(keys.len());
        for (i, key) in keys.iter().enumerate() {
            assert!(
                indices.insert(key.clone(), i).is_none(),
                "VirtualList keys must be unique"
            );
        }
        indices
    }
    /// How far the list has scrolled from the top, in pixels.
    pub fn offset(&self) -> f32 {
        self.offset
    }
    pub fn mounted_rows(&self) -> usize {
        self.rows.len()
    }
    pub fn row(&self, key: &K) -> Option<Child<R>> {
        self.rows.iter().find(|(k, _)| k == key).map(|(_, c)| *c)
    }
    fn extent(&self) -> f32 {
        self.keys.len() as f32 * self.row_height
    }
    fn overflow(&self) -> f32 {
        (self.extent() - self.height).max(0.)
    }
    /// The scrollbar for the current geometry, or `None` when everything fits.
    fn bar(&self, bounds: Rect) -> Option<crate::Scrollbar> {
        crate::Scrollbar::new(bounds, self.height, self.extent(), self.offset, &self.theme)
    }
    /// The strip the bar occupies, or zero when the rows all fit.
    fn bar_strip(&self) -> f32 {
        if self.extent() > self.height {
            crate::Scrollbar::width(&self.theme)
        } else {
            0.
        }
    }
    /// Tracks whether the pointer is on the bar, repainting when that changes.
    fn track_bar_hover(&mut self, cx: &mut Update<'_, Self>, position: Point) {
        let hovered = self
            .bar(cx.bounds())
            .is_some_and(|bar| bar.contains(position));
        if hovered != self.bar_hover {
            self.bar_hover = hovered;
            cx.repaint()
        }
    }
    fn scroll_to(&mut self, cx: &mut Update<'_, Self>, offset: f32) {
        let offset = offset.clamp(0., self.overflow());
        if offset != self.offset {
            self.offset = offset;
            self.sync(cx)
        }
    }
    fn navigate(&mut self, cx: &mut Update<'_, Self>, delta: i32) {
        if self.keys.is_empty() {
            return;
        }
        let old = self
            .selected
            .as_ref()
            .and_then(|k| self.indices.get(k).copied());
        let index = old.map_or(0, |i| {
            (i as i64 + delta as i64).clamp(0, self.keys.len() as i64 - 1) as usize
        });
        self.selected = Some(self.keys[index].clone());
        if index as f32 * self.row_height < self.offset {
            self.offset = index as f32 * self.row_height;
        }
        if (index + 1) as f32 * self.row_height > self.offset + self.height {
            self.offset = (index + 1) as f32 * self.row_height - self.height;
        }
        self.sync(cx);
    }
    fn sync(&mut self, cx: &mut Update<'_, Self>) {
        let first = (self.offset / self.row_height).floor().max(0.) as usize;
        let count = (self.height / self.row_height).ceil().max(1.) as usize + 4;
        let start = first.saturating_sub(2);
        let end = (start + count).min(self.keys.len());
        let wanted = &self.keys[start.min(end)..end];
        self.pending = false;
        self.rows.retain(|(key, child)| {
            if wanted.contains(key) {
                true
            } else if cx.remove(*child).is_ok() {
                false
            } else {
                self.pending = true;
                true
            }
        });
        for key in wanted {
            if self.rows.iter().any(|(k, _)| k == key) {
                continue;
            }
            let output_key = key.clone();
            match cx.insert((self.factory)(key), move |output| {
                ListCommand::Row(output_key.clone(), output.clone())
            }) {
                Ok(child) => self.rows.push((key.clone(), child)),
                Err(_) => {
                    self.pending = true;
                    cx.request_frame();
                    break;
                }
            }
        }
        self.row_states
            .retain(|key, _| self.rows.iter().any(|(k, _)| k == key));
        for (key, child) in &self.rows {
            let selected = self.selected.as_ref() == Some(key);
            if self.row_states.get(key) != Some(&selected)
                && cx
                    .set_environment(*child, Rc::new(ListRowState { selected }), false)
                    .is_ok()
            {
                self.row_states.insert(key.clone(), selected);
            }
        }
        if self.pending {
            cx.request_frame();
        }
        cx.relayout();
    }
}
impl<K, R, F> Widget for VirtualList<K, R, F>
where
    K: Data + Clone + Eq + Hash,
    R: Widget<Output: Clone>,
    F: Fn(&K) -> Element<R> + 'static,
{
    type Command = ListCommand<K, R::Output>;
    type Output = ListOutput<K, R::Output>;
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        if matches!(event, Lifecycle::Mount | Lifecycle::Resized) {
            self.height = cx.bounds().height;
            self.sync(cx)
        }
    }
    fn frame(&mut self, cx: &mut Update<'_, Self>, _: FrameTime) {
        if self.pending {
            self.sync(cx)
        }
    }
    fn update(&mut self, cx: &mut Update<'_, Self>, command: Self::Command) {
        match command {
            ListCommand::Navigate(delta) => self.navigate(cx, delta),
            ListCommand::Activate => {
                if let Some(key) = &self.selected {
                    let _ = cx.emit(ListOutput::Selected(key.clone()));
                }
            }
            ListCommand::Keys(keys) => {
                self.indices = Self::index(&keys);
                self.keys = keys;
                self.offset = self
                    .offset
                    .min((self.keys.len() as f32 * self.row_height - self.height).max(0.));
                if self
                    .selected
                    .as_ref()
                    .is_some_and(|k| !self.keys.contains(k))
                {
                    self.selected = None
                }
                self.sync(cx)
            }
            ListCommand::ScrollTo(key) => {
                if let Some(i) = self.indices.get(&key).copied() {
                    self.offset = (i as f32 * self.row_height)
                        .min((self.keys.len() as f32 * self.row_height - self.height).max(0.));
                    self.selected = Some(key);
                    self.sync(cx)
                }
            }
            ListCommand::Row(key, output) => {
                let _ = cx.emit(ListOutput::Row(key, output));
            }
        }
    }
    fn cursor(&self, position: Point) -> Option<CursorIcon> {
        // On its scrollbar the list is a scrolling surface, not a row of choices.
        if self.bar_strip() > 0. && position.x >= self.width - self.bar_strip() {
            return Some(CursorIcon::Arrow);
        }
        Some(CursorIcon::Pointer)
    }
    fn focusable(&self) -> bool {
        true
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        self.theme = crate::theme(cx);
        self.width = c.max.width;
        let extent = self.keys.len() as f32 * self.row_height;
        let size = c.constrain(Size::new(c.max.width, extent.min(c.max.height)));
        self.height = size.height;
        // Rows stop where the scrollbar begins rather than running underneath it.
        let bar = if extent > size.height {
            crate::Scrollbar::width(&self.theme)
        } else {
            0.
        };
        let row_width = (size.width - bar).max(0.);
        for (key, child) in &self.rows {
            if let Some(index) = self.indices.get(key).copied() {
                cx.measure(
                    *child,
                    Constraints::tight(Size::new(row_width, self.row_height)),
                );
                cx.place(
                    *child,
                    Point::new(0., index as f32 * self.row_height - self.offset),
                );
            }
        }
        Metrics::new(size)
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if phase == Phase::Preview {
            return;
        }
        match input {
            Input::Pointer { position, pointer } => {
                if let Some((held, grab)) = self.bar_drag.filter(|(p, _)| p == pointer) {
                    let _ = held;
                    if let Some(bar) = self.bar(cx.bounds()) {
                        let offset = bar.offset_for(position.y - grab, self.overflow());
                        self.scroll_to(cx, offset)
                    }
                    cx.stop();
                    return;
                }
                self.track_bar_hover(cx, *position);
                if self.hover_selection && !self.bar_hover {
                    let index = ((position.y + self.offset) / self.row_height)
                        .floor()
                        .max(0.) as usize;
                    if let Some(key) = self.keys.get(index) {
                        if self.selected.as_ref() != Some(key) {
                            self.selected = Some(key.clone());
                            self.sync(cx);
                        }
                    }
                }
            }
            Input::Scroll { delta, .. } => {
                self.offset = (self.offset - delta.y).clamp(
                    0.,
                    (self.keys.len() as f32 * self.row_height - self.height).max(0.),
                );
                self.sync(cx);
                cx.stop()
            }
            Input::Button {
                button: 1,
                down: true,
                pointer,
                position,
            } if self
                .bar(cx.bounds())
                .is_some_and(|bar| bar.track.contains(*position)) =>
            {
                let bar = self.bar(cx.bounds()).unwrap();
                let grab = if bar.thumb.contains(*position) {
                    position.y - bar.thumb.y
                } else {
                    // Jump so the thumb centres on the click, then keep dragging.
                    let offset =
                        bar.offset_for(position.y - bar.thumb.height / 2., self.overflow());
                    self.scroll_to(cx, offset);
                    bar.thumb.height / 2.
                };
                self.bar_drag = Some((*pointer, grab));
                let _ = cx.capture(*pointer);
                cx.stop()
            }
            Input::Button {
                button: 1,
                down: false,
                pointer,
                ..
            } if self.bar_drag.is_some_and(|(p, _)| p == *pointer) => {
                self.bar_drag = None;
                let _ = cx.release(*pointer);
                cx.repaint();
                cx.stop()
            }
            Input::Button {
                button: 1,
                down: true,
                pointer,
                ..
            } => {
                self.pressed = Some(*pointer);
                let _ = cx.capture(*pointer);
                let _ = cx.focus();
                cx.stop()
            }
            Input::Button {
                button: 1,
                down: false,
                pointer,
                position,
                ..
            } => {
                if self.pressed.take() != Some(*pointer) {
                    return;
                }
                let _ = cx.release(*pointer);
                if !cx.bounds().contains(*position) {
                    return;
                }
                let index = ((position.y + self.offset) / self.row_height)
                    .floor()
                    .max(0.) as usize;
                if let Some(key) = self.keys.get(index) {
                    self.selected = Some(key.clone());
                    let _ = cx.emit(ListOutput::Selected(key.clone()));
                    cx.repaint();
                    cx.stop()
                }
            }
            Input::Key {
                key: Key::Up | Key::Down,
                down: true,
                ..
            } => {
                self.navigate(
                    cx,
                    if matches!(input, Input::Key { key: Key::Up, .. }) {
                        -1
                    } else {
                        1
                    },
                );
                cx.stop();
            }
            Input::Key {
                key: Key::Enter,
                down: true,
                repeat: false,
                ..
            } => {
                if let Some(key) = &self.selected {
                    let _ = cx.emit(ListOutput::Selected(key.clone()));
                    cx.stop()
                }
            }
            _ => {}
        }
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = cx
            .environment::<crate::Theme>()
            .cloned()
            .unwrap_or_default();
        let bar = self.bar(cx.bounds);
        let reserved = bar.map_or(0., |_| crate::Scrollbar::width(&t));
        if let Some(index) = self
            .selected
            .as_ref()
            .and_then(|k| self.indices.get(k).copied())
        {
            let y = index as f32 * self.row_height - self.offset;
            // Only paint a selection that is actually on screen, inset from the edges
            // so its rounded corners sit inside whatever surface holds the list.
            if y + self.row_height > 0. && y < self.height {
                let pad = t.scale.space(0.25);
                let row = Rect::new(
                    pad,
                    y + pad,
                    (cx.bounds.width - reserved - 2. * pad).max(0.),
                    (self.row_height - 2. * pad).max(0.),
                );
                cx.painter
                    .rect(row, t.scale.radius, t.color.selection.into());
                // A short accent edge marks the selected row without a heavy fill.
                cx.painter.rect(
                    Rect::new(row.x, row.y, 2.5, row.height),
                    1.25,
                    t.color.accent.into(),
                )
            }
        }
        if let Some(bar) = bar {
            bar.paint(cx.painter, &t, self.bar_hover, self.bar_drag.is_some())
        }
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::List,
            label: format!("{} items", self.keys.len()),
            ..Semantics::default()
        }
    }
}
