use crate::{Point, Rect, Size};
use std::{ops::Range, sync::Arc};
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
    pub range: Range<usize>,
    pub x: f32,
    pub width: f32,
}
#[derive(Clone, Debug)]
pub struct TextRun {
    pub x: f32,
    /// Host-shaped display text; source byte offsets remain in TextCell and Stop.
    pub display: Arc<str>,
}
#[derive(Clone, Debug)]
pub struct TextLine {
    pub range: Range<usize>,
    pub y: f32,
    pub width: f32,
    pub stops: Vec<Stop>,
    pub cells: Vec<TextCell>,
    pub runs: Vec<TextRun>,
}
#[derive(Clone, Debug)]
pub struct Paragraph {
    pub text: Arc<str>,
    pub style: TextStyle,
    pub revision: u64,
    pub service_revision: u64,
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
                l.stops
                    .iter()
                    .find(|s| s.caret == caret)
                    .or_else(|| l.stops.iter().find(|s| s.caret.byte == caret.byte))
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
                l.stops
                    .iter()
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
            .stops
            .iter()
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
                l.stops.iter().min_by(|a, b| {
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
                l.stops.iter().min_by(|a, b| {
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
                    .cells
                    .iter()
                    .filter(|cell| cell.range.start < range.end && cell.range.end > range.start)
                    .collect();
                cells.sort_by(|a, b| a.x.total_cmp(&b.x));
                for cell in cells {
                    let r = Rect::new(cell.x, l.y, cell.width, self.line_height);
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
pub struct TextRequest {
    pub text: Arc<str>,
    pub style: TextStyle,
    pub width: Option<f32>,
    pub revision: u64,
}
pub trait TextEngine {
    fn revision(&self) -> u64 {
        0
    }
    fn layout(&mut self, request: TextRequest) -> Arc<Paragraph>;
}
/// Deterministic metrics for behavioral tests, never substituted for native shaping.
pub struct TestText;
impl TextEngine for TestText {
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
                        x: 0.,
                        display: Arc::from(&r.text[start..byte]),
                    }],
                    stops: std::mem::take(&mut stops),
                });
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
                x: 0.,
                display: Arc::from(&r.text[start..]),
            }],
            stops,
        });
        let size = Size::new(
            lines.iter().map(|l| l.width).fold(0., f32::max),
            lines.len() as f32 * height,
        );
        Arc::new(Paragraph {
            text: r.text,
            style: r.style,
            revision: r.revision,
            service_revision: 0,
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
            range: w[0].caret.byte..w[1].caret.byte,
            x: w[0].x.min(w[1].x),
            width: (w[1].x - w[0].x).abs(),
        })
        .collect()
}
