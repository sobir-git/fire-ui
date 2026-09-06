use crate::{theme, Theme};
use fire_ui::*;
use std::{collections::VecDeque, ops::Range, sync::Arc, time::Duration};
use unicode_segmentation::UnicodeSegmentation;
/// Storage adapters own text; editing and rendering share versioned paragraph snapshots.
pub trait Document: 'static {
    fn text(&self) -> &str;
    fn revision(&self) -> u64;
    fn replace(&mut self, range: Range<usize>, text: &str);
}
pub struct StringDocument {
    text: String,
    revision: u64,
}
impl StringDocument {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            revision: 0,
        }
    }
}
impl Document for StringDocument {
    fn text(&self) -> &str {
        &self.text
    }
    fn revision(&self) -> u64 {
        self.revision
    }
    fn replace(&mut self, range: Range<usize>, text: &str) {
        self.text.replace_range(range, text);
        self.revision += 1
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct EditorState {
    pub caret: Caret,
    pub anchor: Option<usize>,
    pub scroll: Point,
    pub wrap: bool,
}
impl Default for EditorState {
    fn default() -> Self {
        Self {
            caret: Caret::at(0),
            anchor: None,
            scroll: Point::default(),
            wrap: true,
        }
    }
}
impl Data for EditorState {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}
#[derive(Clone, Debug)]
pub enum Edit {
    Set(String),
    Insert(String),
    Select { anchor: usize, caret: usize },
    Undo,
    Redo,
    Wrap(bool),
    ReportCursor,
    Restore(EditorState),
    Placeholder(Arc<str>),
}
impl Data for Edit {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Set(s) | Self::Insert(s) => s.capacity(),
                Self::Placeholder(s) => s.len(),
                _ => 0,
            }
    }
}
#[derive(Clone, Debug)]
pub enum EditorOutput {
    Changed { revision: u64, text: Arc<str> },
    Submitted,
    FocusChanged(bool),
    LimitReached,
    Cursor(Rect),
    StateChanged(EditorState),
}
impl Data for EditorOutput {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Changed { text, .. } => text.len(),
                Self::Submitted
                | Self::FocusChanged(_)
                | Self::LimitReached
                | Self::Cursor(_)
                | Self::StateChanged(_) => 0,
            }
    }
}
struct Undo {
    start: usize,
    removed: String,
    inserted: String,
}
/// Geometry in paragraph coordinates, shared by editing and optional decoration layers.
pub struct EditorView<'a> {
    pub paragraph: &'a Paragraph,
    pub caret: Caret,
    pub selection: Option<Range<usize>>,
    pub viewport: Rect,
    pub focused: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EditorLayer {
    BehindText,
    AboveText,
}
/// Optional consumer-owned effects. Returning true schedules another visible frame.
pub trait EditorDecoration: 'static {
    fn frame(&mut self, view: &EditorView<'_>, time: FrameTime, edited: bool) -> bool;
    fn paint(&self, view: &EditorView<'_>, layer: EditorLayer, painter: &mut dyn Painter);
}
/// A retained editor. Edits are public commands, while pointer/key input uses the same operations.
pub struct Editor<D: Document = StringDocument> {
    document: D,
    caret: Caret,
    anchor: Option<usize>,
    paragraph: Option<Arc<Paragraph>>,
    theme: Theme,
    scroll: Point,
    wrap: bool,
    multiline: bool,
    placeholder: Arc<str>,
    placeholder_layout: Option<Arc<Paragraph>>,
    undo: VecDeque<Undo>,
    redo: Vec<Undo>,
    history_bytes: usize,
    blink: Timer,
    caret_on: bool,
    caret_blink: bool,
    max_bytes: usize,
    limit_reached: bool,
    drag: Option<u32>,
    drag_position: Point,
    state_pending: bool,
    preedit: String,
    preedit_layout: Option<Arc<Paragraph>>,
    last_click: Option<(Duration, Point)>,
    click_count: u8,
    modifiers: Modifiers,
    chrome: bool,
    padding: Option<Point>,
    decoration: Option<Box<dyn EditorDecoration>>,
    edited: bool,
    reveal_pending: bool,
    restore_pending: bool,
    scrollbar_drag: Option<(u32, f32)>,
}
impl Editor<StringDocument> {
    pub fn new(text: impl Into<String>) -> Self {
        Self::with_document(StringDocument::new(text))
    }
    pub fn field(text: impl Into<String>) -> Self {
        Self {
            multiline: false,
            wrap: false,
            ..Self::new(text)
        }
    }
}
impl<D: Document> Editor<D> {
    pub fn with_document(document: D) -> Self {
        Self {
            document,
            caret: Caret::at(0),
            anchor: None,
            paragraph: None,
            theme: Theme::default(),
            scroll: Point::default(),
            wrap: true,
            multiline: true,
            placeholder: Arc::from(""),
            placeholder_layout: None,
            undo: VecDeque::new(),
            redo: vec![],
            history_bytes: 0,
            blink: Timer::new(),
            caret_on: true,
            caret_blink: true,
            max_bytes: usize::MAX,
            limit_reached: false,
            drag: None,
            drag_position: Point::default(),
            state_pending: false,
            preedit: String::new(),
            preedit_layout: None,
            last_click: None,
            click_count: 0,
            modifiers: Modifiers::default(),
            chrome: true,
            padding: None,
            decoration: None,
            edited: false,
            reveal_pending: true,
            restore_pending: false,
            scrollbar_drag: None,
        }
    }
    pub fn state(&self) -> EditorState {
        EditorState {
            caret: self.caret,
            anchor: self.anchor,
            scroll: self.scroll,
            wrap: self.wrap,
        }
    }
    pub fn restore(mut self, state: EditorState) -> Self {
        self.apply_state(state);
        self
    }
    fn apply_state(&mut self, state: EditorState) {
        self.caret = Caret {
            byte: self.boundary(state.caret.byte),
            affinity: state.caret.affinity,
        };
        self.anchor = state.anchor.map(|b| self.boundary(b));
        self.scroll = Point::new(
            if state.scroll.x.is_finite() {
                state.scroll.x.max(0.)
            } else {
                0.
            },
            if state.scroll.y.is_finite() {
                state.scroll.y.max(0.)
            } else {
                0.
            },
        );
        self.wrap = state.wrap;
        self.restore_pending = true;
        self.reveal_pending = false;
    }
    fn report_state(&self, cx: &mut Update<'_, Self>) {
        let _ = cx.emit(EditorOutput::StateChanged(self.state()));
    }
    fn scrollbar(&self, bounds: Rect) -> Option<Rect> {
        let total = self.paragraph.as_ref()?.size.height + 2. * self.insets().y;
        if !self.multiline || total <= bounds.height {
            return None;
        }
        let height = (bounds.height * bounds.height / total)
            .max(30.)
            .min(bounds.height);
        let y = self.scroll.y / (total - bounds.height) * (bounds.height - height);
        Some(Rect::new(bounds.width - 12., y, 12., height))
    }
    pub fn padding(mut self, horizontal: f32, vertical: f32) -> Self {
        assert!(
            horizontal.is_finite() && vertical.is_finite() && horizontal >= 0. && vertical >= 0.
        );
        self.padding = Some(Point::new(horizontal, vertical));
        self
    }
    pub fn decoration(mut self, decoration: impl EditorDecoration) -> Self {
        self.decoration = Some(Box::new(decoration));
        self
    }
    fn insets(&self) -> Point {
        self.padding
            .unwrap_or(Point::new(self.theme.inset, self.theme.inset))
    }
    fn view(&self, bounds: Rect, focused: bool) -> Option<EditorView<'_>> {
        Some(EditorView {
            paragraph: self.paragraph.as_deref()?,
            caret: self.caret,
            selection: self.selection(),
            viewport: Rect::new(self.scroll.x, self.scroll.y, bounds.width, bounds.height),
            focused,
        })
    }
    pub fn placeholder(mut self, text: impl Into<Arc<str>>) -> Self {
        self.placeholder = text.into();
        self
    }
    pub fn caret_blink(mut self, enabled: bool) -> Self {
        self.caret_blink = enabled;
        self
    }
    /// Reject edits that grow beyond this size. Initial text and `Edit::Set`
    /// remain application-controlled; existing oversized text can be reduced.
    pub fn max_bytes(mut self, bytes: usize) -> Self {
        self.max_bytes = bytes;
        self
    }
    pub fn chrome(mut self, chrome: bool) -> Self {
        self.chrome = chrome;
        self
    }
    pub fn text(&self) -> &str {
        self.document.text()
    }
    pub fn caret(&self) -> Caret {
        self.caret
    }
    pub fn selection(&self) -> Option<Range<usize>> {
        self.anchor
            .filter(|a| *a != self.caret.byte)
            .map(|a| a.min(self.caret.byte)..a.max(self.caret.byte))
    }
    pub fn paragraph(&self) -> Option<&Arc<Paragraph>> {
        self.paragraph.as_ref()
    }
    fn boundary(&self, byte: usize) -> usize {
        self.text()
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain(std::iter::once(self.text().len()))
            .take_while(|i| *i <= byte)
            .last()
            .unwrap_or(0)
    }
    fn move_to(&mut self, byte: usize, select: bool) {
        if select {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret.byte)
            }
        } else {
            self.anchor = None
        }
        self.caret = Caret::at(self.boundary(byte));
    }
    fn prev(&self) -> usize {
        self.text()[..self.caret.byte]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i)
    }
    fn next(&self) -> usize {
        self.text()[self.caret.byte..]
            .graphemes(true)
            .next()
            .map_or(self.text().len(), |s| self.caret.byte + s.len())
    }
    fn word_left(&self) -> usize {
        self.text()[..self.caret.byte]
            .unicode_word_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }
    fn word_right(&self) -> usize {
        self.text()[self.caret.byte..]
            .unicode_word_indices()
            .next()
            .map_or(self.text().len(), |(i, s)| self.caret.byte + i + s.len())
    }
    fn replace(&mut self, range: Range<usize>, text: &str) {
        let removed = self.text()[range.clone()].to_string();
        let inserted = text.replace('\r', "");
        let inserted = if self.multiline {
            inserted
        } else {
            inserted.replace('\n', " ")
        };
        let next_len = self.text().len() - range.len() + inserted.len();
        if next_len > self.max_bytes && next_len > self.text().len() {
            self.limit_reached = true;
            return;
        }
        self.document.replace(range.clone(), &inserted);
        self.caret = Caret::at(range.start + inserted.len());
        self.anchor = None;
        self.history_bytes += removed.len() + inserted.len();
        for e in self.redo.drain(..) {
            self.history_bytes = self
                .history_bytes
                .saturating_sub(e.removed.len() + e.inserted.len())
        }
        self.undo.push_back(Undo {
            start: range.start,
            removed,
            inserted,
        });
        while self.undo.len() > 256 || self.history_bytes > 2 * 1024 * 1024 {
            if let Some(e) = self.undo.pop_front() {
                self.history_bytes = self
                    .history_bytes
                    .saturating_sub(e.removed.len() + e.inserted.len())
            } else {
                break;
            }
        }
    }
    fn insert(&mut self, text: &str) {
        self.replace(
            self.selection().unwrap_or(self.caret.byte..self.caret.byte),
            text,
        )
    }
    fn move_lines(&mut self, down: bool) {
        let range = self.selection().unwrap_or(self.caret.byte..self.caret.byte);
        let start = self.text()[..range.start].rfind('\n').map_or(0, |i| i + 1);
        let last = if range.end > range.start && self.text().as_bytes()[range.end - 1] == b'\n' {
            range.end - 1
        } else {
            range.end
        };
        let end = self.text()[last..]
            .find('\n')
            .map_or(self.text().len(), |i| last + i);
        let caret = self.caret.byte - start;
        let anchor = self.anchor.map(|a| a - start);
        let destination = if down && end < self.text().len() {
            let next_end = self.text()[end + 1..]
                .find('\n')
                .map_or(self.text().len(), |i| end + 1 + i);
            let following = self.text()[end + 1..next_end].to_owned();
            let text = format!("{}\n{}", following, &self.text()[start..end]);
            self.replace(start..next_end, &text);
            start + following.len() + 1
        } else if !down && start > 0 {
            let previous = self.text()[..start - 1].rfind('\n').map_or(0, |i| i + 1);
            let text = format!(
                "{}\n{}",
                &self.text()[start..end],
                &self.text()[previous..start - 1]
            );
            self.replace(previous..end, &text);
            previous
        } else {
            return;
        };
        self.caret = Caret::at((destination + caret).min(self.text().len()));
        self.anchor = anchor.map(|a| (destination + a).min(self.text().len()));
    }
    fn history(&mut self, redo: bool) {
        if redo {
            if let Some(e) = self.redo.pop() {
                self.document
                    .replace(e.start..e.start + e.removed.len(), &e.inserted);
                self.caret = Caret::at(e.start + e.inserted.len());
                self.undo.push_back(e)
            }
        } else if let Some(e) = self.undo.pop_back() {
            self.document
                .replace(e.start..e.start + e.inserted.len(), &e.removed);
            self.caret = Caret::at(e.start + e.removed.len());
            self.redo.push(e)
        }
        self.anchor = None;
    }
    fn reset_blink(&mut self, cx: &mut Update<'_, Self>) {
        self.caret_on = true;
        if cx.focused() && self.caret_blink {
            let _ = cx.after(self.blink, Duration::from_millis(530));
        }
        cx.repaint()
    }
    fn changed(&mut self, cx: &mut Update<'_, Self>, before: u64) {
        if std::mem::take(&mut self.limit_reached) {
            let _ = cx.emit(EditorOutput::LimitReached);
        }
        if self.decoration.is_some() {
            self.edited |= self.document.revision() != before;
            cx.request_frame();
        }
        if self.document.revision() != before {
            let _ = cx.emit(EditorOutput::Changed {
                revision: self.document.revision(),
                text: Arc::from(self.text()),
            });
            cx.relayout()
        }
        self.reveal_pending = true;
        self.state_pending = true;
        cx.request_frame();
        self.report_state(cx);
        self.reset_blink(cx)
    }
    fn drag_scrollbar(&mut self, cx: &mut Update<'_, Self>, position: Point, grab: f32) {
        if let (Some(thumb), Some(p)) = (self.scrollbar(cx.bounds()), &self.paragraph) {
            let max = (p.size.height + 2. * self.insets().y - cx.bounds().height).max(0.);
            let travel = (cx.bounds().height - thumb.height).max(1.);
            self.scroll.y = ((position.y - grab) / travel).clamp(0., 1.) * max;
            self.report_state(cx);
            cx.repaint();
        }
    }
    fn hit(&self, p: Point) -> Caret {
        self.paragraph.as_ref().map_or(Caret::at(0), |layout| {
            layout.hit(Point::new(
                p.x - self.insets().x + self.scroll.x,
                p.y - self.insets().y + self.scroll.y,
            ))
        })
    }
    fn reveal(&mut self, size: Size) {
        if let Some(p) = &self.paragraph {
            let point = p.caret_point(self.caret);
            let height = (size.height - 2. * self.insets().y).max(0.);
            let width = (size.width - 2. * self.insets().x).max(0.);
            if point.y < self.scroll.y {
                self.scroll.y = point.y
            }
            if point.y + p.line_height > self.scroll.y + height {
                self.scroll.y = point.y + p.line_height - height
            }
            if self.wrap {
                self.scroll.x = 0.
            } else {
                if point.x < self.scroll.x {
                    self.scroll.x = point.x
                }
                if point.x > self.scroll.x + width {
                    self.scroll.x = point.x - width
                }
            }
            self.scroll.y = self.scroll.y.clamp(0., (p.size.height - height).max(0.));
        }
    }
}
impl<D: Document> Widget for Editor<D> {
    type Command = Edit;
    type Output = EditorOutput;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: Edit) {
        let before = self.document.revision();
        match command {
            Edit::Restore(state) => {
                self.apply_state(state);
                cx.relayout();
                return;
            }
            Edit::Placeholder(text) => {
                self.placeholder = text;
                self.placeholder_layout = None;
                cx.relayout();
                return;
            }
            Edit::ReportCursor => {
                if let Some(rect) = self.ime_cursor() {
                    let _ = cx.emit(EditorOutput::Cursor(rect));
                }
                return;
            }
            Edit::Set(text) => {
                self.document.replace(0..self.text().len(), &text);
                self.caret = Caret::at(0);
                self.anchor = None;
                self.undo.clear();
                self.redo.clear();
                self.history_bytes = 0;
                self.scroll = Point::default();
                self.reveal_pending = true;
                self.preedit.clear();
                cx.relayout();
                self.reset_blink(cx);
                return;
            }
            Edit::Insert(text) => self.insert(&text),
            Edit::Select { anchor, caret } => {
                self.anchor = Some(self.boundary(anchor));
                self.caret = Caret::at(self.boundary(caret));
            }
            Edit::Undo => self.history(false),
            Edit::Redo => self.history(true),
            Edit::Wrap(wrap) => {
                if self.wrap == wrap {
                    return;
                }
                self.wrap = wrap;
                cx.relayout()
            }
        }
        self.changed(cx, before)
    }
    fn frame(&mut self, cx: &mut Update<'_, Self>, time: FrameTime) {
        if std::mem::take(&mut self.state_pending) {
            self.report_state(cx);
        }
        if self.drag.is_some() {
            let y = self.drag_position.y;
            let height = cx.bounds().height;
            let overflow = if y < 0. {
                y
            } else if y > height {
                y - height
            } else {
                0.
            };
            if overflow != 0. {
                if let Some(p) = &self.paragraph {
                    let max = (p.size.height + 2. * self.insets().y - height).max(0.);
                    self.scroll.y = (self.scroll.y
                        + overflow.clamp(-120., 120.) * 10. * time.elapsed.as_secs_f32().min(0.05))
                    .clamp(0., max);
                    self.caret = self.hit(self.drag_position);
                    self.report_state(cx);
                    cx.repaint();
                    cx.request_frame();
                }
            }
        }

        if let Some(mut decoration) = self.decoration.take() {
            let edited = std::mem::take(&mut self.edited);
            if let Some(view) = self.view(cx.bounds(), cx.focused()) {
                if decoration.frame(&view, time, edited) {
                    cx.request_frame();
                }
                cx.repaint();
            }
            self.decoration = Some(decoration);
        }
    }
    fn cursor(&self, _position: Point) -> Option<CursorIcon> {
        Some(CursorIcon::Text)
    }
    fn focusable(&self) -> bool {
        true
    }
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        match event {
            Lifecycle::Focus(true) => {
                let _ = cx.emit(EditorOutput::FocusChanged(true));
                self.reset_blink(cx);
                if self.decoration.is_some() {
                    cx.request_frame();
                }
            }
            Lifecycle::Focus(false) | Lifecycle::Visibility(false) | Lifecycle::Unmount => {
                if event == Lifecycle::Focus(false) {
                    let _ = cx.emit(EditorOutput::FocusChanged(false));
                }
                self.drag = None;
                self.preedit.clear();
                cx.cancel_timer(self.blink);
                cx.repaint()
            }
            Lifecycle::CaptureLost(pointer) => {
                if self.drag == Some(pointer) {
                    self.drag = None;
                }
                if self.scrollbar_drag.is_some_and(|(p, _)| p == pointer) {
                    self.scrollbar_drag = None;
                }
            }
            _ => {}
        }
    }
    fn timer(&mut self, cx: &mut Update<'_, Self>, timer: Timer) {
        if timer == self.blink && cx.focused() && self.caret_blink {
            self.caret_on = !self.caret_on;
            cx.repaint();
            let _ = cx.after(self.blink, Duration::from_millis(530));
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        self.theme = theme(cx);
        let style = TextStyle {
            size: self.theme.font_size,
            font: 0,
        };
        let inset = self.insets();
        let width = (self.wrap && self.multiline)
            .then_some((c.max.width - 2. * inset.x).max(0.))
            .filter(|w| w.is_finite());
        let revision = self.document.revision();
        let rebuild = self.paragraph.as_ref().is_none_or(|p| {
            p.revision != revision
                || p.style != style
                || p.width != width
                || p.service_revision != cx.text_revision()
        });
        if rebuild {
            self.paragraph = Some(cx.paragraph(TextRequest {
                text: Arc::from(self.text()),
                style,
                width,
                revision,
            }));
        }
        if self
            .placeholder_layout
            .as_ref()
            .is_none_or(|p| p.style != style || p.service_revision != cx.text_revision())
        {
            self.placeholder_layout = Some(cx.paragraph(TextRequest {
                text: self.placeholder.clone(),
                style,
                width,
                revision: 0,
            }));
        }
        self.preedit_layout = if self.preedit.is_empty() {
            None
        } else {
            Some(cx.paragraph(TextRequest {
                text: Arc::from(self.preedit.as_str()),
                style,
                width: None,
                revision: 0,
            }))
        };
        let p = self.paragraph.as_ref().unwrap();
        let desired = Size::new(
            if c.max.width.is_finite() {
                c.max.width
            } else {
                p.size.width + 2. * inset.x
            },
            if self.multiline && c.max.height.is_finite() {
                c.max.height
            } else {
                p.size.height + 2. * inset.y
            },
        );
        let size = c.constrain(desired);
        let baseline = (p.baseline + inset.y).min(size.height);
        let max_y = (p.size.height + 2. * inset.y - size.height).max(0.);
        self.scroll.y = self.scroll.y.min(max_y);
        if self.reveal_pending && !self.restore_pending {
            self.reveal(size);
        }
        self.reveal_pending = false;
        self.restore_pending = false;
        Metrics {
            size,
            baseline: Some(baseline),
        }
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if phase != Phase::Target {
            return;
        }
        let before = self.document.revision();
        match input {
            Input::Button {
                pointer,
                button: 1,
                down: true,
                position,
            } if self.scrollbar(cx.bounds()).is_some() && position.x >= cx.bounds().width - 12. => {
                let thumb = self.scrollbar(cx.bounds()).unwrap();
                let grab = if thumb.contains(*position) {
                    position.y - thumb.y
                } else {
                    thumb.height / 2.
                };
                self.scrollbar_drag = Some((*pointer, grab));
                let _ = cx.capture(*pointer);
                self.drag_scrollbar(cx, *position, grab);
                cx.stop();
                return;
            }
            Input::Pointer { pointer, position }
                if self.scrollbar_drag.is_some_and(|(p, _)| p == *pointer) =>
            {
                let grab = self.scrollbar_drag.unwrap().1;
                self.drag_scrollbar(cx, *position, grab);
                cx.stop();
                return;
            }
            Input::Button {
                pointer,
                button: 1,
                down: false,
                ..
            } if self.scrollbar_drag.is_some_and(|(p, _)| p == *pointer) => {
                self.scrollbar_drag = None;
                let _ = cx.release(*pointer);
                cx.stop();
                return;
            }
            Input::Modifiers(m) => {
                self.modifiers = *m;
                return;
            }
            Input::Button {
                pointer,
                button: 1,
                down: true,
                position,
            } => {
                let previous = self.caret.byte;
                self.caret = self.hit(*position);
                if self.modifiers.shift {
                    self.anchor = Some(self.anchor.unwrap_or(previous));
                } else {
                    self.anchor = Some(self.caret.byte);
                }
                self.drag = Some(*pointer);
                self.drag_position = *position;
                self.click_count = if self.last_click.is_some_and(|(time, p)| {
                    cx.now().saturating_sub(time) < Duration::from_millis(350)
                        && (p.x - position.x).abs() < 4.
                        && (p.y - position.y).abs() < 4.
                }) {
                    self.click_count % 3 + 1
                } else {
                    1
                };
                if self.click_count == 2 && !self.modifiers.shift {
                    if let Some((start, word)) =
                        self.text().unicode_word_indices().find(|(start, word)| {
                            *start <= self.caret.byte && *start + word.len() >= self.caret.byte
                        })
                    {
                        let end = start + word.len();
                        self.anchor = Some(start);
                        self.caret = Caret::at(end);
                    }
                } else if self.click_count == 3 && !self.modifiers.shift {
                    let start = self.text()[..self.caret.byte]
                        .rfind('\n')
                        .map_or(0, |i| i + 1);
                    let end = self.text()[self.caret.byte..]
                        .find('\n')
                        .map_or(self.text().len(), |i| self.caret.byte + i + 1);
                    self.anchor = Some(start);
                    self.caret = Caret::at(end);
                }
                self.last_click = Some((cx.now(), *position));
                let _ = cx.focus();
                let _ = cx.capture(*pointer);
            }
            Input::Pointer { pointer, position } if self.drag == Some(*pointer) => {
                self.drag_position = *position;
                self.caret = self.hit(*position);
                self.reveal(cx.bounds().size());
            }
            Input::Button {
                pointer,
                button: 1,
                down: false,
                ..
            } if self.drag == Some(*pointer) => {
                self.drag = None;
                let _ = cx.release(*pointer);
            }
            Input::Text { text, .. } => {
                self.insert(text);
                self.preedit.clear();
            }
            Input::Preedit { text, .. } => {
                self.preedit = text.clone();
                cx.relayout();
            }
            Input::Scroll { delta, .. } if self.multiline => {
                if let Some(p) = &self.paragraph {
                    let max = (p.size.height + 2. * self.insets().y - cx.bounds().height).max(0.);
                    self.scroll.y = (self.scroll.y - delta.y).clamp(0., max);
                    if !self.wrap {
                        self.scroll.x = (self.scroll.x - delta.x).max(0.)
                    }
                    self.report_state(cx);
                    cx.repaint();
                    cx.stop();
                }
                return;
            }
            Input::Key {
                key,
                down: true,
                modifiers: m,
                ..
            } => {
                let ctrl = m.command();
                match key {
                    Key::Character('a') if ctrl => {
                        self.anchor = Some(0);
                        self.caret = Caret::at(self.text().len())
                    }
                    Key::Character('c') if ctrl => {
                        if let Some(r) = self.selection() {
                            let _ = cx.copy(self.text()[r].into());
                        }
                    }
                    Key::Character('x') if ctrl => {
                        if let Some(r) = self.selection() {
                            if cx.copy(self.text()[r.clone()].into()).is_ok() {
                                self.replace(r, "")
                            }
                        }
                    }
                    Key::Character('v') if ctrl => {
                        let _ = cx.paste();
                    }
                    Key::Character('z') if ctrl => self.history(m.shift),
                    Key::Character('y') if ctrl => self.history(true),
                    Key::Left => {
                        let byte = if !m.shift && self.selection().is_some() {
                            self.selection().unwrap().start
                        } else if ctrl {
                            self.word_left()
                        } else {
                            self.prev()
                        };
                        self.move_to(byte, m.shift)
                    }
                    Key::Right => {
                        let byte = if !m.shift && self.selection().is_some() {
                            self.selection().unwrap().end
                        } else if ctrl {
                            self.word_right()
                        } else {
                            self.next()
                        };
                        self.move_to(byte, m.shift)
                    }
                    Key::Home | Key::End => {
                        let byte = if ctrl {
                            if *key == Key::Home {
                                0
                            } else {
                                self.text().len()
                            }
                        } else {
                            let start = self.text()[..self.caret.byte]
                                .rfind('\n')
                                .map_or(0, |i| i + 1);
                            if *key == Key::Home {
                                start
                            } else {
                                self.text()[self.caret.byte..]
                                    .find('\n')
                                    .map_or(self.text().len(), |i| self.caret.byte + i)
                            }
                        };
                        self.move_to(byte, m.shift)
                    }
                    Key::Up | Key::Down if m.alt && self.multiline => {
                        self.move_lines(*key == Key::Down)
                    }
                    Key::Up | Key::Down | Key::PageUp | Key::PageDown if self.multiline => {
                        if let Some(p) = &self.paragraph {
                            let point = p.caret_point(self.caret);
                            let step = if matches!(key, Key::PageUp | Key::PageDown) {
                                cx.bounds().height - 2. * self.insets().y
                            } else {
                                p.line_height
                            };
                            let y = point.y
                                + if matches!(key, Key::Up | Key::PageUp) {
                                    -step
                                } else {
                                    step
                                };
                            let caret = p.hit(Point::new(point.x, y));
                            self.move_to(caret.byte, m.shift)
                        }
                    }
                    Key::Backspace => {
                        let r = self.selection().unwrap_or(
                            (if ctrl { self.word_left() } else { self.prev() })..self.caret.byte,
                        );
                        if !r.is_empty() {
                            self.replace(r, "")
                        }
                    }
                    Key::Delete => {
                        let r = self.selection().unwrap_or(
                            self.caret.byte..if ctrl { self.word_right() } else { self.next() },
                        );
                        if !r.is_empty() {
                            self.replace(r, "")
                        }
                    }
                    Key::Enter => {
                        if self.multiline {
                            self.insert("\n")
                        } else {
                            let _ = cx.emit(EditorOutput::Submitted);
                        }
                    }
                    Key::Tab if self.multiline && !ctrl => self.insert("\t"),
                    Key::Escape if self.selection().is_some() || !self.preedit.is_empty() => {
                        self.anchor = None;
                        self.preedit.clear();
                        cx.relayout()
                    }
                    _ => return,
                }
            }
            _ => return,
        }
        self.reveal(cx.bounds().size());
        self.changed(cx, before);
        cx.stop();
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = cx
            .environment::<Theme>()
            .cloned()
            .unwrap_or_else(|| self.theme.clone());
        let inset = self.insets();
        cx.painter.rect(cx.bounds, t.radius, t.background.into());
        if self.chrome {
            cx.painter.stroke(
                cx.bounds.inset(0.5),
                t.radius,
                1.,
                if cx.focused { t.accent } else { t.border },
            )
        }
        cx.painter.save();
        cx.painter.clip(if self.chrome {
            cx.bounds.inset(inset.x.min(inset.y))
        } else {
            cx.bounds
        });
        cx.painter.transform(Transform::translate(
            inset.x - self.scroll.x,
            inset.y - self.scroll.y,
        ));
        if let Some(p) = &self.paragraph {
            if let (Some(decoration), Some(view)) =
                (&self.decoration, self.view(cx.bounds, cx.focused))
            {
                decoration.paint(&view, EditorLayer::BehindText, cx.painter);
            }
            if let Some(selection) = self.selection() {
                for rect in p.selection_in(
                    selection,
                    Rect::new(
                        self.scroll.x - inset.x,
                        self.scroll.y - inset.y,
                        cx.bounds.width,
                        cx.bounds.height,
                    ),
                ) {
                    cx.painter.rect(rect, 0., t.selection.into())
                }
            }
            if self.text().is_empty() {
                if let Some(placeholder) = &self.placeholder_layout {
                    cx.painter
                        .paragraph(placeholder, Point::default(), t.muted.into())
                }
            } else {
                cx.painter
                    .paragraph(p, Point::default(), t.foreground.into())
            }
            if let (Some(decoration), Some(view)) =
                (&self.decoration, self.view(cx.bounds, cx.focused))
            {
                decoration.paint(&view, EditorLayer::AboveText, cx.painter);
            }
            if cx.focused {
                let point = p.caret_point(self.caret);
                if self.caret_on {
                    cx.painter.rect(
                        Rect::new(point.x, point.y, 1.5, p.line_height),
                        0.,
                        t.accent.into(),
                    )
                }
                if let Some(preedit) = &self.preedit_layout {
                    cx.painter.paragraph(preedit, point, t.accent.into());
                    cx.painter.rect(
                        Rect::new(
                            point.x,
                            point.y + p.line_height - 1.,
                            preedit.size.width,
                            1.,
                        ),
                        0.,
                        t.accent.into(),
                    )
                }
            }
        }
        cx.painter.restore();
        if let Some(thumb) = self.scrollbar(cx.bounds) {
            cx.painter.rect(
                Rect::new(thumb.x + 3., thumb.y, 6., thumb.height),
                3.,
                Color::hex(0xffffff)
                    .alpha(if cx.hovered { 0.35 } else { 0.20 })
                    .into(),
            );
        }
    }
    fn ime_cursor(&self) -> Option<Rect> {
        let p = self.paragraph.as_ref()?;
        let point = p.caret_point(self.caret);
        Some(Rect::new(
            point.x + self.insets().x - self.scroll.x,
            point.y + self.insets().y - self.scroll.y,
            1.5,
            p.line_height,
        ))
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::TextInput,
            label: self.placeholder.to_string(),
            value: Some(self.text().into()),
            ..Semantics::default()
        }
    }
}
