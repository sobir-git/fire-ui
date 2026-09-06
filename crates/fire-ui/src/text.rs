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
pub struct TextLine {
    pub range: Range<usize>,
    pub y: f32,
    pub width: f32,
    pub stops: Vec<Stop>,
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
        let mut lines = self
            .lines
            .iter()
            .filter(|l| l.range.contains(&caret.byte) || l.range.end == caret.byte);
        let line = match caret.affinity {
            Affinity::Upstream => lines.clone().next(),
            Affinity::Downstream => lines.next_back(),
        }
        .or(self.lines.last());
        line.map(|l| {
            Point::new(
                l.stops
                    .iter()
                    .find(|s| s.caret.byte == caret.byte)
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
    pub fn selection(&self, range: Range<usize>) -> Vec<Rect> {
        self.lines
            .iter()
            .filter_map(|l| {
                let start = range.start.max(l.range.start);
                let end = range.end.min(l.range.end);
                if start >= end {
                    return None;
                }
                let x = |b| {
                    l.stops
                        .iter()
                        .find(|s| s.caret.byte == b)
                        .map_or(l.width, |s| s.x)
                };
                let (a, b) = (x(start), x(end));
                Some(Rect::new(a.min(b), l.y, (a - b).abs(), self.line_height))
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
