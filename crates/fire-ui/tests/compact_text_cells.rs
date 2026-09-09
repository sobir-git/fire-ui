use fire_ui::{Caret, TextCell, TextLine};

#[test]
fn custom_bidi_cells_keep_logical_ranges_independent_of_visual_order() {
    // A line beginning at byte 100 contains é, א, and an invisible three-byte character.
    let line = TextLine {
        range: 100..107,
        y: 24.,
        width: 25.,
        runs: vec![],
        cells: vec![
            TextCell {
                end: 102,
                x: 20.,
                advance: -5.,
            },
            TextCell {
                end: 104,
                x: 10.,
                advance: -5.,
            },
            TextCell {
                end: 107,
                x: 5.,
                advance: 0.,
            },
        ],
    };
    assert_eq!(
        line.cells().map(|(r, _)| r).collect::<Vec<_>>(),
        [100..102, 102..104, 104..107]
    );
    assert_eq!(
        line.cells().rev().map(|(r, _)| r).collect::<Vec<_>>(),
        [104..107, 102..104, 100..102]
    );
    assert_eq!(line.cells().len(), 3);
    assert_eq!(line.cell_at(100).unwrap().x, 20.);
    assert_eq!(line.cell_at(102).unwrap().x, 10.);
    assert_eq!(line.cell_at(106).unwrap().width(), 0.);
    assert!(line.cell_at(99).is_none());
    assert!(line.cell_at(107).is_none());
    assert_eq!(line.cell_range(0), Some(100..102));
    assert_eq!(line.cell_range(2), Some(104..107));
    assert_eq!(line.cell_range(3), None);
    assert_eq!(line.cell_range(usize::MAX), None);
    assert!(line
        .stops()
        .any(|s| s.caret == Caret::at(100) && s.x == 25.));
    assert!(line.stops().any(|s| s.caret.byte == 107 && s.x == 5.));
}

#[test]
fn custom_cluster_can_span_multiple_characters_without_losing_invisible_tail() {
    let line = TextLine {
        range: 0..6,
        y: 0.,
        width: 10.,
        runs: vec![],
        cells: vec![
            TextCell {
                end: 3,
                x: 0.,
                advance: 10.,
            }, // ffi cluster
            TextCell {
                end: 6,
                x: 10.,
                advance: 0.,
            }, // zero-width source character
        ],
    };
    assert_eq!(line.cell_range(0), Some(0..3));
    assert_eq!(line.cell_at(2).unwrap().width(), 10.);
    assert_eq!(line.cell_at(3).unwrap().width(), 0.);
    let bytes: Vec<_> = line.stops().map(|s| s.caret.byte).collect();
    assert_eq!(bytes, [0, 3, 6]);
}

#[test]
fn empty_custom_line_keeps_its_nonzero_source_caret() {
    let line = TextLine {
        range: 19..19,
        y: 48.,
        width: 0.,
        cells: vec![],
        runs: vec![],
    };
    assert_eq!(line.cells().len(), 0);
    assert!(line.cell_range(0).is_none());
    assert!(line.cell_at(19).is_none());
    let stops: Vec<_> = line.stops().collect();
    assert_eq!(stops.len(), 1);
    assert_eq!(stops[0].caret, Caret::at(19));
}
