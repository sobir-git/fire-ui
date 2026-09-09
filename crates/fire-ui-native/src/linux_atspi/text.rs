//! One current paragraph reference per text node. D-Bus objects own the store,
//! never a historical paragraph. AccessKit keeps text events and control metadata.
use fire_ui::{Caret, Paragraph, Point, Rect, SemanticNode, Transform};
use std::{collections::HashMap, ops::Range, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;
use zbus::fdo;

pub(crate) type TextSnapshots = HashMap<u64, TextSnapshot>;
#[derive(Debug)]
pub(crate) struct TextSnapshot {
    pub paragraph: Arc<Paragraph>,
    pub origin: Point,
    pub transform: Transform,
    pub bounds: Rect,
    pub scale: f64,
    pub anchor: usize,
    pub caret: Caret,
}
pub(crate) fn snapshots(nodes: &[SemanticNode], scale: f64) -> TextSnapshots {
    nodes
        .iter()
        .filter_map(|n| {
            let text = n.semantics.text.as_ref()?;
            Some((
                n.id,
                TextSnapshot {
                    paragraph: text.paragraph.clone()?,
                    origin: text.origin,
                    transform: n.transform,
                    bounds: n.bounds,
                    scale,
                    anchor: text.anchor,
                    caret: text.caret,
                },
            ))
        })
        .collect()
}
fn invalid() -> fdo::Error {
    fdo::Error::InvalidArgs("text offset is outside the document".into())
}
impl TextSnapshot {
    pub fn byte(&self, offset: i32) -> fdo::Result<usize> {
        if offset < 0 {
            return Err(invalid());
        }
        self.paragraph
            .text
            .char_indices()
            .map(|(b, _)| b)
            .chain(std::iter::once(self.paragraph.text.len()))
            .nth(offset as usize)
            .ok_or_else(invalid)
    }
    pub fn scalar(&self, byte: usize) -> i32 {
        self.paragraph.text[..byte].chars().count() as i32
    }
    pub fn range(&self, start: i32, end: i32) -> fdo::Result<Range<usize>> {
        let start = self.byte(start)?;
        let end = if end == -1 {
            self.paragraph.text.len()
        } else {
            self.byte(end)?
        };
        if start > end {
            return Err(invalid());
        }
        Ok(start..end)
    }
    pub fn window_rect(&self, rect: Rect) -> Rect {
        self.transform.rect(Rect::new(
            rect.x + self.origin.x,
            rect.y + self.origin.y,
            rect.width,
            rect.height,
        ))
    }
    pub fn character_rect(&self, byte: usize) -> Rect {
        let p = &self.paragraph;
        if byte == p.text.len() {
            let point = p.caret_point(Caret::at(byte));
            return Rect::new(point.x, point.y, 0., p.line_height);
        }
        let index = p
            .lines
            .partition_point(|l| l.range.start <= byte)
            .saturating_sub(1);
        if let Some(line) = p.lines.get(index) {
            let cell = line.cell_at(byte);
            return cell.map_or(Rect::new(line.width, line.y, 0., p.line_height), |c| {
                Rect::new(c.x, line.y, c.width(), p.line_height)
            });
        }
        Rect::new(0., 0., 0., p.line_height)
    }
    pub fn range_rect(&self, range: Range<usize>) -> Rect {
        if range.is_empty() {
            let mut rect = self.character_rect(range.start);
            rect.width = 0.;
            return rect;
        }
        let p = &self.paragraph;
        let mut result: Option<Rect> = None;
        for (index, line) in p.lines.iter().enumerate() {
            let end = p
                .lines
                .get(index + 1)
                .map_or(p.text.len(), |l| l.range.start);
            if line.range.start >= range.end || end <= range.start {
                continue;
            }
            for (cell_range, cell) in line.cells() {
                if cell_range.start < range.end && cell_range.end > range.start {
                    let r = Rect::new(cell.x, line.y, cell.width(), p.line_height);
                    result = Some(result.map_or(r, |old| old.union(r)));
                }
            }
            if line.range.end < end && line.range.end < range.end && end > range.start {
                let r = Rect::new(line.width, line.y, 0., p.line_height);
                result = Some(result.map_or(r, |old| old.union(r)));
            }
        }
        result.unwrap_or_else(|| self.character_rect(range.start))
    }
    pub fn text_range(
        &self,
        offset: i32,
        granularity: atspi::Granularity,
    ) -> fdo::Result<Range<usize>> {
        let byte = self.byte(offset)?;
        let value = &self.paragraph.text;
        if byte == value.len() && granularity == atspi::Granularity::Char {
            return Ok(byte..byte);
        }
        match granularity {
            atspi::Granularity::Char => {
                let range = value
                    .grapheme_indices(true)
                    .map(|(b, g)| b..b + g.len())
                    .find(|r| r.contains(&byte))
                    .ok_or_else(invalid)?;
                if range.len() > u8::MAX as usize {
                    Ok(byte..byte + value[byte..].chars().next().ok_or_else(invalid)?.len_utf8())
                } else {
                    Ok(range)
                }
            }
            atspi::Granularity::Line => {
                let index = self
                    .paragraph
                    .lines
                    .partition_point(|l| l.range.start <= byte)
                    .saturating_sub(1);
                let start = self.paragraph.lines[index].range.start;
                let end = self
                    .paragraph
                    .lines
                    .get(index + 1)
                    .map_or(value.len(), |l| l.range.start);
                Ok(start..end)
            }
            atspi::Granularity::Paragraph => {
                let start = value[..byte].rfind('\n').map_or(0, |b| b + 1);
                let end = value[byte..]
                    .find('\n')
                    .map_or(value.len(), |b| byte + b + 1);
                Ok(start..end)
            }
            atspi::Granularity::Word => {
                // Leading whitespace is its own word in each paragraph. A
                // preceding word includes its newline, not the next paragraph.
                let paragraph_start = value[..byte].rfind('\n').map_or(0, |b| b + 1);
                let paragraph_end = value[byte..]
                    .find('\n')
                    .map_or(value.len(), |b| byte + b + 1);
                let paragraph = &value[paragraph_start..paragraph_end];
                let start = paragraph
                    .unicode_word_indices()
                    .map(|(b, _)| paragraph_start + b)
                    .take_while(|b| *b <= byte)
                    .last()
                    .unwrap_or(paragraph_start);
                let end = paragraph
                    .unicode_word_indices()
                    .map(|(b, _)| paragraph_start + b)
                    .find(|b| *b > start)
                    .unwrap_or(paragraph_end);
                Ok(start..end)
            }
            atspi::Granularity::Sentence => Err(fdo::Error::NotSupported(
                "sentence granularity is not provided".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit_atspi_common::{
        Adapter, AdapterCallback, AppContext, Event, FullNodeId, WindowBounds,
    };
    use fire_ui::{Element, Limits, Size, Ui};
    use fire_ui_widgets::Editor;
    struct Callback;
    impl AdapterCallback for Callback {
        fn register_interfaces(&self, _: &Adapter, _: FullNodeId, _: atspi::InterfaceSet) {}
        fn unregister_interfaces(&self, _: &Adapter, _: FullNodeId, _: atspi::InterfaceSet) {}
        fn emit_event(&self, _: &Adapter, _: Event) {}
    }
    impl accesskit::ActionHandler for Callback {
        fn do_action(&mut self, _: accesskit::ActionRequest) {}
    }
    fn source(value: &str, width: f32) -> Vec<SemanticNode> {
        let mut ui = Ui::new(
            Element::leaf(Editor::new(value)),
            Size::new(width, 100.),
            Limits::default(),
        )
        .unwrap();
        ui.pump(100, |_| {}, |_| {});
        ui.layout(
            &mut fire_ui_text::Text::new(
                fire_ui_fonts::Fonts::load(&["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"])
                    .unwrap(),
            )
            .unwrap(),
        );
        ui.semantics()
    }
    #[test]
    fn text_boundaries_match_positioned_accesskit() {
        for value in [
            "  alpha, beta!\n  שלום e\u{301} end",
            "one\n\nlast\n",
            "",
            "a\r\nb",
        ] {
            let source = source(value, 180.);
            let mut snaps = snapshots(&source, 1.);
            let s = snaps.remove(&source[0].id).unwrap();
            let app = AppContext::new(None);
            let a = Adapter::new(
                &app,
                Callback,
                crate::accessibility::AccessibilityTree::new(true).tree(source, "test", 1.),
                true,
                WindowBounds::default(),
                Callback,
            );
            let root = a.platform_node(a.root_id());
            let n = a.platform_node(root.child_at_index(0).unwrap().unwrap());
            let mut differences = vec![];
            for offset in 0..=value.chars().count() as i32 {
                for g in [
                    atspi::Granularity::Char,
                    atspi::Granularity::Word,
                    atspi::Granularity::Line,
                    atspi::Granularity::Paragraph,
                ] {
                    let r = s.text_range(offset, g).unwrap();
                    let actual = (
                        value[r.clone()].to_string(),
                        s.scalar(r.start),
                        s.scalar(r.end),
                    );
                    let expected = n.string_at_offset(offset, g).unwrap();
                    if actual != expected {
                        differences.push(format!(
                            "{offset} {g:?}: actual {actual:?}, expected {expected:?}"
                        ));
                    }
                }
            }
            assert!(
                differences.is_empty(),
                "{value:?}: {}",
                differences.join("\n")
            );
        }
    }
    #[test]
    fn character_and_range_geometry_matches_positioned_accesskit() {
        for value in ["abc e\u{301} def\nsecond line", "x\n\n", ""] {
            let source = source(value, 100.);
            let mut snaps = snapshots(&source, 1.);
            let s = snaps.remove(&source[0].id).unwrap();
            let app = AppContext::new(None);
            let a = Adapter::new(
                &app,
                Callback,
                crate::accessibility::AccessibilityTree::new(true).tree(source, "test", 1.),
                true,
                WindowBounds::default(),
                Callback,
            );
            let root = a.platform_node(a.root_id());
            let n = a.platform_node(root.child_at_index(0).unwrap().unwrap());
            let convert = |r: Rect| {
                let r = s.window_rect(r);
                accesskit_atspi_common::Rect {
                    x: r.x as i32,
                    y: r.y as i32,
                    width: r.width as i32,
                    height: r.height as i32,
                }
            };
            let count = value.chars().count() as i32;
            let mut differences = vec![];
            for offset in 0..=count {
                let actual = convert(s.character_rect(s.byte(offset).unwrap()));
                let expected = n
                    .character_extents(offset, atspi::CoordType::Window)
                    .unwrap();
                if actual != expected {
                    differences.push(format!("char {offset}: {actual:?} != {expected:?}"));
                }
                for end in [offset, count] {
                    let actual = convert(s.range_rect(s.range(offset, end).unwrap()));
                    let expected = n
                        .range_extents(offset, end, atspi::CoordType::Window)
                        .unwrap();
                    if actual != expected {
                        differences
                            .push(format!("range {offset}..{end}: {actual:?} != {expected:?}"));
                    }
                }
            }
            assert!(
                differences.is_empty(),
                "{value:?}: {}",
                differences.join("\n")
            );
        }
    }

    #[test]
    fn bidi_ranges_cover_all_selected_cells_and_eof_uses_logical_caret() {
        let source = source("שלום", 180.);
        let mut store = snapshots(&source, 2.);
        let s = store.remove(&source[0].id).unwrap();
        for offset in 0..4 {
            let range = s.range(offset, 4).unwrap();
            let expected = s
                .paragraph
                .selection(range.clone())
                .into_iter()
                .reduce(|a, b| a.union(b))
                .unwrap();
            assert_eq!(s.range_rect(range), expected);
        }
        let end = s.paragraph.text.len();
        let point = s.paragraph.caret_point(Caret::at(end));
        assert_eq!(
            s.character_rect(end),
            Rect::new(point.x, point.y, 0., s.paragraph.line_height)
        );
        assert!(point.x < s.paragraph.lines[0].width);
    }

    #[test]
    fn offsets_are_scalars_and_long_graphemes_keep_accesskit_fallback() {
        let value = format!("a{}", "\u{301}".repeat(300));
        let source = source(&value, 300.);
        let mut store = snapshots(&source, 1.);
        let s = store.remove(&source[0].id).unwrap();
        assert_eq!(s.byte(301).unwrap(), 601);
        assert!(s.byte(-1).is_err());
        assert!(s.byte(302).is_err());
        assert!(s.range(3, 2).is_err());
        assert_eq!(s.range(0, -1).unwrap(), 0..601);
        assert_eq!(
            s.text_range(299, atspi::Granularity::Char).unwrap(),
            597..599
        );
        assert!(s.text_range(301, atspi::Granularity::Sentence).is_err());
    }

    #[test]
    fn visual_wrapping_does_not_create_word_boundaries() {
        let value = "supercalifragilisticexpialidocious";
        let source = source(value, 60.);
        let mut store = snapshots(&source, 1.);
        let s = store.remove(&source[0].id).unwrap();
        assert!(s.paragraph.lines.len() > 1);
        for i in 0..=value.len() as i32 {
            assert_eq!(
                s.text_range(i, atspi::Granularity::Word).unwrap(),
                0..value.len()
            );
        }
    }
    #[test]
    fn replacing_and_clearing_snapshots_releases_old_paragraphs() {
        let first = source("old text", 180.);
        let old = Arc::downgrade(
            first[0]
                .semantics
                .text
                .as_ref()
                .unwrap()
                .paragraph
                .as_ref()
                .unwrap(),
        );
        let mut store = snapshots(&first, 1.);
        assert_eq!(store.len(), 1);
        drop(first);
        assert!(old.upgrade().is_some());
        let next = source("new text", 180.);
        let current = Arc::downgrade(
            next[0]
                .semantics
                .text
                .as_ref()
                .unwrap()
                .paragraph
                .as_ref()
                .unwrap(),
        );
        store = snapshots(&next, 1.);
        drop(next);
        assert!(old.upgrade().is_none());
        assert!(current.upgrade().is_some());
        store.clear();
        assert!(current.upgrade().is_none());
    }
}
