use femtovg::{FontId, TextContext};
use fire_ui::*;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;
/// The paragraph service and GPU painter share this font context.
pub struct NativeText {
    pub(crate) context: TextContext,
    pub(crate) fonts: Vec<FontId>,
    revision: u64,
    pub layouts: u64,
}
impl NativeText {
    pub fn new() -> Result<Self, String> {
        let context = TextContext::default();
        let mut fonts = vec![];
        if let Ok(path) = std::env::var("FIRE_UI_FONT") {
            fonts.push(context.add_font_file(path).map_err(|e| e.to_string())?)
        } else {
            for path in [
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/TTF/DejaVuSans.ttf",
                "/System/Library/Fonts/Supplemental/Arial.ttf",
                "C:\\Windows\\Fonts\\segoeui.ttf",
            ] {
                if let Ok(id) = context.add_font_file(path) {
                    fonts.push(id);
                    break;
                }
            }
        }
        if fonts.is_empty() {
            return Err("No default font found; set FIRE_UI_FONT to a TrueType font file".into());
        }
        Ok(Self {
            context,
            fonts,
            revision: 0,
            layouts: 0,
        })
    }
    pub fn add_font(&mut self, path: impl AsRef<std::path::Path>) -> Result<u32, String> {
        let font = self
            .context
            .add_font_file(path)
            .map_err(|e| e.to_string())?;
        self.fonts.push(font);
        self.revision += 1;
        Ok((self.fonts.len() - 1) as u32)
    }
    pub(crate) fn configure(&self, paint: &mut femtovg::Paint, style: TextStyle) {
        let index = (style.font as usize).min(self.fonts.len() - 1);
        paint.set_font(&self.fonts[index..]);
        paint.set_font_size(style.size);
        paint.set_text_baseline(femtovg::Baseline::Top);
    }
    fn stops(&self, text: &str, style: TextStyle) -> (f32, Vec<Stop>) {
        let mut paint = femtovg::Paint::default();
        self.configure(&mut paint, style);
        let Ok(metrics) = self.context.measure_text(0., 0., text, &paint) else {
            return (
                0.,
                vec![Stop {
                    caret: Caret::at(0),
                    x: 0.,
                }],
            );
        };
        let width = metrics.width();
        let mut clusters: Vec<_> = metrics
            .glyphs
            .iter()
            .map(|g| (g.byte_index, g.x - g.bearing_x - g.offset_x))
            .collect();
        clusters.sort_by_key(|(b, _)| *b);
        clusters.dedup_by_key(|(b, _)| *b);
        if clusters.first().is_none_or(|(b, _)| *b != 0) {
            clusters.insert(0, (0, 0.))
        }
        clusters.push((text.len(), width));
        let boundaries: Vec<_> = text
            .grapheme_indices(true)
            .map(|(b, _)| b)
            .chain(std::iter::once(text.len()))
            .collect();
        let mut stops = Vec::with_capacity(boundaries.len());
        let mut cluster = 0;
        for byte in boundaries {
            while cluster + 1 < clusters.len() && clusters[cluster + 1].0 <= byte {
                cluster += 1
            }
            let (start, x) = clusters[cluster];
            let (end, right) = clusters
                .get(cluster + 1)
                .copied()
                .unwrap_or((text.len(), width));
            let x = if byte == start || end == start {
                x
            } else {
                let total = text[start..end].graphemes(true).count().max(1) as f32;
                let before = text[start..byte].graphemes(true).count() as f32;
                x + (right - x) * before / total
            };
            stops.push(Stop {
                caret: Caret::at(byte),
                x,
            });
        }
        (width, stops)
    }
}
impl TextEngine for NativeText {
    fn revision(&self) -> u64 {
        self.revision
    }
    fn layout(&mut self, r: TextRequest) -> Arc<Paragraph> {
        self.layouts += 1;
        let height = r.style.size * 1.5;
        let mut lines = vec![];
        let mut base = 0;
        for logical in r.text.split('\n') {
            let (width, stops) = self.stops(logical, r.style);
            if r.width.is_none_or(|limit| width <= limit) || logical.is_empty() {
                lines.push(TextLine {
                    range: base..base + logical.len(),
                    y: lines.len() as f32 * height,
                    width,
                    stops: stops
                        .into_iter()
                        .map(|s| Stop {
                            caret: Caret::at(s.caret.byte + base),
                            x: s.x,
                        })
                        .collect(),
                });
            } else {
                let limit = r.width.unwrap().max(1.);
                let mut start = 0;
                while start + 1 < stops.len() {
                    let mut end = start + 1;
                    while end + 1 < stops.len()
                        && (stops[end + 1].x - stops[start].x).abs() <= limit
                    {
                        end += 1
                    }
                    let from = stops[start].caret.byte;
                    // Prefer a whole word; an overlong word still breaks at a grapheme.
                    if end + 1 < stops.len() {
                        if let Some(boundary) = (start + 1..=end).rev().find(|i| {
                            logical[..stops[*i].caret.byte]
                                .chars()
                                .next_back()
                                .is_some_and(char::is_whitespace)
                        }) {
                            end = boundary;
                        }
                    }
                    let to = stops[end].caret.byte;
                    let (width, run) = self.stops(&logical[from..to], r.style);
                    lines.push(TextLine {
                        range: base + from..base + to,
                        y: lines.len() as f32 * height,
                        width,
                        stops: run
                            .into_iter()
                            .map(|s| Stop {
                                caret: Caret {
                                    byte: base + from + s.caret.byte,
                                    affinity: if s.caret.byte == 0 {
                                        Affinity::Downstream
                                    } else {
                                        Affinity::Upstream
                                    },
                                },
                                x: s.x,
                            })
                            .collect(),
                    });
                    start = end;
                }
            }
            base += logical.len() + 1;
        }
        let size = Size::new(
            lines.iter().map(|l| l.width).fold(0., f32::max),
            lines.len() as f32 * height,
        );
        Arc::new(Paragraph {
            text: r.text,
            style: r.style,
            revision: r.revision,
            service_revision: self.revision,
            width: r.width,
            size,
            baseline: r.style.size * 1.1,
            line_height: height,
            lines,
        })
    }
}
