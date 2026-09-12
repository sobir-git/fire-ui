use crate::{theme, Theme};
use fire_ui::*;
use std::{ops::Range, sync::Arc, time::Duration};
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
    /// Edits in application order; each start addresses the document state
    /// its edit applied to, so undo replays in reverse and redo forward.
    edits: UndoEdits,
    /// Ordinary typing/deletion derives its collapsed selections from the edit.
    /// Preserve explicit selections only when the operation needs them.
    selection: Option<Box<UndoSelection>>,
}
enum UndoEdits {
    Single(UndoEdit),
    Multiple(Box<[UndoEdit]>),
}
impl UndoEdits {
    fn as_slice(&self) -> &[UndoEdit] {
        match self {
            Self::Single(edit) => std::slice::from_ref(edit),
            Self::Multiple(edits) => edits,
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
struct UndoSelection {
    /// Selection after the change applied, restored by redo.
    caret: Caret,
    anchor: Option<usize>,
    /// Selection before the change, restored by undo.
    before_caret: Caret,
    before_anchor: Option<usize>,
}
impl Undo {
    fn selection(&self) -> UndoSelection {
        self.selection.as_deref().copied().unwrap_or_else(|| {
            let edit = &self.edits.as_slice()[0];
            UndoSelection {
                before_caret: Caret::at(edit.start + edit.removed_len),
                before_anchor: None,
                caret: Caret::at(edit.start + edit.inserted().len()),
                anchor: None,
            }
        })
    }
    fn set_selection(&mut self, selection: UndoSelection) {
        self.selection = None;
        if selection != self.selection() {
            self.selection = Some(Box::new(selection));
        }
    }
}
struct UndoEdit {
    start: usize,
    removed_len: usize,
    text: Box<str>,
}

#[cfg(test)]
mod history_cost_tests {
    use super::*;

    #[test]
    fn plain_typing_retains_inline_edits_without_selection_allocations() {
        let mut editor = Editor::new("");
        for _ in 0..1_000 {
            editor.insert("x");
        }
        assert_eq!(editor.undo.len(), 1_000);
        assert!(editor
            .undo
            .iter()
            .all(|e| matches!(e.edits, UndoEdits::Single(_)) && e.selection.is_none()));
        assert!(std::mem::size_of::<Undo>() <= 48);
        eprintln!(
            "Plain editor: {} bytes fixed history metadata/edit, no per-edit metadata allocation",
            std::mem::size_of::<Undo>()
        );
        for _ in 0..1_000 {
            editor.history(false);
        }
        assert_eq!(editor.text(), "");
        assert_eq!(editor.caret.byte, 0);
        for _ in 0..1_000 {
            editor.history(true);
        }
        assert_eq!(editor.text().len(), 1_000);
        assert_eq!(editor.caret.byte, 1_000);
    }
}
impl UndoEdit {
    fn removed(&self) -> &str {
        &self.text[..self.removed_len]
    }
    fn inserted(&self) -> &str {
        &self.text[self.removed_len..]
    }
}
/// Map a byte offset through edits given in range order: offsets before an
/// edit shift by its delta, offsets inside an edit collapse to its start.
fn map_byte(byte: usize, edits: &[TextEdit]) -> usize {
    let mut shift: isize = 0;
    for edit in edits {
        if byte >= edit.range.end {
            shift += edit.text.len() as isize - edit.range.len() as isize;
        } else if byte > edit.range.start {
            return (edit.range.start as isize + shift).max(0) as usize;
        } else {
            break;
        }
    }
    (byte as isize + shift).max(0) as usize
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
/// One replacement within a [`Transaction`]. Ranges address the document as
/// the extension saw it, before any edit of the same transaction applies.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TextEdit {
    range: Range<usize>,
    text: String,
}
/// A validated document change an extension applies through the editor's
/// normal path: size limits, history, selection and change notifications.
///
/// Edits are applied in range order, later offsets adjusted by earlier ones,
/// and the whole transaction either applies or is rejected atomically. One
/// transaction is one undo step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    edits: Vec<TextEdit>,
    selection: Selection,
    scroll: ScrollPolicy,
}
/// Where the caret and selection go after a [`Transaction`] applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Selection {
    /// Map the current caret and selection through the edits.
    #[default]
    Map,
    /// Collapse to a byte offset in the resulting text, clearing the selection.
    At(usize),
}
/// Whether a [`Transaction`] may scroll the caret into view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScrollPolicy {
    /// Keep the viewport exactly where it is.
    #[default]
    Stay,
    /// Reveal the resulting caret, like a user edit.
    Reveal,
}
impl Transaction {
    /// Replace one source range. The caret and selection map through the edit,
    /// the change is one undo step, and the viewport stays put.
    pub fn replace(range: Range<usize>, text: impl Into<String>) -> Self {
        Self {
            edits: vec![TextEdit {
                range,
                text: text.into(),
            }],
            selection: Selection::Map,
            scroll: ScrollPolicy::Stay,
        }
    }
    /// Add another replacement. Ranges address the same pre-transaction
    /// document and must not overlap.
    pub fn edit(mut self, range: Range<usize>, text: impl Into<String>) -> Self {
        self.edits.push(TextEdit {
            range,
            text: text.into(),
        });
        self
    }
    /// Collapse to a byte offset in the resulting text, clearing the selection.
    pub fn caret(mut self, at: usize) -> Self {
        self.selection = Selection::At(at);
        self
    }
    /// Reveal the resulting caret, like a user edit.
    pub fn reveal(mut self) -> Self {
        self.scroll = ScrollPolicy::Reveal;
        self
    }
}
/// An interactive region an extension contributes, in paragraph coordinates.
/// The editor claims presses inside it, shows the pointer cursor, and offers
/// activation on release inside, keyboard shortcuts and assistive technology.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    /// Extension-chosen identity, passed back to [`EditorExtension::activate`]
    /// and published as the semantic child's `key`.
    pub id: u64,
    pub bounds: Rect,
}
/// A press on a target. Release activates it; moving beyond the click
/// threshold converts it to ordinary text selection from the press location.
#[derive(Clone, Copy, Debug)]
struct Armed {
    extension: usize,
    target: u64,
    bounds: Rect,
    inside: bool,
    pointer: u32,
    origin: Point,
}
/// Whether a presented range currently shows its source text: the caret or
/// the selection intersects it, so editing there must see real characters.
/// Extensions apply the same rule when painting over a presented range.
pub fn revealed(range: Range<usize>, caret: Caret, selection: Option<Range<usize>>) -> bool {
    let editing = caret.byte > range.start && caret.byte < range.end;
    let selecting = selection.is_some_and(|s| s.start < range.end && s.end > range.start);
    editing || selecting
}
/// A composable editor capability. Extensions form a stack: the first added
/// paints first, behind later ones, while input consults the stack from the
/// top down, so the topmost extension claiming a pointer position owns it.
///
/// An extension may contribute any combination of presented source ranges,
/// interactive targets, visual effects and semantics. Structure is derived
/// in [`sync`](EditorExtension::sync), after layout and before any
/// interaction or paint; [`frame`](EditorExtension::frame) only animates.
pub trait EditorExtension: 'static {
    /// Re-derive structure from the current text and layout.
    fn sync(&mut self, _view: &EditorView<'_>) {}
    /// Source ranges to present as extension visuals instead of text glyphs.
    /// Suppressed ranges keep their laid-out space; the editor reveals the
    /// raw characters while the caret or selection intersects a range.
    fn presentation(&self, _view: &EditorView<'_>, _present: &mut dyn FnMut(Range<usize>)) {}
    /// Interactive regions for the current sync, in paragraph coordinates.
    fn targets(&self, _view: &EditorView<'_>) -> Vec<Target> {
        Vec::new()
    }
    /// Activate a target previously offered by
    /// [`targets`](EditorExtension::targets): a pointer release inside it or
    /// an assistive-technology activation. A returned transaction applies
    /// through the editor's normal path; the interaction is consumed either way.
    fn activate(&mut self, _view: &EditorView<'_>, _target: u64) -> Option<Transaction> {
        None
    }
    /// A key press before the editor's own handling. A returned transaction
    /// replaces the default behavior for that key. Only plain keys reach
    /// extensions, never composition.
    fn key(
        &mut self,
        _view: &EditorView<'_>,
        _key: Key,
        _modifiers: Modifiers,
    ) -> Option<Transaction> {
        None
    }
    /// Animate. `inserted` reports committed text insertion through the
    /// editor's input path since the last frame, excluding line breaks,
    /// extension transactions and undo/redo. Returning true schedules another frame.
    fn frame(&mut self, _view: &EditorView<'_>, _time: FrameTime, _inserted: bool) -> bool {
        false
    }
    /// Bounds of all extension pixels in paragraph coordinates, so the editor
    /// can invalidate exactly what the extension paints. None means the
    /// extension paints nothing in this state.
    fn damage(&self, _view: &EditorView<'_>) -> Option<Rect> {
        None
    }
    /// Paint visual effects for a layer.
    fn paint(&self, _view: &EditorView<'_>, _layer: EditorLayer, _painter: &mut dyn Painter) {}
    /// Semantics for a published target, for assistive technology. The `key`
    /// should identify the target; it routes activation back here.
    fn semantics(&self, _view: &EditorView<'_>, _target: u64) -> Option<Semantics> {
        None
    }
}
/// A retained editor. Edits are public commands, while pointer/key input uses the same operations.
pub struct Editor<D: Document = StringDocument> {
    document: D,
    caret: Caret,
    anchor: Option<usize>,
    paragraph: Option<Arc<Paragraph>>,
    // Shared by output consumers and layout; hidden editors retain no extra text owner.
    snapshot: Option<(u64, Arc<str>)>,
    visible: bool,
    theme: Theme,
    scroll: Point,
    wrap: bool,
    multiline: bool,
    placeholder: Arc<str>,
    placeholder_layout: Option<Arc<Paragraph>>,
    undo: Vec<Undo>,
    redo: Vec<Undo>,
    blink: Timer,
    caret_on: bool,
    caret_blink: bool,
    max_bytes: usize,
    limit_reached: bool,
    drag: Option<u32>,
    drag_position: Point,
    state_pending: bool,
    preedit: String,
    preedit_selection: Option<(usize, usize)>,
    label: Option<String>,
    key: Option<String>,
    composition_layout: Option<Arc<Paragraph>>,
    last_click: Option<(Duration, Point)>,
    click_count: u8,
    modifiers: Modifiers,
    chrome: bool,
    padding: Option<Point>,
    extensions: Vec<Box<dyn EditorExtension>>,
    /// Source ranges currently presented as extension visuals, collected at
    /// the last sync. Text glyphs inside them are suppressed unless revealed.
    presented: Vec<Range<usize>>,
    /// A target the pointer pressed but has not yet activated.
    armed: Option<Armed>,
    /// Whether the editor holds keyboard focus, for extension sync.
    focused: bool,
    inserted: bool,
    reveal_pending: bool,
    restore_pending: bool,
    scrollbar_drag: Option<(u32, f32)>,
    /// Viewport width and scrollbar strip from the last layout, for cursor shape.
    width: f32,
    strip: f32,
    /// Viewport height from the last layout, for cursor shape over decorations.
    height: f32,
    /// Whether the pointer is resting on the scrollbar.
    bar_hover: bool,
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
            snapshot: None,
            visible: true,
            theme: Theme::default(),
            scroll: Point::default(),
            wrap: true,
            multiline: true,
            placeholder: Arc::from(""),
            placeholder_layout: None,
            undo: Vec::new(),
            redo: vec![],
            blink: Timer::new(),
            caret_on: true,
            caret_blink: true,
            max_bytes: usize::MAX,
            limit_reached: false,
            drag: None,
            drag_position: Point::default(),
            state_pending: false,
            preedit: String::new(),
            preedit_selection: None,
            label: None,
            key: None,
            composition_layout: None,
            last_click: None,
            click_count: 0,
            modifiers: Modifiers::default(),
            chrome: true,
            padding: None,
            extensions: Vec::new(),
            presented: Vec::new(),
            armed: None,
            focused: false,
            inserted: false,
            reveal_pending: true,
            restore_pending: false,
            scrollbar_drag: None,
            width: 0.,
            strip: 0.,
            height: 0.,
            bar_hover: false,
        }
    }
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
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
    /// The editor's own bar, using the same geometry as every other scrolling
    /// surface so the three read as one control.
    fn scrollbar(&self, bounds: Rect) -> Option<crate::Scrollbar> {
        if !self.multiline {
            return None;
        }
        let total = self.paragraph.as_ref()?.size.height + 2. * self.insets().y;
        crate::Scrollbar::new(
            bounds,
            bounds.height,
            total,
            self.scroll.y,
            crate::Scrollbar::width(&self.theme),
        )
    }
    pub fn padding(mut self, horizontal: f32, vertical: f32) -> Self {
        assert!(
            horizontal.is_finite() && vertical.is_finite() && horizontal >= 0. && vertical >= 0.
        );
        self.padding = Some(Point::new(horizontal, vertical));
        self
    }
    /// Append an extension to the editor's stack. The first extension added
    /// paints first, behind later ones; input consults the stack from the top.
    pub fn extension(mut self, extension: impl EditorExtension) -> Self {
        self.extensions.push(Box::new(extension));
        self
    }
    fn insets(&self) -> Point {
        self.padding
            .unwrap_or(Point::new(self.theme.scale.inset, self.theme.scale.inset))
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
    /// A widget-local pointer position in paragraph coordinates.
    fn paragraph_point(&self, position: Point) -> Point {
        Point::new(
            position.x - self.insets().x + self.scroll.x,
            position.y - self.insets().y + self.scroll.y,
        )
    }
    /// Claim a press for a target at this widget-local position, arming it for
    /// activation on release inside. Shift presses fall through to selection.
    fn arm_target(&mut self, position: Point, pointer: u32, focused: bool) -> bool {
        // Compositions own the text: no target arming while one is active.
        if self.modifiers.shift || !self.preedit.is_empty() {
            return false;
        }
        let Some(paragraph) = self.paragraph.clone() else {
            return false;
        };
        let view = EditorView {
            paragraph: &paragraph,
            caret: self.caret,
            selection: self.selection(),
            viewport: Rect::new(self.scroll.x, self.scroll.y, self.width, self.height),
            focused,
        };
        let at = self.paragraph_point(position);
        for (index, extension) in self.extensions.iter().enumerate().rev() {
            if let Some(target) = extension
                .targets(&view)
                .into_iter()
                .find(|t| t.bounds.contains(at))
            {
                self.armed = Some(Armed {
                    extension: index,
                    target: target.id,
                    bounds: target.bounds,
                    inside: true,
                    pointer,
                    origin: position,
                });
                return true;
            }
        }
        false
    }
    /// Track an armed target as the pointer moves. Leaving the bounds disarms
    /// the pending activation; sliding back in re-arms it, like any press-and-
    /// hold button. Release outside (or capture loss) cancels for good.
    fn arm_move(&mut self, position: Point, pointer: u32) {
        if self.armed.as_ref().is_none_or(|a| a.pointer != pointer) {
            return;
        }
        let point = self.paragraph_point(position);
        if let Some(armed) = &mut self.armed {
            armed.inside = armed.bounds.contains(point);
        }
    }
    /// Release an armed target: activate inside, cancel outside. Either way
    /// the press is consumed and the target disarmed.
    fn arm_release(&mut self, pointer: u32, focused: bool) -> Option<Transaction> {
        let armed = self.armed.take()?;
        if armed.pointer != pointer {
            return None;
        }
        if !armed.inside {
            return None;
        }
        let paragraph = self.paragraph.clone()?;
        let view = EditorView {
            paragraph: &paragraph,
            caret: self.caret,
            selection: self.selection(),
            viewport: Rect::new(self.scroll.x, self.scroll.y, self.width, self.height),
            focused,
        };
        self.extensions
            .get_mut(armed.extension)?
            .activate(&view, armed.target)
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
    /// How far the view has scrolled from the top, in pixels.
    pub fn scroll_offset(&self) -> f32 {
        self.scroll.y
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
    /// Apply a transaction through the editor's normal path. All edits apply
    /// or none do; the change is one undo step. Returns false when the
    /// transaction is rejected: out-of-bounds or overlapping ranges, or the
    /// size limit.
    fn apply(&mut self, transaction: Transaction) -> bool {
        let mut edits = transaction.edits;
        if edits.is_empty() {
            return false;
        }
        let before_caret = self.caret;
        let before_anchor = self.anchor;
        // Extension transactions meet the same text policy as typed input.
        for edit in &mut edits {
            if edit.text.contains('\r') || (!self.multiline && edit.text.contains('\n')) {
                let mut text = edit.text.replace('\r', "");
                if !self.multiline {
                    text = text.replace('\n', " ");
                }
                edit.text = text;
            }
        }
        edits.sort_by_key(|e| e.range.start);
        let text = self.text();
        for edit in &edits {
            if edit.range.start > edit.range.end
                || edit.range.end > text.len()
                || !text.is_char_boundary(edit.range.start)
                || !text.is_char_boundary(edit.range.end)
            {
                debug_assert!(false, "transaction range out of bounds or split");
                return false;
            }
        }
        if let Some(overlap) = edits.windows(2).find(|w| w[0].range.end > w[1].range.start) {
            debug_assert!(false, "transaction edits overlap");
            let _ = overlap;
            return false;
        }
        let removed_total: usize = edits.iter().map(|e| e.range.len()).sum();
        let added_total: usize = edits.iter().map(|e| e.text.len()).sum();
        let next_len = text.len() - removed_total + added_total;
        if next_len > self.max_bytes && next_len > text.len() {
            self.limit_reached = true;
            return false;
        }
        let mut delta: isize = 0;
        let mut record_edit = |edit: &TextEdit| {
            let start = (edit.range.start as isize + delta).max(0) as usize;
            let end = start + edit.range.len();
            let removed = &self.text()[start..end];
            let removed_len = removed.len();
            let mut record = String::with_capacity(removed_len + edit.text.len());
            record.push_str(removed);
            record.push_str(&edit.text);
            let undo = UndoEdit {
                start,
                removed_len,
                text: record.into_boxed_str(),
            };
            self.document.replace(start..end, &edit.text);
            delta += edit.text.len() as isize - edit.range.len() as isize;
            undo
        };
        let undo = if edits.len() == 1 {
            UndoEdits::Single(record_edit(&edits[0]))
        } else {
            UndoEdits::Multiple(edits.iter().map(record_edit).collect())
        };
        match transaction.selection {
            Selection::Map => {
                self.caret = Caret {
                    byte: self.boundary(map_byte(self.caret.byte, &edits)),
                    affinity: self.caret.affinity,
                };
                self.anchor = self.anchor.map(|a| self.boundary(map_byte(a, &edits)));
            }
            Selection::At(at) => {
                self.caret = Caret::at(self.boundary(at));
                self.anchor = None;
            }
        }
        self.redo.clear();
        let mut record = Undo {
            edits: undo,
            selection: None,
        };
        record.set_selection(UndoSelection {
            caret: self.caret,
            anchor: self.anchor,
            before_caret,
            before_anchor,
        });
        self.undo.push(record);
        true
    }
    /// The editor's own single-edit path for user input: the caret collapses
    /// to the end of the inserted text, like any typed change.
    fn replace(&mut self, range: Range<usize>, text: &str) {
        let inserted = text.replace('\r', "");
        let inserted = if self.multiline {
            inserted
        } else {
            inserted.replace('\n', " ")
        };
        let collapse = range.start + inserted.len();
        self.apply(Transaction::replace(range, inserted).caret(collapse));
    }
    fn insert(&mut self, text: &str) {
        let before = self.document.revision();
        self.replace(
            self.selection().unwrap_or(self.caret.byte..self.caret.byte),
            text,
        );
        self.inserted |=
            self.document.revision() != before && text.chars().any(|ch| ch != '\n' && ch != '\r');
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
        // The move adjustment above happens after the edit was recorded; keep
        // the record's post-change selection in step so redo restores it.
        if let Some(e) = self.undo.last_mut() {
            e.set_selection(UndoSelection {
                caret: self.caret,
                anchor: self.anchor,
                ..e.selection()
            });
        }
    }
    fn history(&mut self, redo: bool) {
        if redo {
            if let Some(e) = self.redo.pop() {
                for edit in e.edits.as_slice() {
                    self.document.replace(
                        edit.start..edit.start + edit.removed().len(),
                        edit.inserted(),
                    );
                }
                let selection = e.selection();
                self.caret = selection.caret;
                self.anchor = selection.anchor;
                self.undo.push(e)
            }
        } else if let Some(e) = self.undo.pop() {
            for edit in e.edits.as_slice().iter().rev() {
                self.document.replace(
                    edit.start..edit.start + edit.inserted().len(),
                    edit.removed(),
                );
            }
            // Undo returns the selection to the state before the change, so
            // toggling a checkbox or deleting a selection never drags the
            // caret into the edit.
            let selection = e.selection();
            self.caret = selection.before_caret;
            self.anchor = selection.before_anchor;
            self.redo.push(e)
        }
    }
    fn clear_preedit(&mut self) {
        self.preedit.clear();
        self.preedit_selection = None;
        self.composition_layout = None;
    }
    fn composition_start(&self) -> usize {
        self.selection().map_or(self.caret.byte, |r| r.start)
    }
    fn reset_blink(&mut self, cx: &mut Update<'_, Self>) {
        self.caret_on = true;
        if cx.focused() && self.caret_blink {
            let _ = cx.after(self.blink, Duration::from_millis(530));
        }
        cx.repaint()
    }
    fn text_snapshot(&mut self) -> Arc<str> {
        let revision = self.document.revision();
        if let Some((cached_revision, text)) = &self.snapshot {
            if *cached_revision == revision {
                return text.clone();
            }
        }
        self.snapshot = None;
        let text: Arc<str> = Arc::from(self.text());
        self.snapshot = self.visible.then(|| (revision, text.clone()));
        text
    }
    fn changed(&mut self, cx: &mut Update<'_, Self>, before: u64, reveal: bool) {
        if std::mem::take(&mut self.limit_reached) {
            let _ = cx.emit(EditorOutput::LimitReached);
        }
        if !self.extensions.is_empty() {
            cx.request_frame();
        }
        if self.document.revision() != before {
            let _ = cx.emit(EditorOutput::Changed {
                revision: self.document.revision(),
                text: self.text_snapshot(),
            });
            cx.relayout()
        }
        // Extensions decide their own scroll policy: a toggle that keeps the
        // viewport put must not drag the caret into view on the next layout.
        if reveal {
            self.reveal_pending = true;
        }
        self.state_pending = true;
        cx.request_frame();
        self.report_state(cx);
        self.reset_blink(cx)
    }
    fn drag_scrollbar(&mut self, cx: &mut Update<'_, Self>, position: Point, grab: f32) {
        if let (Some(bar), Some(p)) = (self.scrollbar(cx.bounds()), &self.paragraph) {
            let thumb = bar.thumb;
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
    /// Paint the paragraph, leaving out glyphs inside presented ranges unless
    /// the caret or selection reveals them. Each visible stretch of a line is
    /// painted under its own clip, so suppressed glyphs never reach pixels.
    fn paint_text(&self, p: &Paragraph, cx: &mut Paint<'_>, brush: Brush) {
        let top = self.scroll.y - self.insets().y;
        let bottom = top + cx.bounds.height;
        let first = p.lines.partition_point(|l| l.y + p.line_height < top);
        let end = p.lines.partition_point(|l| l.y <= bottom);
        if first >= end {
            return;
        }
        let visible = p.lines[first].range.start..p.lines[end - 1].range.end;
        let mut hidden: Vec<Vec<Range<f32>>> = vec![Vec::new(); end - first];
        for range in &self.presented {
            if range.end <= visible.start || range.start >= visible.end {
                continue;
            }
            if revealed(range.clone(), self.caret, self.selection()) {
                continue;
            }
            for fragment in
                p.range_fragments(range.start.max(visible.start)..range.end.min(visible.end))
            {
                let line = p.lines.partition_point(|l| l.y < fragment.y);
                if line >= first && line < end {
                    hidden[line - first].push(fragment.x);
                }
            }
        }
        if hidden.iter().all(|h| h.is_empty()) {
            cx.painter.paragraph(p, Point::default(), brush);
            return;
        }
        for (line, spans) in hidden.iter().enumerate() {
            let line = line + first;
            let mut spans = spans.clone();
            spans.sort_by(|a, b| a.start.total_cmp(&b.start));
            let mut edge = 0.;
            for span in spans {
                if span.start > edge {
                    self.paint_text_span(p, cx, brush, line, edge..span.start);
                }
                edge = edge.max(span.end);
            }
            let width = p.lines[line].width;
            if edge < width {
                self.paint_text_span(p, cx, brush, line, edge..width);
            }
        }
    }
    /// Paint one visible stretch of a line: the whole paragraph under a clip
    /// that keeps only this x-range of this row.
    fn paint_text_span(
        &self,
        p: &Paragraph,
        cx: &mut Paint<'_>,
        brush: Brush,
        line: usize,
        x: Range<f32>,
    ) {
        cx.painter.save();
        cx.painter.clip(Rect::new(
            x.start,
            p.lines[line].y,
            (x.end - x.start).max(0.),
            p.line_height,
        ));
        cx.painter.paragraph(p, Point::default(), brush);
        cx.painter.restore();
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
                self.scroll = Point::default();
                self.reveal_pending = true;
                self.clear_preedit();
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
        self.reveal(cx.bounds().size());
        self.changed(cx, before, true)
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

        let mut extensions = std::mem::take(&mut self.extensions);
        let inserted = std::mem::take(&mut self.inserted);
        if let Some(view) = self.view(cx.bounds(), cx.focused()) {
            let mut damage: Option<Rect> = None;
            for extension in extensions.iter_mut() {
                let before = extension.damage(&view);
                if extension.frame(&view, time, inserted) {
                    cx.request_frame();
                }
                let after = extension.damage(&view);
                // Pixels the extension owned before and after this frame both
                // need invalidation; None simply means it paints nothing in
                // that state.
                let region = match (before, after) {
                    (Some(a), Some(b)) => Some(a.union(b)),
                    (Some(a), None) | (None, Some(a)) => Some(a),
                    (None, None) => None,
                }
                .filter(|r| r.width > 0. && r.height > 0.);
                if let Some(region) = region {
                    damage = Some(damage.map_or(region, |d| d.union(region)));
                }
            }
            if let Some(damage) = damage {
                cx.repaint_rect(Rect::new(
                    damage.x + self.insets().x - self.scroll.x,
                    damage.y + self.insets().y - self.scroll.y,
                    damage.width,
                    damage.height,
                ));
            }
        }
        self.extensions = extensions;
    }
    fn cursor(&self, position: Point) -> Option<CursorIcon> {
        // Over its own scrollbar the editor is a scrolling surface, not a text field.
        if position.x >= self.width - self.strip {
            return Some(CursorIcon::Arrow);
        }
        if let Some(view) = self.view(
            Rect::new(self.scroll.x, self.scroll.y, self.width, self.height),
            false,
        ) {
            let at = self.paragraph_point(position);
            for extension in self.extensions.iter().rev() {
                if extension
                    .targets(&view)
                    .iter()
                    .any(|t| t.bounds.contains(at))
                {
                    return Some(CursorIcon::Pointer);
                }
            }
        }
        Some(CursorIcon::Text)
    }
    fn focusable(&self) -> bool {
        true
    }
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        if let Lifecycle::Visibility(visible) = event {
            self.visible = visible;
        }
        match event {
            Lifecycle::Focus(true) => {
                self.focused = true;
                let _ = cx.emit(EditorOutput::FocusChanged(true));
                self.reset_blink(cx);
                if !self.extensions.is_empty() {
                    cx.request_frame();
                }
            }
            Lifecycle::Focus(false) | Lifecycle::Visibility(false) | Lifecycle::Unmount => {
                if event == Lifecycle::Focus(false) {
                    self.focused = false;
                    let _ = cx.emit(EditorOutput::FocusChanged(false));
                }
                if event == Lifecycle::Visibility(false) || event == Lifecycle::Unmount {
                    self.paragraph = None;
                    self.snapshot = None;
                    self.composition_layout = None;
                    cx.relayout();
                }
                self.drag = None;
                self.clear_preedit();
                cx.cancel_timer(self.blink);
                cx.repaint()
            }
            Lifecycle::Visibility(true) => {
                cx.relayout();
            }
            Lifecycle::CaptureLost(pointer) => {
                if self.drag == Some(pointer) {
                    self.drag = None;
                }
                if self.scrollbar_drag.is_some_and(|(p, _)| p == pointer) {
                    self.scrollbar_drag = None;
                }
                // Losing capture cancels a pending target activation: without
                // the release event the target would stay armed forever.
                if self.armed.is_some_and(|a| a.pointer == pointer) {
                    self.armed = None;
                }
            }
            _ => {}
        }
    }
    fn timer(&mut self, cx: &mut Update<'_, Self>, timer: Timer) {
        if timer == self.blink && cx.focused() && self.caret_blink {
            self.caret_on = !self.caret_on;
            if let Some(rect) = self.ime_cursor() {
                cx.repaint_rect(rect.inset(-2.));
            }
            let _ = cx.after(self.blink, Duration::from_millis(530));
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        self.theme = theme(cx);
        self.width = c.max.width;
        let style = TextStyle {
            size: self.theme.scale.font_size,
            font: 0,
        };
        let inset = self.insets();
        let revision = self.document.revision();
        let wrapping = self.wrap && self.multiline;
        let content_width = |strip: f32| {
            wrapping
                .then_some((c.max.width - 2. * inset.x - strip).max(0.))
                .filter(|w| w.is_finite())
        };
        // A bar takes its own strip so text wraps beside it rather than under it.
        // Only ambiguous overflow needs both widths; existing layouts can supply
        // unchanged lines while the text service recalculates the edited ones.
        let valid = self.paragraph.as_ref().is_some_and(|p| {
            p.revision == revision
                && p.style == style
                && p.service_revision == cx.text_revision()
                && p.width == content_width(self.strip)
                && ((!self.multiline || !c.max.height.is_finite())
                    || ((p.size.height + 2. * inset.y > c.max.height) == (self.strip > 0.)))
        });
        if !valid {
            let previous = self.paragraph.take();
            let text = self.text_snapshot();
            // Start beside an existing scrollbar. If the hard line breaks alone
            // overflow, widening cannot remove it and a second layout is wasteful.
            let mut paragraph = cx.paragraph(TextRequest {
                previous: previous.as_deref(),
                text: text.clone(),
                style,
                width: content_width(self.strip),
                revision,
            });
            drop(previous);
            let hard_lines = text.bytes().filter(|b| *b == b'\n').count() + 1;
            let definitely_overflows =
                hard_lines as f32 * paragraph.line_height + 2. * inset.y > c.max.height;
            if self.strip > 0. && (!self.multiline || !definitely_overflows) {
                self.strip = 0.;
                paragraph = cx.paragraph(TextRequest {
                    previous: Some(&paragraph),
                    text: text.clone(),
                    style,
                    width: content_width(0.),
                    revision,
                });
            }
            if self.strip == 0.
                && self.multiline
                && c.max.height.is_finite()
                && paragraph.size.height + 2. * inset.y > c.max.height
            {
                self.strip = crate::Scrollbar::width(&self.theme);
                // A narrower layout can rewrap every line. Release the wide
                // geometry before allocating it, especially on initial load.
                drop(paragraph);
                paragraph = cx.paragraph(TextRequest {
                    previous: None,
                    text,
                    style,
                    width: content_width(self.strip),
                    revision,
                });
            }
            self.paragraph = Some(paragraph);
        }
        let width = content_width(self.strip);
        if self
            .placeholder_layout
            .as_ref()
            .is_none_or(|p| p.style != style || p.service_revision != cx.text_revision())
        {
            self.placeholder_layout = Some(cx.paragraph(TextRequest {
                previous: None,
                text: self.placeholder.clone(),
                style,
                width,
                revision: 0,
            }));
        }
        self.composition_layout = if self.preedit.is_empty() {
            None
        } else {
            let range = self.selection().unwrap_or(self.caret.byte..self.caret.byte);
            let display: Arc<str> = format!(
                "{}{}{}",
                &self.text()[..range.start],
                self.preedit,
                &self.text()[range.end..]
            )
            .into();
            Some(cx.paragraph(TextRequest {
                previous: None,
                text: display,
                style,
                width,
                revision,
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
        self.height = size.height;
        let baseline = (p.baseline + inset.y).min(size.height);
        let max_y = (p.size.height + 2. * inset.y - size.height).max(0.);
        self.scroll.y = self.scroll.y.min(max_y);
        if self.reveal_pending && !self.restore_pending {
            self.reveal(size);
        }
        self.reveal_pending = false;
        self.restore_pending = false;
        let mut extensions = std::mem::take(&mut self.extensions);
        match self.view(Rect::from_size(size), self.focused) {
            Some(view) => {
                for extension in extensions.iter_mut() {
                    extension.sync(&view);
                }
                let mut presented = Vec::new();
                for extension in extensions.iter() {
                    extension.presentation(&view, &mut |range| presented.push(range));
                }
                self.presented = presented;
            }
            None => self.presented = Vec::new(),
        }
        self.extensions = extensions;
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
            } if self
                .scrollbar(cx.bounds())
                .is_some_and(|bar| bar.track.contains(*position)) =>
            {
                let thumb = self.scrollbar(cx.bounds()).unwrap().thumb;
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
            // Armed-target tracking comes before hover handling: a press on a
            // target leaves `drag` none, and the move must still update it.
            Input::Pointer { pointer, position }
                if self.armed.as_ref().is_some_and(|a| a.pointer == *pointer) =>
            {
                let origin = self.armed.as_ref().unwrap().origin;
                if (position.x - origin.x).hypot(position.y - origin.y) > 4. {
                    self.armed = None;
                    self.anchor = Some(self.hit(origin).byte);
                    self.drag = Some(*pointer);
                    self.drag_position = *position;
                    self.caret = self.hit(*position);
                    self.reveal(cx.bounds().size());
                    self.changed(cx, before, true);
                    cx.stop();
                    return;
                }
                self.arm_move(*position, *pointer);
                cx.stop();
                return;
            }
            // Dragging with the button held extends the selection; the reveal
            // keeps the view following the dragging caret. The frame loop
            // auto-scrolls from `drag_position` near the viewport edges.
            Input::Pointer { pointer, position } if self.drag == Some(*pointer) => {
                self.drag_position = *position;
                self.caret = self.hit(*position);
                self.reveal(cx.bounds().size());
                self.changed(cx, before, true);
                cx.stop();
                return;
            }
            Input::Pointer { position, .. } if self.drag.is_none() => {
                let hovered = self
                    .scrollbar(cx.bounds())
                    .is_some_and(|bar| bar.contains(*position));
                if hovered != self.bar_hover {
                    self.bar_hover = hovered;
                    cx.repaint()
                }
                // Moving the pointer is not an edit. Falling through from here would
                // reach the caret reveal at the end of this match and drag the view
                // back to the caret on every mouse move.
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
                // A target that claims the press arms for activation on release
                // inside; the press is consumed either way.
                if self.arm_target(*position, *pointer, cx.focused()) {
                    let _ = cx.focus();
                    let _ = cx.capture(*pointer);
                    cx.stop();
                    return;
                }
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
            Input::Button {
                pointer,
                button: 1,
                down: false,
                ..
            } if self.armed.as_ref().is_some_and(|a| a.pointer == *pointer) => {
                let focused = cx.focused();
                let transaction = self.arm_release(*pointer, focused);
                let _ = cx.release(*pointer);
                if let Some(transaction) = transaction {
                    let scroll = transaction.scroll;
                    if self.apply(transaction) {
                        if scroll == ScrollPolicy::Reveal {
                            self.reveal(cx.bounds().size());
                        }
                        self.changed(cx, before, scroll == ScrollPolicy::Reveal);
                    }
                }
                cx.stop();
                return;
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
                self.clear_preedit();
            }
            Input::Preedit {
                text, selection, ..
            } => {
                self.preedit = text.clone();
                self.preedit_selection = selection
                    .filter(|(a, b)| text.is_char_boundary(*a) && text.is_char_boundary(*b));
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
                // Compositions confirm text; extensions only see plain key presses.
                if self.preedit.is_empty() {
                    if let Some(paragraph) = self.paragraph.clone() {
                        let view = EditorView {
                            paragraph: &paragraph,
                            caret: self.caret,
                            selection: self.selection(),
                            viewport: Rect::new(
                                self.scroll.x,
                                self.scroll.y,
                                cx.bounds().width,
                                cx.bounds().height,
                            ),
                            focused: cx.focused(),
                        };
                        let mut extensions = std::mem::take(&mut self.extensions);
                        let claimed = extensions
                            .iter_mut()
                            .rev()
                            .find_map(|e| e.key(&view, *key, *m));
                        self.extensions = extensions;
                        if let Some(transaction) = claimed {
                            let scroll = transaction.scroll;
                            if self.apply(transaction) {
                                if scroll == ScrollPolicy::Reveal {
                                    self.reveal(cx.bounds().size());
                                }
                                self.changed(cx, before, scroll == ScrollPolicy::Reveal);
                                cx.stop();
                                return;
                            }
                            // A claimed key stays consumed even when its atomic
                            // transaction is rejected by the document policy.
                            if std::mem::take(&mut self.limit_reached) {
                                let _ = cx.emit(EditorOutput::LimitReached);
                            }
                            cx.stop();
                            return;
                        }
                    }
                }
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
                    Key::Left | Key::Right => {
                        let right = *key == Key::Right;
                        let caret = if ctrl {
                            Caret::at(if right {
                                self.word_right()
                            } else {
                                self.word_left()
                            })
                        } else if let Some(p) = &self.paragraph {
                            if !m.shift && self.selection().is_some() {
                                let anchor = Caret::at(self.anchor.unwrap());
                                let a = p.caret_point(anchor);
                                let b = p.caret_point(self.caret);
                                let anchor_before = (a.y, a.x) < (b.y, b.x);
                                if anchor_before == right {
                                    self.caret
                                } else {
                                    anchor
                                }
                            } else {
                                p.visual_move(self.caret, right)
                            }
                        } else {
                            Caret::at(if right { self.next() } else { self.prev() })
                        };
                        self.move_to(caret.byte, m.shift);
                        self.caret.affinity = caret.affinity;
                    }
                    Key::Home | Key::End => {
                        let caret = if ctrl {
                            Caret::at(if *key == Key::Home {
                                0
                            } else {
                                self.text().len()
                            })
                        } else {
                            self.paragraph
                                .as_ref()
                                .map_or(self.caret, |p| p.line_edge(self.caret, *key == Key::End))
                        };
                        self.move_to(caret.byte, m.shift);
                        self.caret.affinity = caret.affinity;
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
                            self.move_to(caret.byte, m.shift);
                            self.caret.affinity = caret.affinity;
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
                        self.clear_preedit();
                        cx.relayout()
                    }
                    _ => return,
                }
            }
            _ => return,
        }
        self.reveal(cx.bounds().size());
        self.changed(cx, before, true);
        cx.stop();
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        let t = cx.environment::<Theme>().cloned().unwrap_or(self.theme);
        let inset = self.insets();
        // Without chrome the editor draws no surface of its own: whatever contains
        // it owns the background, and nesting two wells looks like a mistake.
        if self.chrome {
            cx.painter
                .rect(cx.bounds, t.scale.radius, t.color.background.into());
            cx.painter.stroke(
                cx.bounds.inset(0.5),
                t.scale.radius,
                1.,
                if cx.focused {
                    t.color.accent
                } else {
                    t.color.border
                },
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
            for extension in self.extensions.iter() {
                if let Some(view) = self.view(cx.bounds, cx.focused) {
                    extension.paint(&view, EditorLayer::BehindText, cx.painter);
                }
            }
            if let Some(selection) = self.selection().filter(|_| self.preedit.is_empty()) {
                for rect in p.selection_in(
                    selection,
                    Rect::new(
                        self.scroll.x - inset.x,
                        self.scroll.y - inset.y,
                        cx.bounds.width,
                        cx.bounds.height,
                    ),
                ) {
                    cx.painter.rect(rect, 0., t.color.selection.into())
                }
            }
            if let Some(composition) = &self.composition_layout {
                cx.painter
                    .paragraph(composition, Point::default(), t.color.foreground.into());
            } else if self.text().is_empty() {
                if let Some(placeholder) = &self.placeholder_layout {
                    cx.painter
                        .paragraph(placeholder, Point::default(), t.color.muted.into())
                }
            } else {
                self.paint_text(p, cx, Brush::Solid(t.color.foreground))
            }
            for extension in self.extensions.iter() {
                if let Some(view) = self.view(cx.bounds, cx.focused) {
                    extension.paint(&view, EditorLayer::AboveText, cx.painter);
                }
            }
            if cx.focused {
                let point = p.caret_point(self.caret);
                if self.caret_on && self.preedit.is_empty() {
                    cx.painter.rect(
                        Rect::new(point.x, point.y, 1.5, p.line_height),
                        0.,
                        t.color.accent.into(),
                    )
                }
                if let Some(composition) = &self.composition_layout {
                    let start = self.composition_start();
                    for rect in composition.selection(start..start + self.preedit.len()) {
                        cx.painter.rect(
                            Rect::new(rect.x, rect.y + rect.height - 1., rect.width, 1.),
                            0.,
                            t.color.accent.into(),
                        );
                    }
                    if let Some((anchor, caret)) = self.preedit_selection {
                        for rect in composition
                            .selection(start + anchor.min(caret)..start + anchor.max(caret))
                        {
                            cx.painter.rect(rect, 0., t.color.selection.into());
                        }
                        let at = composition.caret_point(Caret::at(start + caret));
                        cx.painter.rect(
                            Rect::new(at.x, at.y, 1.5, composition.line_height),
                            0.,
                            t.color.accent.into(),
                        );
                    }
                }
            }
        }
        cx.painter.restore();
        if let Some(bar) = self.scrollbar(cx.bounds) {
            bar.paint(
                cx.painter,
                &t,
                self.bar_hover,
                self.scrollbar_drag.is_some(),
            )
        }
    }
    fn ime_cursor(&self) -> Option<Rect> {
        let p = self.paragraph.as_ref()?;
        let point = if let Some(composition) = &self.composition_layout {
            let byte = self.composition_start()
                + self
                    .preedit_selection
                    .map_or(self.preedit.len(), |(_, caret)| caret);
            composition.caret_point(Caret::at(byte))
        } else {
            p.caret_point(self.caret)
        };
        Some(Rect::new(
            point.x + self.insets().x - self.scroll.x,
            point.y + self.insets().y - self.scroll.y,
            1.5,
            p.line_height,
        ))
    }
    fn accessibility(
        &mut self,
        cx: &mut Update<'_, Self>,
        action: SemanticAction,
    ) -> Result<(), SemanticError> {
        if action == SemanticAction::Focus {
            return cx.focus().map_err(|_| SemanticError::Unavailable);
        }
        if let SemanticAction::ScrollBy(delta) = action {
            let p = self.paragraph.as_ref().ok_or(SemanticError::Unavailable)?;
            let bounds = cx.bounds();
            let size = bounds.size();
            let inset = self.insets();
            let strip = if self.scrollbar(bounds).is_some() {
                crate::Scrollbar::width(&self.theme)
            } else {
                0.
            };
            self.scroll.y = (self.scroll.y - delta.y)
                .clamp(0., (p.size.height + 2. * inset.y - size.height).max(0.));
            self.scroll.x = if self.wrap {
                0.
            } else {
                (self.scroll.x - delta.x).clamp(
                    0.,
                    (p.size.width + 2. * inset.x + strip - size.width).max(0.),
                )
            };
            self.report_state(cx);
            cx.repaint();
            return Ok(());
        }
        if let SemanticAction::ActivateChild { key } = &action {
            // A published child was activated: route it to the owning
            // extension's target and apply the resulting transaction through
            // the same finalization as pointer and keyboard activation.
            if !self.preedit.is_empty() {
                return Err(SemanticError::Unavailable);
            }
            let Some((index, target)) = key.split_once(':') else {
                return Err(SemanticError::Unsupported);
            };
            let Ok(index) = index.parse::<usize>() else {
                return Err(SemanticError::Unsupported);
            };
            let Ok(target) = target.parse::<u64>() else {
                return Err(SemanticError::Unsupported);
            };
            let transaction = {
                let paragraph = self.paragraph.as_ref().ok_or(SemanticError::Unavailable)?;
                let view = EditorView {
                    paragraph,
                    caret: self.caret,
                    selection: self.selection(),
                    viewport: Rect::new(self.scroll.x, self.scroll.y, self.width, self.height),
                    focused: self.focused,
                };
                self.extensions
                    .get_mut(index)
                    .ok_or(SemanticError::Unavailable)?
                    .activate(&view, target)
            };
            let Some(transaction) = transaction else {
                return Ok(());
            };
            let scroll = transaction.scroll;
            let before = self.document.revision();
            if self.apply(transaction) {
                if scroll == ScrollPolicy::Reveal {
                    self.reveal(cx.bounds().size());
                }
                self.changed(cx, before, scroll == ScrollPolicy::Reveal);
                return Ok(());
            }
            return Err(SemanticError::Unavailable);
        }
        let before = self.document.revision();
        let replacement = match action {
            SemanticAction::SetValue(text) => Some((0..self.text().len(), text)),
            SemanticAction::ReplaceSelectedText(text) => Some((
                self.selection().unwrap_or(self.caret.byte..self.caret.byte),
                text,
            )),
            SemanticAction::ReplaceText { start, end, text } => Some((start..end, text)),
            SemanticAction::SetSelection { anchor, caret } => {
                if self.boundary(anchor) != anchor || self.boundary(caret) != caret {
                    return Err(SemanticError::InvalidSelection);
                }
                self.anchor = Some(anchor);
                self.caret = Caret::at(caret);
                None
            }
            _ => return Err(SemanticError::Unsupported),
        };
        if let Some((range, text)) = replacement {
            // Check before mutating composition, selection or history.
            if range.start > range.end
                || !self.text().is_char_boundary(range.start)
                || !self.text().is_char_boundary(range.end)
            {
                return Err(SemanticError::InvalidSelection);
            }
            let added = text
                .chars()
                .filter(|c| *c != '\r')
                .map(char::len_utf8)
                .sum::<usize>();
            let next_len = self.text().len() - range.len() + added;
            if next_len > self.max_bytes && next_len > self.text().len() {
                return Err(SemanticError::LimitReached);
            }
            self.replace(range, &text);
        }
        self.clear_preedit();
        self.reveal(cx.bounds().size());
        self.changed(cx, before, true);
        Ok(())
    }
    fn semantics(&self) -> Semantics {
        let mut semantics = Semantics {
            role: Role::TextInput,
            label: self
                .label
                .clone()
                .unwrap_or_else(|| self.placeholder.to_string()),
            key: self.key.clone(),
            actions: vec![
                SemanticActionKind::SetValue,
                SemanticActionKind::ReplaceSelectedText,
                SemanticActionKind::ReplaceText,
                SemanticActionKind::SetSelection,
                SemanticActionKind::ScrollBy,
            ],
            text: Some(TextSemantics {
                anchor: self.anchor.unwrap_or(self.caret.byte),
                caret: self.caret,
                multiline: self.multiline,
                composition: (!self.preedit.is_empty()).then(|| Composition {
                    text: self.preedit.clone(),
                    selection: self.preedit_selection,
                }),
                paragraph: self.paragraph.clone(),
                origin: Point::new(
                    self.insets().x - self.scroll.x,
                    self.insets().y - self.scroll.y,
                ),
            }),
            value: Some(self.text().into()),
            ..Semantics::default()
        };
        if let Some(view) = self.view(Rect::new(0., 0., self.width, self.height), self.focused) {
            // Paragraph coordinates become widget-local at the inset origin
            // minus the scroll offset.
            let inset = self.insets();
            let origin = Point::new(inset.x - self.scroll.x, inset.y - self.scroll.y);
            for (index, extension) in self.extensions.iter().enumerate() {
                for target in extension.targets(&view) {
                    if let Some(mut child) = extension.semantics(&view, target.id) {
                        // The key namespaces the target by extension, so two
                        // extensions picking the same id never hijack each
                        // other's activation.
                        child.key = Some(format!("{index}:{}", target.id));
                        child.bounds = child
                            .bounds
                            .or(Some(target.bounds))
                            .map(|b| Rect::new(b.x + origin.x, b.y + origin.y, b.width, b.height));
                        semantics.children.push(child);
                    }
                }
            }
        }
        semantics
    }
}
