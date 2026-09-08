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
    pub fn with_font(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let context = TextContext::default();
        let font = context.add_font_file(path).map_err(|e| e.to_string())?;
        Ok(Self {
            context,
            fonts: vec![font],
            revision: 0,
            layouts: 0,
        })
    }
    /// Explicitly discover and load one system font. No fallback faces are added.
    pub fn new() -> Result<Self, String> {
        Self::with_font(system_font().ok_or("No system font found; supply a font file")?)
    }
    /// No font discovery or font data allocation. Text requires `add_font` first.
    pub fn empty() -> Self {
        Self {
            context: TextContext::default(),
            fonts: vec![],
            revision: 0,
            layouts: 0,
        }
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
        let index = (style.font as usize).min(self.fonts.len().saturating_sub(1));
        paint.set_font(&self.fonts[index..]);
        paint.set_font_size(style.size);
        paint.set_text_baseline(femtovg::Baseline::Top);
    }
    fn shape_line(
        &self,
        text: &str,
        range: std::ops::Range<usize>,
        bidi: &unicode_bidi::BidiInfo<'_>,
        style: TextStyle,
    ) -> TextLine {
        let mut line = TextLine {
            range: range.clone(),
            y: 0.,
            width: 0.,
            stops: vec![],
            cells: vec![],
            runs: vec![],
        };
        if range.is_empty() {
            line.stops.push(Stop {
                caret: Caret::at(range.start),
                x: 0.,
            });
            return line;
        }
        let para = bidi
            .paragraphs
            .iter()
            .find(|p| p.range.contains(&range.start))
            .unwrap();
        let (levels, runs) = bidi.visual_runs(para, range);
        let mut paint = femtovg::Paint::default();
        self.configure(&mut paint, style);
        for run in runs {
            let rtl = levels[run.start].is_rtl();
            let mut pieces = vec![];
            let mut start = run.start;
            for (b, ch) in text[run.clone()].char_indices() {
                if ch == '\t' {
                    pieces.push(start..run.start + b);
                    pieces.push(run.start + b..run.start + b + 1);
                    start = run.start + b + 1;
                }
            }
            pieces.push(start..run.end);
            if rtl {
                pieces.reverse();
            }
            for part in pieces {
                let value = &text[part.clone()];
                if value.is_empty() {
                    continue;
                }
                let (width, clusters, display) = if value == "\t" {
                    let width = self
                        .context
                        .measure_text(0., 0., " ", &paint)
                        .map_or(style.size * 0.6, |m| m.width())
                        * 4.;
                    (width, vec![(0, 0., width)], None)
                } else {
                    // Resolve paragraph direction once, then force that direction for shaping and painting.
                    let display: Arc<str> = format!(
                        "{}{}\u{202c}",
                        if rtl { '\u{202e}' } else { '\u{202d}' },
                        value
                    )
                    .into();
                    let metrics = self
                        .context
                        .measure_text(0., 0., &display, &paint)
                        .unwrap_or_default();
                    let mut clusters: std::collections::BTreeMap<usize, (f32, f32)> =
                        Default::default();
                    for glyph in &metrics.glyphs {
                        if glyph.byte_index < 3 || glyph.byte_index >= 3 + value.len() {
                            continue;
                        }
                        let byte = glyph.byte_index - 3;
                        let x = glyph.x - glyph.bearing_x - glyph.offset_x;
                        if let Some(cluster) = clusters.get_mut(&byte) {
                            let end = (cluster.0 + cluster.1).max(x + glyph.advance_x);
                            cluster.0 = cluster.0.min(x);
                            cluster.1 = end - cluster.0;
                        } else {
                            clusters.insert(byte, (x, glyph.advance_x));
                        }
                    }
                    clusters
                        .entry(0)
                        .or_insert((if rtl { metrics.width() } else { 0. }, 0.));
                    let clusters = clusters
                        .into_iter()
                        .map(|(byte, (x, width))| (byte, x, width))
                        .collect();
                    (metrics.width(), clusters, Some(display))
                };
                let boundaries: Vec<_> = value
                    .grapheme_indices(true)
                    .map(|(b, _)| b)
                    .chain(std::iter::once(value.len()))
                    .collect();
                for pair in boundaries.windows(2) {
                    let index = clusters
                        .partition_point(|c| c.0 <= pair[0])
                        .saturating_sub(1);
                    let (base, x, advance) = clusters.get(index).copied().unwrap_or((0, 0., 0.));
                    let end = clusters.get(index + 1).map_or(value.len(), |c| c.0);
                    let count = value[base..end].graphemes(true).count().max(1) as f32;
                    let before = value[base..pair[0]].graphemes(true).count() as f32;
                    let cell_width = advance / count;
                    let left = line.width
                        + x
                        + if rtl {
                            advance - (before + 1.) * cell_width
                        } else {
                            before * cell_width
                        };
                    let from = part.start + pair[0];
                    let to = part.start + pair[1];
                    line.cells.push(TextCell {
                        range: from..to,
                        x: left,
                        width: cell_width,
                    });
                    line.stops.push(Stop {
                        caret: Caret::at(from),
                        x: if rtl { left + cell_width } else { left },
                    });
                    line.stops.push(Stop {
                        caret: Caret {
                            byte: to,
                            affinity: Affinity::Upstream,
                        },
                        x: if rtl { left } else { left + cell_width },
                    });
                }
                if let Some(display) = display {
                    line.runs.push(TextRun {
                        x: line.width,
                        display,
                    });
                }
                line.width += width;
            }
        }
        line.stops
            .sort_by_key(|s| (s.caret.byte, s.caret.affinity == Affinity::Upstream));
        line.stops
            .dedup_by(|a, b| a.caret.byte == b.caret.byte && (a.x - b.x).abs() < 0.01);
        line.cells.sort_by_key(|c| c.range.start);
        line
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
            let bidi = unicode_bidi::BidiInfo::new(logical, None);
            let full = self.shape_line(logical, 0..logical.len(), &bidi, r.style);
            let mut shaped = vec![];
            if r.width.is_none_or(|limit| full.width <= limit) || logical.is_empty() {
                shaped.push(full);
            } else {
                let limit = r.width.unwrap().max(1.);
                let mut start = 0;
                while start < full.cells.len() {
                    let mut end = start;
                    let mut width = 0.;
                    while end < full.cells.len()
                        && (end == start || width + full.cells[end].width <= limit)
                    {
                        width += full.cells[end].width;
                        end += 1;
                    }
                    if end < full.cells.len() {
                        if let Some(word) = (start + 1..=end).rev().find(|i| {
                            logical[..full.cells[*i - 1].range.end]
                                .chars()
                                .next_back()
                                .is_some_and(char::is_whitespace)
                        }) {
                            end = word;
                        }
                    }
                    let from = full.cells[start].range.start;
                    let mut line = self.shape_line(
                        logical,
                        from..full.cells[end - 1].range.end,
                        &bidi,
                        r.style,
                    );
                    while line.width > limit && end > start + 1 {
                        end -= 1;
                        line = self.shape_line(
                            logical,
                            from..full.cells[end - 1].range.end,
                            &bidi,
                            r.style,
                        );
                    }
                    shaped.push(line);
                    start = end;
                }
            }
            for mut line in shaped {
                line.y = lines.len() as f32 * height;
                line.range = base + line.range.start..base + line.range.end;
                for stop in &mut line.stops {
                    stop.caret.byte += base;
                }
                for cell in &mut line.cells {
                    cell.range = base + cell.range.start..base + cell.range.end;
                }
                lines.push(line);
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

/// Find one common system font, or the explicit `FIRE_UI_FONT` override.
/// Called only when an application chooses it; the native host never discovers fonts.
pub fn system_font() -> Option<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("FIRE_UI_FONT") {
        return Some(path.into());
    }
    [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
    ]
    .into_iter()
    .map(std::path::PathBuf::from)
    .find(|path| path.is_file())
}

#[cfg(test)]
mod font_tests {
    use super::*;

    #[test]
    fn empty_context_configures_without_font_discovery_or_index_underflow() {
        let text = NativeText::empty();
        assert!(text.fonts.is_empty());
        text.configure(&mut femtovg::Paint::default(), TextStyle::default());
        assert!(NativeText::with_font("/nonexistent-fire-ui-font.ttf").is_err());
    }
}
