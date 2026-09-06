use crate::Theme;
use fire_ui::*;
use std::{collections::VecDeque, ops::Range, time::Duration};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug)]
pub struct TextChanged;
struct Edit {
    start: usize,
    removed: String,
    inserted: String,
}
pub struct TextLine {
    pub range: Range<usize>,
    pub metrics: LineMetrics,
    pub y: f32,
}

/// The same editing implementation supports single-line fields and multiline editors.
/// UTF-8 byte offsets are always placed on grapheme boundaries by editing commands.
pub struct TextInput {
    text: String,
    cursor: usize,
    anchor: Option<usize>,
    pub placeholder: String,
    pub theme: Theme,
    pub multiline: bool,
    pub wrap: bool,
    pub style: TextStyle,
    pub scroll: Point,
    lines: Vec<TextLine>,
    dirty: bool,
    layout_width: f32,
    layout_style: TextStyle,
    layout_wrap: bool,
    layout_size: Size,
    unwrapped_width: Option<f32>,
    undo: VecDeque<Edit>,
    redo: Vec<Edit>,
    history_bytes: usize,
    dragging: bool,
    last_pointer: Point,
    cursor_on: bool,
    composition: String,
}
impl TextInput {
    pub fn new(text: impl Into<String>, theme: Theme) -> Self {
        Self {
            text: text.into(),
            cursor: 0,
            anchor: None,
            placeholder: String::new(),
            theme,
            multiline: false,
            wrap: false,
            style: TextStyle {
                size: theme.font_size,
                font: 0,
            },
            scroll: Point::default(),
            lines: vec![],
            dirty: true,
            layout_width: -1.,
            layout_style: TextStyle::default(),
            layout_wrap: false,
            layout_size: Size::default(),
            unwrapped_width: None,
            undo: VecDeque::new(),
            redo: vec![],
            history_bytes: 0,
            dragging: false,
            last_pointer: Point::default(),
            cursor_on: true,
            composition: String::new(),
        }
    }
    pub fn editor(text: impl Into<String>, theme: Theme) -> Self {
        Self {
            multiline: true,
            wrap: true,
            ..Self::new(text, theme)
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn selection(&self) -> Option<Range<usize>> {
        self.anchor
            .filter(|a| *a != self.cursor)
            .map(|a| a.min(self.cursor)..a.max(self.cursor))
    }
    pub fn lines(&self) -> &[TextLine] {
        &self.lines
    }
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = 0;
        self.anchor = None;
        self.undo.clear();
        self.redo.clear();
        self.history_bytes = 0;
        self.dirty = true;
    }
    fn line_height(&self) -> f32 {
        self.style.size * 1.55
    }
    fn prev(&self, byte: usize) -> usize {
        self.text[..byte]
            .grapheme_indices(true)
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
    fn next(&self, byte: usize) -> usize {
        self.text[byte..]
            .graphemes(true)
            .next()
            .map(|g| byte + g.len())
            .unwrap_or(self.text.len())
    }
    fn word_left(&self) -> usize {
        self.text[..self.cursor]
            .unicode_word_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
    fn word_right(&self) -> usize {
        self.text[self.cursor..]
            .unicode_word_indices()
            .next()
            .map(|(i, s)| self.cursor + i + s.len())
            .unwrap_or(self.text.len())
    }
    fn move_to(&mut self, byte: usize, select: bool) {
        if select {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else {
            self.anchor = None;
        }
        self.cursor = byte;
    }
    fn replace(&mut self, range: Range<usize>, inserted: &str) {
        let removed = self.text[range.clone()].to_owned();
        self.text.replace_range(range.clone(), inserted);
        self.cursor = range.start + inserted.len();
        self.anchor = None;
        self.history_bytes += removed.len() + inserted.len();
        self.undo.push_back(Edit {
            start: range.start,
            removed,
            inserted: inserted.into(),
        });
        self.redo.clear();
        // History has both an operation and byte bound. The document itself is unrestricted.
        while self.undo.len() > 256 || self.history_bytes > 2 * 1024 * 1024 {
            if let Some(e) = self.undo.pop_front() {
                self.history_bytes = self
                    .history_bytes
                    .saturating_sub(e.removed.len() + e.inserted.len());
            } else {
                break;
            }
        }
        self.dirty = true;
    }
    fn insert(&mut self, value: &str) {
        let value = value.replace('\r', "");
        let value = if self.multiline {
            value
        } else {
            value.replace('\n', " ")
        };
        self.replace(self.selection().unwrap_or(self.cursor..self.cursor), &value);
    }
    fn history(&mut self, redo: bool) {
        if redo {
            if let Some(e) = self.redo.pop() {
                self.text
                    .replace_range(e.start..e.start + e.removed.len(), &e.inserted);
                self.cursor = e.start + e.inserted.len();
                self.undo.push_back(e);
                self.dirty = true;
            }
        } else if let Some(e) = self.undo.pop_back() {
            self.text
                .replace_range(e.start..e.start + e.inserted.len(), &e.removed);
            self.cursor = e.start + e.removed.len();
            self.redo.push(e);
            self.dirty = true;
        }
        self.anchor = None;
    }
    fn rebuild(&mut self, width: f32, text: &mut dyn TextEngine) -> bool {
        let width = (width - self.theme.padding * 2.).max(1.);
        if !self.dirty
            && self.layout_style == self.style
            && self.layout_wrap == self.wrap
            && (self.layout_width == width
                || !self.wrap
                || self.unwrapped_width.is_some_and(|needed| needed <= width))
        {
            self.layout_width = width;
            return false;
        }
        self.lines.clear();
        self.layout_width = width;
        self.layout_style = self.style;
        self.layout_wrap = self.wrap;
        self.unwrapped_width = Some(0.);
        let mut base = 0;
        for logical in self.text.split('\n') {
            // Shape the logical line once. Do not repeatedly shape each remaining suffix.
            let measured = text.measure(logical, self.style);
            if !self.wrap || measured.width <= width {
                if let Some(max) = &mut self.unwrapped_width {
                    *max = max.max(measured.width);
                }
                self.lines.push(TextLine {
                    range: base..base + logical.len(),
                    metrics: measured,
                    y: self.lines.len() as f32 * self.line_height(),
                });
                base += logical.len() + 1;
                continue;
            }
            self.unwrapped_width = None;
            let mut start = 0;
            loop {
                let rest = &logical[start..];
                let mut end = rest.len();
                let origin = measured.x(start);
                if measured.width - origin > width {
                    end = rest
                        .grapheme_indices(true)
                        .skip(1)
                        .take_while(|(b, _)| measured.x(start + *b) - origin <= width)
                        .map(|(b, _)| b)
                        .last()
                        .unwrap_or_else(|| rest.graphemes(true).next().map(str::len).unwrap_or(0));
                    // Prefer a word break if it does not produce an empty visual line.
                    if let Some((i, g)) = rest[..end]
                        .grapheme_indices(true)
                        .rfind(|(_, g)| g.chars().all(char::is_whitespace))
                    {
                        if i > 0 {
                            end = i + g.len();
                        }
                    }
                }
                let metrics = text.measure(&rest[..end], self.style);
                let y = self.lines.len() as f32 * self.line_height();
                self.lines.push(TextLine {
                    range: base + start..base + start + end,
                    metrics,
                    y,
                });
                start += end;
                if start >= logical.len() {
                    break;
                }
            }
            base += logical.len() + 1;
        }
        self.dirty = false;
        true
    }
    fn cursor_line(&self) -> usize {
        self.lines
            .partition_point(|line| line.range.start <= self.cursor)
            .saturating_sub(1)
    }
    fn caret(&self) -> Point {
        let Some(line) = self.lines.get(self.cursor_line()) else {
            return Point::default();
        };
        Point::new(
            line.metrics.x(self
                .cursor
                .saturating_sub(line.range.start)
                .min(line.range.len())),
            line.y,
        )
    }
    fn hit(&self, point: Point) -> usize {
        let y = (point.y - self.theme.padding + self.scroll.y).max(0.);
        let row =
            ((y / self.line_height()).floor() as usize).min(self.lines.len().saturating_sub(1));
        let Some(line) = self.lines.get(row) else {
            return 0;
        };
        let byte = line.range.start
            + line
                .metrics
                .hit(point.x - self.theme.padding + self.scroll.x);
        // A backend may expose shaping-cluster stops. Keep edits on grapheme boundaries.
        self.text[line.range.clone()]
            .grapheme_indices(true)
            .map(|(i, _)| line.range.start + i)
            .chain(std::iter::once(line.range.end))
            .min_by_key(|i| i.abs_diff(byte))
            .unwrap_or(0)
    }
    fn reveal(&mut self, bounds: Rect) {
        let caret = self.caret();
        let w = (bounds.width - 2. * self.theme.padding - 2.).max(1.);
        let h = (bounds.height - 2. * self.theme.padding).max(self.line_height());
        if self.wrap {
            self.scroll.x = 0.;
        } else {
            if caret.x < self.scroll.x {
                self.scroll.x = caret.x;
            }
            if caret.x > self.scroll.x + w {
                self.scroll.x = caret.x - w;
            }
        }
        if caret.y < self.scroll.y {
            self.scroll.y = caret.y;
        }
        if caret.y + self.line_height() > self.scroll.y + h {
            self.scroll.y = caret.y + self.line_height() - h;
        }
    }
    fn blink(&mut self, ctx: &mut EventCtx<'_>) {
        self.cursor_on = true;
        ctx.repaint();
        if ctx.focused {
            ctx.request_frame_after(Duration::from_millis(530));
        }
    }
}
impl Widget for TextInput {
    fn focusable(&self) -> bool {
        true
    }
    fn layout(&mut self, _: &mut [Node], c: Constraints, text: &mut dyn TextEngine) -> Size {
        let size = c.constrain(Size::new(
            c.max.width,
            if self.multiline {
                c.max.height
            } else {
                self.line_height() + self.theme.padding * 2.
            },
        ));
        let resized = self.layout_size != size;
        self.layout_size = size;
        if self.rebuild(size.width, text) || resized {
            self.reveal(Rect::from_size(size));
        }
        size
    }
    fn event(&mut self, ctx: &mut EventCtx<'_>, event: &Event) {
        let mut edited = false;
        match event {
            Event::Focus(true) => {
                self.blink(ctx);
                return;
            }
            Event::Focus(false) | Event::Cancel => {
                self.dragging = false;
                self.composition.clear();
                ctx.cancel_frame();
                ctx.repaint();
                return;
            }
            Event::Frame => {
                self.cursor_on = !self.cursor_on;
                let c = self.caret();
                ctx.repaint_rect(Rect::new(
                    self.theme.padding + c.x - self.scroll.x,
                    self.theme.padding + c.y - self.scroll.y,
                    2.,
                    self.line_height(),
                ));
                if ctx.focused {
                    ctx.request_frame_after(Duration::from_millis(530));
                }
                return;
            }
            Event::Composition(value) => {
                self.composition = value.clone();
                ctx.repaint();
                ctx.handle();
                return;
            }
            Event::PointerDown(p) => {
                self.cursor = self.hit(*p);
                self.anchor = Some(self.cursor);
                self.dragging = true;
                self.last_pointer = *p;
                ctx.focus();
                ctx.capture_pointer();
            }
            Event::PointerMove(p) if self.dragging => {
                self.last_pointer = *p;
                self.cursor = self.hit(*p);
                self.reveal(ctx.bounds);
            }
            Event::PointerUp(_) => {
                self.dragging = false;
                ctx.release_pointer();
            }
            Event::Text(s) | Event::Paste(s) => {
                self.insert(s);
                self.composition.clear();
                edited = true;
            }
            Event::Scroll { delta, .. } if self.multiline => {
                let max = (self.lines.len() as f32 * self.line_height() + 2. * self.theme.padding
                    - ctx.bounds.height)
                    .max(0.);
                let y = (self.scroll.y - delta.y).clamp(0., max);
                if y != self.scroll.y {
                    self.scroll.y = y;
                    ctx.repaint();
                    ctx.handle();
                }
                return;
            }
            Event::Key { key, modifiers: m } => {
                let ctrl = m.control || m.meta;
                match key {
                    Key::Character('a') if ctrl => {
                        self.anchor = Some(0);
                        self.cursor = self.text.len();
                    }
                    Key::Character('c') if ctrl => {
                        if let Some(r) = self.selection() {
                            ctx.copy(self.text[r].into());
                        }
                    }
                    Key::Character('x') if ctrl => {
                        if let Some(r) = self.selection() {
                            ctx.copy(self.text[r.clone()].into());
                            self.replace(r, "");
                            edited = true;
                        }
                    }
                    Key::Character('v') if ctrl => {
                        ctx.paste();
                    }
                    Key::Character('z') if ctrl => {
                        self.history(m.shift);
                        edited = true;
                    }
                    Key::Character('y') if ctrl => {
                        self.history(true);
                        edited = true;
                    }
                    Key::Left => {
                        let target = if !m.shift && self.selection().is_some() {
                            self.selection().unwrap().start
                        } else if ctrl {
                            self.word_left()
                        } else {
                            self.prev(self.cursor)
                        };
                        self.move_to(target, m.shift);
                    }
                    Key::Right => {
                        let target = if !m.shift && self.selection().is_some() {
                            self.selection().unwrap().end
                        } else if ctrl {
                            self.word_right()
                        } else {
                            self.next(self.cursor)
                        };
                        self.move_to(target, m.shift);
                    }
                    Key::Home => {
                        let byte = if ctrl {
                            0
                        } else {
                            self.lines
                                .get(self.cursor_line())
                                .map(|l| l.range.start)
                                .unwrap_or(0)
                        };
                        self.move_to(byte, m.shift);
                    }
                    Key::End => {
                        let byte = if ctrl {
                            self.text.len()
                        } else {
                            self.lines
                                .get(self.cursor_line())
                                .map(|l| l.range.end)
                                .unwrap_or(self.text.len())
                        };
                        self.move_to(byte, m.shift);
                    }
                    Key::Up | Key::Down if self.multiline => {
                        let row = self.cursor_line();
                        let next = if *key == Key::Up {
                            row.saturating_sub(1)
                        } else {
                            (row + 1).min(self.lines.len() - 1)
                        };
                        let l = &self.lines[next];
                        let byte = l.range.start + l.metrics.hit(self.caret().x);
                        self.move_to(byte, m.shift);
                    }
                    Key::Backspace => {
                        let r = self.selection().unwrap_or_else(|| {
                            (if ctrl {
                                self.word_left()
                            } else {
                                self.prev(self.cursor)
                            })..self.cursor
                        });
                        if !r.is_empty() {
                            self.replace(r, "");
                            edited = true;
                        }
                    }
                    Key::Delete => {
                        let r = self.selection().unwrap_or_else(|| {
                            self.cursor..if ctrl {
                                self.word_right()
                            } else {
                                self.next(self.cursor)
                            }
                        });
                        if !r.is_empty() {
                            self.replace(r, "");
                            edited = true;
                        }
                    }
                    Key::Enter if self.multiline => {
                        self.insert("\n");
                        edited = true;
                    }
                    Key::Escape => {
                        if self.selection().is_some() || !self.composition.is_empty() {
                            self.anchor = None;
                            self.composition.clear();
                        } else {
                            return;
                        }
                    }
                    _ => return,
                }
            }
            _ => return,
        }
        if edited {
            self.rebuild(ctx.bounds.width, ctx.text);
            ctx.emit(TextChanged);
            ctx.relayout();
        }
        self.reveal(ctx.bounds);
        self.blink(ctx);
        ctx.handle();
    }
    fn paint(&self, ctx: &mut PaintCtx<'_>) {
        let t = self.theme;
        let pad = t.padding;
        ctx.painter.rect(ctx.bounds, t.radius, t.background.into());
        ctx.painter.stroke(
            ctx.bounds.inset(0.5),
            t.radius,
            1.,
            if ctx.focused { t.accent } else { t.border },
        );
        ctx.painter.save();
        ctx.painter.clip(ctx.bounds.inset(pad));
        let selection = self.selection();
        let first = (self.scroll.y / self.line_height()).floor().max(0.) as usize;
        let visible = (ctx.bounds.height / self.line_height()).ceil().max(0.) as usize + 1;
        for line in self.lines.iter().skip(first).take(visible) {
            let y = pad + line.y - self.scroll.y;
            if y + self.line_height() < pad || y > ctx.bounds.height - pad {
                continue;
            }
            if let Some(r) = &selection {
                let start = r.start.max(line.range.start);
                let end = r.end.min(line.range.end);
                if start < end {
                    let x1 = line.metrics.x(start - line.range.start);
                    let x2 = line.metrics.x(end - line.range.start);
                    ctx.painter.rect(
                        Rect::new(
                            pad + x1.min(x2) - self.scroll.x,
                            y,
                            (x2 - x1).abs(),
                            self.line_height(),
                        ),
                        0.,
                        t.selection.into(),
                    );
                }
            }
            ctx.painter.text(
                &self.text[line.range.clone()],
                Point::new(pad - self.scroll.x, y),
                self.style,
                t.foreground.into(),
            );
        }
        if self.text.is_empty() {
            ctx.painter.text(
                &self.placeholder,
                Point::new(pad, pad),
                self.style,
                t.muted.into(),
            );
        }
        if ctx.focused {
            let c = self.caret();
            let origin = Point::new(pad + c.x - self.scroll.x, pad + c.y - self.scroll.y);
            if self.cursor_on {
                ctx.painter.rect(
                    Rect::new(origin.x, origin.y, 1.5, self.line_height()),
                    0.,
                    t.accent.into(),
                );
            }
            if !self.composition.is_empty() {
                ctx.painter
                    .text(&self.composition, origin, self.style, t.accent.into());
            }
        }
        ctx.painter.restore();
    }
}
