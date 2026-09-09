use crate::{Point, Rect, Size};
use std::{
    ops::Range,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub font: u32,
}
impl Default for TextStyle {
    fn default() -> Self {
        Self { size: 15., font: 0 }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Affinity {
    Upstream,
    Downstream,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caret {
    pub byte: usize,
    pub affinity: Affinity,
}
impl Caret {
    pub fn at(byte: usize) -> Self {
        Self {
            byte,
            affinity: Affinity::Downstream,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Stop {
    pub caret: Caret,
    pub x: f32,
}
#[derive(Clone, Debug)]
pub struct TextCell {
    /// Exclusive UTF-8 byte boundary in the paragraph source.
    ///
    /// The start is the preceding cell's end, or the containing line's range start.
    /// See [`TextLine::cells`] for the logical coverage contract.
    pub end: usize,
    pub x: f32,
    /// Signed advance: negative for a right-to-left cell; x remains the left edge.
    pub advance: f32,
}
impl TextCell {
    pub fn width(&self) -> f32 {
        self.advance.abs()
    }
}
#[derive(Clone, Debug)]
pub struct Glyph {
    pub index: u32,
    pub offset: Point,
}
#[derive(Clone, Debug)]
pub struct TextRun {
    /// Entry in the ordered font resource list shared by the text engine and renderer.
    /// This is not the face index within a font file or collection file.
    pub font: u32,
    pub glyphs: Box<[Glyph]>,
    pub x: f32,
}
#[derive(Clone, Debug)]
pub struct TextLine {
    pub range: Range<usize>,
    pub y: f32,
    pub width: f32,
    /// Cells partition `range` in logical source order, independently of visual x.
    /// Ends must strictly increase on UTF-8 boundaries, with the final end equal
    /// to `range.end`. An empty line has no cells; a nonempty line has at least one.
    /// Invisible source characters remain represented, using zero advance when
    /// appropriate. A cell may span a shaped cluster or part of a ligature.
    /// Custom text engines must uphold these invariants when constructing lines.
    pub cells: Vec<TextCell>,
    pub runs: Vec<TextRun>,
}
impl TextLine {
    /// Logical source ranges and their geometry, without allocating range storage.
    pub fn cells(
        &self,
    ) -> impl DoubleEndedIterator<Item = (Range<usize>, &TextCell)> + ExactSizeIterator {
        self.cells
            .iter()
            .enumerate()
            .map(|(index, cell)| (self.cell_range(index).unwrap(), cell))
    }
    pub fn cell_at(&self, byte: usize) -> Option<&TextCell> {
        self.range
            .contains(&byte)
            .then(|| {
                self.cells
                    .get(self.cells.partition_point(|c| c.end <= byte))
            })
            .flatten()
    }
    pub fn cell_range(&self, index: usize) -> Option<Range<usize>> {
        self.cells.get(index).map(|cell| {
            self.cells
                .get(index.wrapping_sub(1))
                .map_or(self.range.start, |c| c.end)..cell.end
        })
    }

    /// Caret edges derived from cells without a second per-grapheme allocation.
    pub fn stops(&self) -> impl Iterator<Item = Stop> + '_ {
        self.cells
            .iter()
            .enumerate()
            .flat_map(|(i, c)| {
                let (start, end) = if c.advance.is_sign_negative() {
                    (c.x + c.width(), c.x)
                } else {
                    (c.x, c.x + c.width())
                };
                let shared = self.cells.get(i + 1).is_some_and(|next| {
                    let x = if next.advance.is_sign_negative() {
                        next.x + next.width()
                    } else {
                        next.x
                    };
                    (x - end).abs() < 0.01
                });
                [
                    Some(Stop {
                        caret: Caret::at(self.cell_range(i).unwrap().start),
                        x: start,
                    }),
                    (!shared).then_some(Stop {
                        caret: Caret {
                            byte: c.end,
                            affinity: Affinity::Upstream,
                        },
                        x: end,
                    }),
                ]
                .into_iter()
                .flatten()
            })
            .chain(self.cells.is_empty().then_some(Stop {
                caret: Caret::at(self.range.start),
                x: 0.,
            }))
    }
}
#[derive(Clone, Debug)]
pub struct Paragraph {
    pub text: Arc<str>,
    pub style: TextStyle,
    pub revision: u64,
    /// Identity of the text engine metrics used for this paragraph.
    pub service_revision: TextRevision,
    pub width: Option<f32>,
    pub size: Size,
    pub baseline: f32,
    pub line_height: f32,
    pub lines: Vec<TextLine>,
}
impl Paragraph {
    pub fn caret_point(&self, caret: Caret) -> Point {
        let mut index = self
            .lines
            .partition_point(|line| line.range.start <= caret.byte)
            .saturating_sub(1);
        if caret.affinity == Affinity::Upstream
            && index > 0
            && self.lines[index - 1].range.end == caret.byte
        {
            index -= 1;
        }
        let line = self.lines.get(index);
        line.map(|l| {
            Point::new(
                l.stops()
                    .find(|s| s.caret == caret)
                    .or_else(|| l.stops().find(|s| s.caret.byte == caret.byte))
                    .map_or(l.width, |s| s.x),
                l.y,
            )
        })
        .unwrap_or_default()
    }
    pub fn hit(&self, p: Point) -> Caret {
        let index = (p.y / self.line_height).floor().max(0.) as usize;
        self.lines
            .get(index.min(self.lines.len().saturating_sub(1)))
            .and_then(|l| {
                l.stops()
                    .min_by(|a, b| (a.x - p.x).abs().total_cmp(&(b.x - p.x).abs()))
            })
            .map_or(Caret::at(0), |s| s.caret)
    }
    /// Move between visually distinct caret positions, retaining bidi/wrap affinity.
    pub fn visual_move(&self, caret: Caret, right: bool) -> Caret {
        let point = self.caret_point(caret);
        let Some(index) = self.lines.iter().position(|l| (l.y - point.y).abs() < 0.1) else {
            return caret;
        };
        let line = &self.lines[index];
        let next = line
            .stops()
            .filter(|s| {
                if right {
                    s.x > point.x + 0.01
                } else {
                    s.x < point.x - 0.01
                }
            })
            .min_by(|a, b| (a.x - point.x).abs().total_cmp(&(b.x - point.x).abs()));
        if let Some(stop) = next {
            return stop.caret;
        }
        let adjacent = if right {
            self.lines.get(index + 1)
        } else {
            index.checked_sub(1).and_then(|i| self.lines.get(i))
        };
        adjacent
            .and_then(|l| {
                l.stops().min_by(|a, b| {
                    if right {
                        a.x.total_cmp(&b.x)
                    } else {
                        b.x.total_cmp(&a.x)
                    }
                })
            })
            .map_or(caret, |s| s.caret)
    }
    pub fn line_edge(&self, caret: Caret, right: bool) -> Caret {
        let point = self.caret_point(caret);
        self.lines
            .iter()
            .find(|l| (l.y - point.y).abs() < 0.1)
            .and_then(|l| {
                l.stops().min_by(|a, b| {
                    if right {
                        b.x.total_cmp(&a.x)
                    } else {
                        a.x.total_cmp(&b.x)
                    }
                })
            })
            .map_or(caret, |s| s.caret)
    }
    pub fn selection(&self, range: Range<usize>) -> Vec<Rect> {
        self.selection_in(range, Rect::new(0., 0., f32::MAX, f32::MAX))
    }
    pub fn selection_in(&self, range: Range<usize>, viewport: Rect) -> Vec<Rect> {
        let first = self
            .lines
            .partition_point(|l| l.y + self.line_height < viewport.y);
        self.lines[first..]
            .iter()
            .take_while(|l| l.y < viewport.y + viewport.height)
            .flat_map(|l| {
                let mut rects: Vec<Rect> = vec![];
                let mut cells: Vec<_> = l
                    .cells()
                    .filter(|(cell, _)| cell.start < range.end && cell.end > range.start)
                    .map(|(_, cell)| cell)
                    .collect();
                cells.sort_by(|a, b| a.x.total_cmp(&b.x));
                for cell in cells {
                    let r = Rect::new(cell.x, l.y, cell.width(), self.line_height);
                    if let Some(last) = rects
                        .last_mut()
                        .filter(|last| (last.x + last.width - r.x).abs() < 0.1)
                    {
                        *last = last.union(r);
                    } else {
                        rects.push(r);
                    }
                }
                rects
            })
            .collect()
    }
}
pub struct TextRequest<'a> {
    /// Optional existing layout owned by the caller. Engines may reuse unchanged
    /// lines; they must validate text, metrics and wrapping before doing so.
    /// This borrow does not create a persistent cache or extend its lifetime.
    pub previous: Option<&'a Paragraph>,
    pub text: Arc<str>,
    pub style: TextStyle,
    pub width: Option<f32>,
    pub revision: u64,
}
/// Cache identity for one immutable text-engine metric configuration.
///
/// Create a token for each independent configuration and replace it whenever fonts,
/// shaping policy or other metrics change. Clones of an unchanged configuration may
/// share its token. Document revisions are independent of this identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextRevision(u64);
impl TextRevision {
    /// Issue a process-unique token without allocating heap storage.
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(
            NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .expect("text revision tokens exhausted"),
        )
    }
}
impl Default for TextRevision {
    fn default() -> Self {
        Self::new()
    }
}
pub trait TextEngine {
    /// Stable while this engine's metrics are unchanged, and distinct from engines
    /// with different metrics. Return the same token in [`Paragraph::service_revision`].
    fn revision(&self) -> TextRevision;
    fn layout(&mut self, request: TextRequest) -> Arc<Paragraph>;
}
/// Deterministic metrics for behavioral tests, never substituted for native shaping.
pub struct TestText;
impl TextEngine for TestText {
    fn revision(&self) -> TextRevision {
        // Every TestText instance has the same immutable metrics. Zero is private.
        TextRevision(0)
    }
    fn layout(&mut self, r: TextRequest) -> Arc<Paragraph> {
        let advance = r.style.size * 0.6;
        let height = r.style.size * 1.5;
        let mut lines = vec![];
        let mut start = 0;
        let mut x = 0.;
        let mut stops = vec![Stop {
            caret: Caret::at(0),
            x: 0.,
        }];
        for (byte, ch) in r.text.char_indices() {
            if ch == '\n' || r.width.is_some_and(|w| x > 0. && x + advance > w) {
                lines.push(TextLine {
                    range: start..byte,
                    y: lines.len() as f32 * height,
                    width: x,
                    cells: cells(&stops),
                    runs: vec![TextRun {
                        font: 0,
                        glyphs: Box::default(),
                        x: 0.,
                    }],
                });
                stops.clear();
                start = if ch == '\n' { byte + 1 } else { byte };
                x = 0.;
                stops.push(Stop {
                    caret: Caret::at(start),
                    x,
                });
                if ch == '\n' {
                    continue;
                }
            }
            x += advance;
            stops.push(Stop {
                caret: Caret::at(byte + ch.len_utf8()),
                x,
            });
        }
        lines.push(TextLine {
            range: start..r.text.len(),
            y: lines.len() as f32 * height,
            width: x,
            cells: cells(&stops),
            runs: vec![TextRun {
                font: 0,
                glyphs: Box::default(),
                x: 0.,
            }],
        });
        let size = Size::new(
            lines.iter().map(|l| l.width).fold(0., f32::max),
            lines.len() as f32 * height,
        );
        Arc::new(Paragraph {
            text: r.text,
            style: r.style,
            revision: r.revision,
            service_revision: self.revision(),
            width: r.width,
            size,
            baseline: r.style.size * 1.1,
            line_height: height,
            lines,
        })
    }
}

fn cells(stops: &[Stop]) -> Vec<TextCell> {
    stops
        .windows(2)
        .map(|w| TextCell {
            end: w[1].caret.byte,
            x: w[0].x.min(w[1].x),
            advance: w[1].x - w[0].x,
        })
        .collect()
}
