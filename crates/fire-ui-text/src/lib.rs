use fire_ui::*;
use fire_ui_fonts::Fonts;
use std::sync::Arc;
use unicode_script::{Script, UnicodeScript};
use unicode_segmentation::UnicodeSegmentation;
// Scratch resources belong to one layout. Plans depend on face and segment
// properties, not text contents; buffer storage can be reused after extracting glyphs.
struct Shaping<'a> {
    faces: Vec<rustybuzz::Face<'a>>,
    plans: Vec<(
        usize,
        rustybuzz::Direction,
        rustybuzz::Script,
        Option<rustybuzz::Language>,
        rustybuzz::ShapePlan,
    )>,
    buffer: Option<rustybuzz::UnicodeBuffer>,
}
impl<'a> Shaping<'a> {
    fn new(fonts: &'a Fonts) -> Self {
        Self {
            faces: fonts
                .iter()
                .map(|font| {
                    rustybuzz::Face::from_slice(font.bytes.as_ref(), font.face_index)
                        .expect("validated immutable font")
                })
                .collect(),
            plans: vec![],
            buffer: None,
        }
    }
}
pub struct Text {
    fonts: Fonts,
    revision: TextRevision,
    pub layouts: u64,
}
impl Text {
    pub fn new(fonts: Fonts) -> Result<Self, String> {
        for (entry, font) in fonts.iter().enumerate() {
            rustybuzz::Face::from_slice(font.bytes.as_ref(), font.face_index).ok_or_else(|| {
                format!(
                    "Unsupported shaping font at entry {entry}, face {}",
                    font.face_index
                )
            })?;
        }
        Ok(Self {
            fonts,
            revision: TextRevision::new(),
            layouts: 0,
        })
    }
    fn shape(
        &self,
        value: &str,
        style: TextStyle,
        rtl: bool,
        shaping: &mut Shaping<'_>,
    ) -> (f32, Vec<(usize, f32, f32)>, Vec<TextRun>) {
        if self.fonts.is_empty() {
            return (
                0.,
                vec![],
                vec![TextRun {
                    x: 0.,
                    font: u32::MAX,
                    glyphs: Box::default(),
                }],
            );
        }

        let first = (style.font as usize).min(self.fonts.len() - 1);
        let mut pieces: Vec<(usize, usize, usize, Script)> = vec![];
        let strong = |ch: char| {
            let script = ch.script();
            (!matches!(script, Script::Common | Script::Inherited | Script::Unknown))
                .then_some(script)
        };
        let mut current_script = value.chars().find_map(strong).unwrap_or(Script::Common);
        for (offset, grapheme) in value.grapheme_indices(true) {
            let script = grapheme.chars().find_map(strong).unwrap_or(current_script);
            current_script = script;
            let font = if first + 1 == self.fonts.len() {
                first
            } else {
                (first..self.fonts.len())
                    .find(|&i| {
                        let face = &shaping.faces[i];
                        grapheme.chars().all(|ch| {
                            ch == '\u{200d}' || ch == '\u{fe0f}' || face.glyph_index(ch).is_some()
                        })
                    })
                    .unwrap_or(first)
            };
            if let Some(last) = pieces.last_mut().filter(|p| p.2 == font && p.3 == script) {
                last.1 = offset + grapheme.len();
            } else {
                pieces.push((offset, offset + grapheme.len(), font, script));
            }
        }
        if rtl {
            pieces.reverse();
        }
        let mut width = 0.;
        let mut clusters = Vec::<(usize, f32, f32)>::new();
        let mut runs = Vec::with_capacity(pieces.len());
        for (start, end, font, _) in pieces {
            let face = &shaping.faces[font];
            let scale = style.size / face.units_per_em() as f32;
            let mut buffer = shaping.buffer.take().unwrap_or_default();
            buffer.push_str(&value[start..end]);
            buffer.set_pre_context(&value[..start]);
            buffer.set_post_context(&value[end..]);
            buffer.set_direction(if rtl {
                rustybuzz::Direction::RightToLeft
            } else {
                rustybuzz::Direction::LeftToRight
            });
            buffer.guess_segment_properties();
            let direction = buffer.direction();
            let script = buffer.script();
            let language = buffer.language();
            let index = shaping
                .plans
                .iter()
                .position(|p| p.0 == font && p.1 == direction && p.2 == script && p.3 == language)
                .unwrap_or_else(|| {
                    let plan = rustybuzz::ShapePlan::new(
                        face,
                        direction,
                        Some(script),
                        language.as_ref(),
                        &[],
                    );
                    shaping
                        .plans
                        .push((font, direction, script, language, plan));
                    shaping.plans.len() - 1
                });
            let shaped = rustybuzz::shape_with_plan(face, &shaping.plans[index].4, buffer);
            let mut x = 0.;
            let mut glyphs = Vec::with_capacity(shaped.len());
            for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
                let advance = pos.x_advance as f32 * scale;
                glyphs.push(Glyph {
                    index: info.glyph_id,
                    offset: Point::new(
                        x + pos.x_offset as f32 * scale,
                        -pos.y_offset as f32 * scale,
                    ),
                });
                clusters.push((start + info.cluster as usize, width + x, advance));
                x += advance;
            }
            runs.push(TextRun {
                x: width,
                font: font as u32,
                glyphs: glyphs.into(),
            });
            width += x;
            shaping.buffer = Some(shaped.clear());
        }
        clusters.sort_by_key(|c| c.0);
        clusters.dedup_by(|next, previous| {
            if next.0 == previous.0 {
                previous.2 += next.2;
                true
            } else {
                false
            }
        });
        (width, clusters, runs)
    }
    fn shape_line(
        &self,
        text: &str,
        range: std::ops::Range<usize>,
        bidi: &unicode_bidi::BidiInfo<'_>,
        style: TextStyle,
        shaping: &mut Shaping<'_>,
    ) -> TextLine {
        let mut line = TextLine {
            range: range.clone(),
            y: 0.,
            width: 0.,
            cells: Vec::with_capacity(text[range.clone()].graphemes(true).count()),
            runs: vec![],
        };
        if range.is_empty() {
            return line;
        }
        let para = bidi
            .paragraphs
            .iter()
            .find(|p| p.range.contains(&range.start))
            .unwrap();
        let (levels, runs) = bidi.visual_runs(para, range);

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
                let (width, clusters) = if value == "\t" {
                    let width = self.shape(" ", style, false, shaping).0 * 4.;
                    (width, vec![(0, 0., width)])
                } else {
                    let (width, clusters, runs) = self.shape(value, style, rtl, shaping);
                    for mut run in runs {
                        run.x += line.width;
                        line.runs.push(run);
                    }
                    (width, clusters)
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

                    let to = part.start + pair[1];
                    line.cells.push(TextCell {
                        end: to,
                        x: left,
                        advance: if rtl { -cell_width } else { cell_width },
                    });
                }
                line.width += width;
            }
        }
        line.cells.sort_by_key(|c| c.end);
        line.cells.shrink_to_fit();
        line.runs.shrink_to_fit();
        line
    }
}
impl TextEngine for Text {
    fn revision(&self) -> TextRevision {
        self.revision
    }
    fn layout(&mut self, r: TextRequest) -> Arc<Paragraph> {
        let started = std::time::Instant::now();
        self.layouts += 1;
        let height = r.style.size * 1.5;
        let mut lines = vec![];
        let mut base = 0;
        let previous = r
            .previous
            .filter(|p| p.style == r.style && p.service_revision == self.revision);
        let (prefix, suffix) = previous.map_or((0, 0), |p| {
            let common = p
                .text
                .bytes()
                .zip(r.text.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            let prefix = if common == r.text.len() && common == p.text.len() {
                common
            } else {
                r.text.as_bytes()[..common]
                    .iter()
                    .rposition(|b| *b == b'\n')
                    .map_or(0, |i| i + 1)
            };
            let common = p
                .text
                .bytes()
                .rev()
                .zip(r.text.bytes().rev())
                .take(p.text.len().min(r.text.len()) - prefix)
                .take_while(|(a, b)| a == b)
                .count();
            let suffix = r.text.as_bytes()[r.text.len() - common..]
                .iter()
                .position(|b| *b == b'\n')
                .map_or(0, |i| common - i - 1);
            (prefix, suffix)
        });
        let mut shaping = Shaping::new(&self.fonts);
        for logical in r.text.split('\n') {
            if let Some(p) = previous {
                let old_base = if base + logical.len() <= prefix {
                    Some(base)
                } else if base >= r.text.len() - suffix {
                    Some(p.text.len() - (r.text.len() - base))
                } else {
                    None
                };
                if let Some(old_base) = old_base {
                    let start = p.lines.partition_point(|line| line.range.start < old_base);
                    let end = p
                        .lines
                        .partition_point(|line| line.range.start <= old_base + logical.len());
                    let old_lines = &p.lines[start..end];
                    // At a different width an unwrapped line still fits unchanged.
                    // Wrapped lines must be shaped again because break positions change.
                    let fits = p.width == r.width
                        || (old_lines.len() == 1
                            && r.width.is_none_or(|width| old_lines[0].width <= width));
                    if old_lines
                        .first()
                        .is_some_and(|line| line.range.start == old_base)
                        && old_lines
                            .last()
                            .is_some_and(|line| line.range.end == old_base + logical.len())
                        && fits
                    {
                        for old in old_lines {
                            let mut line = old.clone();
                            line.range = base + (line.range.start - old_base)
                                ..base + (line.range.end - old_base);
                            for cell in &mut line.cells {
                                cell.end = base + (cell.end - old_base);
                            }
                            line.y = lines.len() as f32 * height;
                            lines.push(line);
                        }
                        base += logical.len() + 1;
                        continue;
                    }
                }
            }
            let bidi = unicode_bidi::BidiInfo::new(logical, None);
            let mut full = self.shape_line(logical, 0..logical.len(), &bidi, r.style, &mut shaping);
            let mut shaped = vec![];
            if r.width.is_none_or(|limit| full.width <= limit) || logical.is_empty() {
                shaped.push(full);
            } else {
                full.runs.clear();
                let limit = r.width.unwrap().max(1.);
                let mut start = 0;
                while start < full.cells.len() {
                    let mut end = start;
                    let mut width = 0.;
                    while end < full.cells.len()
                        && (end == start || width + full.cells[end].width() <= limit)
                    {
                        width += full.cells[end].width();
                        end += 1;
                    }
                    if end < full.cells.len() {
                        if let Some(word) = (start + 1..=end).rev().find(|i| {
                            logical[..full.cells[*i - 1].end]
                                .chars()
                                .next_back()
                                .is_some_and(char::is_whitespace)
                        }) {
                            end = word;
                        }
                    }
                    let from = full.cell_range(start).unwrap().start;
                    let mut line = self.shape_line(
                        logical,
                        from..full.cells[end - 1].end,
                        &bidi,
                        r.style,
                        &mut shaping,
                    );
                    while line.width > limit && end > start + 1 {
                        end -= 1;
                        line = self.shape_line(
                            logical,
                            from..full.cells[end - 1].end,
                            &bidi,
                            r.style,
                            &mut shaping,
                        );
                    }
                    shaped.push(line);
                    start = end;
                }
            }
            for mut line in shaped {
                line.y = lines.len() as f32 * height;
                line.range = base + line.range.start..base + line.range.end;
                for cell in &mut line.cells {
                    cell.end += base;
                }
                lines.push(line);
            }
            base += logical.len() + 1;
        }
        let size = Size::new(
            lines.iter().map(|l| l.width).fold(0., f32::max),
            lines.len() as f32 * height,
        );
        lines.shrink_to_fit();
        if std::env::var_os("FIRE_UI_PROFILE").is_some() {
            let bytes = r.text.len()
                + lines.capacity() * std::mem::size_of::<TextLine>()
                + lines
                    .iter()
                    .map(|l| {
                        l.cells.capacity() * std::mem::size_of::<TextCell>()
                            + l.runs.capacity() * std::mem::size_of::<TextRun>()
                            + l.runs
                                .iter()
                                .map(|r| r.glyphs.len() * std::mem::size_of::<Glyph>())
                                .sum::<usize>()
                    })
                    .sum::<usize>();
            eprintln!(
                "paragraph_source_bytes={} paragraph_storage_bytes={bytes} layout_us={}",
                r.text.len(),
                started.elapsed().as_micros()
            );
        }
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
mod plan_tests {
    use super::*;
    #[test]
    fn previous_layout_matches_fresh_after_edits_and_width_changes() {
        let fonts = Fonts::load(&["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"]).unwrap();
        let mut engine = Text::new(fonts).unwrap();
        let original = "first café e\u{301}\nالعربية שלום\nlong wrapped line with many words and a\ttab\nlast\n";
        for before_width in [None, Some(40.), Some(180.)] {
            let previous = engine.layout(TextRequest {
                previous: None,
                text: original.into(),
                style: TextStyle::default(),
                width: before_width,
                revision: 0,
            });
            for value in [
                original.to_string(),
                format!("prefix{original}"),
                original.replace("العربية", "عربي"),
                original.replace('\n', ""),
                original.replace("last", "last\nextra"),
                original.trim_end().to_string(),
                String::new(),
                format!("{original}extra"),
                original.replace("first", ""),
            ] {
                for width in [None, Some(40.), Some(180.)] {
                    let mut request = TextRequest {
                        previous: Some(&previous),
                        text: value.clone().into(),
                        style: TextStyle::default(),
                        width,
                        revision: 1,
                    };
                    let reused = engine.layout(request);
                    request = TextRequest {
                        previous: None,
                        text: value.clone().into(),
                        style: TextStyle::default(),
                        width,
                        revision: 1,
                    };
                    let fresh = engine.layout(request);
                    assert_eq!(
                        format!("{:?}", reused.lines),
                        format!("{:?}", fresh.lines),
                        "text={value:?} widths={before_width:?}->{width:?}"
                    );
                    assert_eq!(reused.size, fresh.size);
                }
            }
        }
    }
    #[test]
    fn reused_plans_and_buffers_match_fresh_shaping_for_alternating_fonts_and_scripts() {
        let fonts = Fonts::load(&[
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ])
        .unwrap();
        let engine = Text::new(fonts.clone()).unwrap();
        let mut shaping = Shaping::new(&fonts);
        let mut plan_count = 0;
        for round in 0..3 {
            for (value, rtl) in [
                ("office café e\u{301}", false),
                ("العربية", true),
                ("שלום", true),
                ("Привет", false),
            ] {
                for font in [0, 1] {
                    let style = TextStyle { font, size: 16. };
                    let (width, _, runs) = engine.shape(value, style, rtl, &mut shaping);
                    assert!(runs.iter().all(|run| run.font == runs[0].font));
                    let face = rustybuzz::Face::from_slice(
                        fonts.get(runs[0].font as usize).unwrap().bytes.as_ref(),
                        fonts.get(runs[0].font as usize).unwrap().face_index,
                    )
                    .unwrap();
                    let mut buffer = rustybuzz::UnicodeBuffer::new();
                    buffer.push_str(value);
                    buffer.set_direction(if rtl {
                        rustybuzz::Direction::RightToLeft
                    } else {
                        rustybuzz::Direction::LeftToRight
                    });
                    let expected = rustybuzz::shape(&face, &[], buffer);
                    let scale = style.size / face.units_per_em() as f32;
                    let glyphs: Vec<_> = runs
                        .iter()
                        .flat_map(|r| r.glyphs.iter().map(move |g| (r.x, g)))
                        .collect();
                    assert_eq!(glyphs.len(), expected.len());
                    let mut x = 0.;
                    for ((origin, g), (info, pos)) in glyphs.iter().zip(
                        expected
                            .glyph_infos()
                            .iter()
                            .zip(expected.glyph_positions()),
                    ) {
                        assert_eq!(g.index, info.glyph_id, "{value}");
                        assert!(
                            (origin + g.offset.x - (x + pos.x_offset as f32 * scale)).abs()
                                < 0.0001,
                            "{value}"
                        );
                        assert_eq!(g.offset.y, -pos.y_offset as f32 * scale);
                        x += pos.x_advance as f32 * scale;
                    }
                    assert!((width - x).abs() < 0.0001);
                }
            }
            if round == 0 {
                plan_count = shaping.plans.len();
            } else {
                assert_eq!(shaping.plans.len(), plan_count);
            }
        }
        assert!(plan_count >= 4);
    }
}
