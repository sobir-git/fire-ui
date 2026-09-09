use fire_ui::*;
use fire_ui_widgets::{Edit, Editor, EditorState};
use std::{sync::Arc, time::Duration};

fn settle(ui: &mut Ui<Editor>) {
    for _ in 0..20 {
        ui.pump(4096, |_| {}, |_| {});
        ui.layout(&mut TestText);
        if !ui.next_work().ready {
            return;
        }
    }
    panic!("editor did not settle");
}
fn editor(wrap: bool) -> (Ui<Editor>, u64) {
    let value = "café a long line which extends outside the narrow viewport\n".repeat(20);
    let mut ui = Ui::new(
        Element::leaf(
            Editor::new(value)
                .chrome(false)
                .padding(0., 0.)
                .restore(EditorState {
                    wrap,
                    ..EditorState::default()
                }),
        ),
        Size::new(120., 80.),
        Limits::default(),
    )
    .unwrap();
    settle(&mut ui);
    let id = ui
        .semantics()
        .into_iter()
        .find(|n| n.semantics.role == Role::TextInput)
        .unwrap()
        .id;
    (ui, id)
}

#[test]
fn semantic_scroll_preserves_text_selection_ime_session_and_undo_redo() {
    let (mut ui, id) = editor(false);
    let original = ui.root().text().to_owned();
    ui.accessibility(id, SemanticAction::Focus).unwrap();
    ui.send(Edit::Insert("first ".into())).unwrap();
    settle(&mut ui);
    let first = ui.root().text().to_owned();
    ui.send(Edit::Insert("second ".into())).unwrap();
    settle(&mut ui);
    let second = ui.root().text().to_owned();
    ui.send(Edit::Undo).unwrap();
    settle(&mut ui);
    ui.accessibility(
        id,
        SemanticAction::SetSelection {
            anchor: 2,
            caret: 8,
        },
    )
    .unwrap();
    settle(&mut ui);
    ui.frame(Duration::from_secs(1), 256);
    settle(&mut ui);
    let selected = ui.root().state();
    let session = ui.session();
    let paragraph = ui.root().paragraph().unwrap().clone();
    ui.accessibility(id, SemanticAction::ScrollBy(Point::new(-30., -40.)))
        .unwrap();
    settle(&mut ui);
    let scrolled = ui.root().state();
    assert_eq!(ui.root().text(), first);
    assert_eq!(
        (scrolled.anchor, scrolled.caret),
        (selected.anchor, selected.caret)
    );
    assert_eq!(ui.session(), session);
    assert_eq!(
        scrolled.scroll,
        Point::new(selected.scroll.x + 30., selected.scroll.y + 40.)
    );
    assert!(Arc::ptr_eq(&paragraph, ui.root().paragraph().unwrap()));
    ui.frame(Duration::from_secs(2), 256);
    settle(&mut ui);
    assert_eq!(
        ui.root().state().scroll,
        scrolled.scroll,
        "scroll must survive the next frame"
    );
    ui.send(Edit::Redo).unwrap();
    settle(&mut ui);
    assert_eq!(ui.root().text(), second);
    ui.send(Edit::Undo).unwrap();
    settle(&mut ui);
    assert_eq!(ui.root().text(), first);
    ui.send(Edit::Undo).unwrap();
    settle(&mut ui);
    assert_eq!(ui.root().text(), original);
}

#[test]
fn semantic_scroll_clamps_vertical_extent_and_suppresses_horizontal_wrapped_scroll() {
    let (mut ui, id) = editor(true);
    let height = ui.root().paragraph().unwrap().size.height;
    ui.accessibility(
        id,
        SemanticAction::ScrollBy(Point::new(-f32::MAX, -f32::MAX)),
    )
    .unwrap();
    settle(&mut ui);
    assert_eq!(ui.root().state().scroll, Point::new(0., height - 80.));
    ui.accessibility(id, SemanticAction::ScrollBy(Point::new(f32::MAX, f32::MAX)))
        .unwrap();
    settle(&mut ui);
    assert_eq!(ui.root().state().scroll, Point::default());
}

#[test]
fn invalid_semantic_scroll_is_atomic() {
    let (mut ui, id) = editor(false);
    ui.accessibility(id, SemanticAction::ScrollBy(Point::new(-20., -30.)))
        .unwrap();
    settle(&mut ui);
    let state = ui.root().state();
    let value = ui.root().text().to_owned();
    let revision = ui.semantic_revision();
    for delta in [
        Point::new(f32::NAN, 0.),
        Point::new(0., f32::NAN),
        Point::new(f32::INFINITY, 0.),
        Point::new(0., f32::NEG_INFINITY),
    ] {
        assert_eq!(
            ui.accessibility(id, SemanticAction::ScrollBy(delta)),
            Err(SemanticError::InvalidValue)
        );
        assert_eq!(ui.root().state(), state);
        assert_eq!(ui.root().text(), value);
        assert_eq!(ui.semantic_revision(), revision);
    }
}

#[test]
fn hidden_editor_releases_paragraph_and_rebuilds_without_losing_history() {
    let (mut ui, id) = editor(false);
    let original = ui.root().text().to_owned();
    ui.send(Edit::Insert("kept edit ".into())).unwrap();
    settle(&mut ui);
    ui.frame(Duration::from_secs(1), 256);
    settle(&mut ui);
    let changed = ui.root().text().to_owned();
    let state = ui.root().state();
    let old = Arc::downgrade(ui.root().paragraph().unwrap());
    ui.window_visible(false);
    settle(&mut ui);
    assert!(ui.root().paragraph().is_none());
    assert!(old.upgrade().is_none());
    assert_eq!(ui.root().text(), changed);
    assert_eq!(
        ui.accessibility(id, SemanticAction::ScrollBy(Point::new(0., -10.))),
        Err(SemanticError::Unavailable)
    );
    ui.window_visible(true);
    settle(&mut ui);
    assert!(ui.root().paragraph().is_some());
    assert_eq!(ui.root().state(), state);
    ui.send(Edit::Undo).unwrap();
    settle(&mut ui);
    assert_eq!(ui.root().text(), original);
    ui.send(Edit::Redo).unwrap();
    settle(&mut ui);
    assert_eq!(ui.root().text(), changed);
}

#[test]
fn semantic_horizontal_scroll_reaches_text_beside_scrollbar_gutter() {
    let (mut ui, id) = editor(false);
    let width = ui.root().paragraph().unwrap().size.width;
    let gutter = fire_ui_widgets::Scrollbar::width(&fire_ui_widgets::Theme::default());
    ui.accessibility(id, SemanticAction::ScrollBy(Point::new(-f32::MAX, 0.)))
        .unwrap();
    settle(&mut ui);
    let x = ui.root().state().scroll.x;
    assert!(
        (x + 120. - gutter - width).abs() < 0.001,
        "the end of the line must be reachable before the scrollbar strip"
    );
    ui.accessibility(id, SemanticAction::ScrollBy(Point::new(f32::MAX, 0.)))
        .unwrap();
    settle(&mut ui);
    assert_eq!(ui.root().state().scroll.x, 0.);
}
