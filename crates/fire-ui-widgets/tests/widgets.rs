use fire_ui::*;
use fire_ui_widgets::*;
use std::{rc::Rc, sync::Arc};

fn settle<W: Widget>(ui: &mut Ui<W>, text: &mut dyn TextEngine) {
    for _ in 0..20 {
        ui.pump(4096, |_| {}, |_| {});
        ui.layout(text);
        if !ui.next_work().ready {
            return;
        }
    }
    panic!("UI did not settle");
}
fn key<W: Widget>(ui: &mut Ui<W>, text: &mut dyn TextEngine, key: Key) {
    ui.dispatch(
        Input::Key {
            key,
            physical: 1,
            down: true,
            repeat: false,
            modifiers: Modifiers::default(),
        },
        text,
    );
}
#[test]
fn editor_grapheme_delete_undo_and_silent_set() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new("a👩‍👩‍👧‍👦e\u{301}")),
        Size::new(300., 140.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    settle(&mut ui, &mut text);
    let id = ui
        .semantics()
        .into_iter()
        .find(|n| n.semantics.role == Role::TextInput)
        .unwrap()
        .id;
    ui.accessibility(id, SemanticAction::Focus);
    let end = ui.root().text().len();
    ui.send(Edit::Select {
        anchor: end,
        caret: end,
    })
    .unwrap();
    settle(&mut ui, &mut text);
    key(&mut ui, &mut text, Key::Backspace);
    settle(&mut ui, &mut text);
    assert_eq!(ui.root().text(), "a👩‍👩‍👧‍👦");
    key(&mut ui, &mut text, Key::Backspace);
    settle(&mut ui, &mut text);
    assert_eq!(ui.root().text(), "a");
    ui.send(Edit::Undo).unwrap();
    settle(&mut ui, &mut text);
    assert_eq!(ui.root().text(), "a👩‍👩‍👧‍👦");
    ui.send(Edit::Redo).unwrap();
    settle(&mut ui, &mut text);
    assert_eq!(ui.root().text(), "a");
    ui.send(Edit::Set("new value".into())).unwrap();
    let mut outputs = vec![];
    ui.pump(100, |o| outputs.push(o), |_| {});
    assert!(outputs.is_empty());
    assert_eq!(ui.root().text(), "new value");
}
#[test]
fn paragraph_is_shared_across_paint_only_theme_and_height_resize() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new(
            "some text that wraps when the width becomes narrow",
        )),
        Size::new(300., 140.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    settle(&mut ui, &mut text);
    let original = ui.root().paragraph().unwrap().clone();
    ui.resize(Size::new(300., 200.));
    settle(&mut ui, &mut text);
    assert!(Arc::ptr_eq(&original, ui.root().paragraph().unwrap()));
    let theme = Theme {
        foreground: Color::hex(0xff0000),
        ..Theme::default()
    };
    ui.set_environment(Rc::new(theme), false);
    settle(&mut ui, &mut text);
    assert!(Arc::ptr_eq(&original, ui.root().paragraph().unwrap()));
    ui.resize(Size::new(150., 200.));
    settle(&mut ui, &mut text);
    assert!(!Arc::ptr_eq(&original, ui.root().paragraph().unwrap()));
}
#[test]
fn virtual_list_keeps_mounts_bounded_for_one_hundred_thousand_rows() {
    let list = VirtualList::new((0..100_000usize).collect(), 30., |key: &usize| {
        Element::leaf(Label::new(key.to_string()))
    });
    let mut ui = Ui::new(
        Element::leaf(list),
        Size::new(300., 300.),
        Limits {
            nodes: 64,
            ..Limits::default()
        },
    )
    .unwrap();
    let mut text = TestText;
    settle(&mut ui, &mut text);
    assert!(ui.root().mounted_rows() <= 14);
    assert!(ui.stats().nodes <= 15);
    let old = ui.root().row(&0).unwrap();
    ui.send(ListCommand::ScrollTo(99_999)).unwrap();
    settle(&mut ui, &mut text);
    assert!(ui.root().mounted_rows() <= 14);
    assert!(ui.stats().nodes <= 15);
    assert!(ui.root().row(&99_999).is_some());
    assert!(ui.geometry(old).is_none());
    let retained = ui.root().row(&99_999).unwrap();
    ui.send(ListCommand::Keys(vec![99_998, 99_999])).unwrap();
    settle(&mut ui, &mut text);
    assert!(ui.geometry(retained).is_some());
    assert_eq!(ui.root().mounted_rows(), 2);
}
#[test]
fn clean_pointer_motion_does_not_remeasure_or_republish_geometry() {
    let mut ui = Ui::new(
        Element::leaf(Label::new("still")),
        Size::new(200., 80.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    settle(&mut ui, &mut text);
    let before = ui.stats();
    for x in 0..100 {
        ui.dispatch(
            Input::Pointer {
                pointer: 0,
                position: Point::new(x as f32, 20.),
            },
            &mut text,
        );
    }
    let after = ui.stats();
    assert_eq!(before.layouts, after.layouts);
    assert_eq!(before.geometries, after.geometries);
}

#[test]
fn editor_focus_and_blink_recover_after_blur_and_occlusion() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new("test")),
        Size::new(300., 140.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    settle(&mut ui, &mut text);
    let id = ui.semantics()[0].id;
    ui.accessibility(id, SemanticAction::Focus);
    assert!(ui.ime_cursor().is_some());
    assert!(ui.next_work().deadline.is_some());
    ui.window_focus(false);
    assert!(ui.ime_cursor().is_none());
    assert!(ui.next_work().deadline.is_none());
    ui.window_focus(true);
    assert!(ui.ime_cursor().is_some());
    assert!(ui.next_work().deadline.is_some());
    ui.window_visible(false);
    assert!(ui.ime_cursor().is_none());
    assert!(ui.next_work().deadline.is_none());
    ui.window_visible(true);
    settle(&mut ui, &mut text);
    assert!(ui.ime_cursor().is_some());
    assert!(ui.next_work().deadline.is_some());
}

#[test]
fn quiet_editor_keeps_a_caret_without_scheduling_idle_work() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new("Quiet writing").caret_blink(false)),
        Size::new(300., 140.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    settle(&mut ui, &mut text);
    let id = ui.semantics()[0].id;
    ui.accessibility(id, SemanticAction::Focus);
    ui.send(Edit::Insert("A".into())).unwrap();
    settle(&mut ui, &mut text);
    assert!(ui.ime_cursor().is_some());
    assert!(ui.next_work().deadline.is_none());
    assert!(!ui.next_work().ready);
}

#[test]
fn rejected_oversized_edit_preserves_selection_content_and_undo() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new("café").max_bytes(6)),
        Size::new(300., 100.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    settle(&mut ui, &mut text);
    ui.send(Edit::Select {
        anchor: 5,
        caret: 5,
    })
    .unwrap();
    settle(&mut ui, &mut text);
    ui.send(Edit::Insert("!".into())).unwrap();
    settle(&mut ui, &mut text);
    ui.send(Edit::Select {
        anchor: 0,
        caret: 6,
    })
    .unwrap();
    settle(&mut ui, &mut text);
    ui.send(Edit::Insert("too long".into())).unwrap();
    let mut rejected = false;
    ui.pump(
        100,
        |o| rejected |= matches!(o, EditorOutput::LimitReached),
        |_| {},
    );
    assert!(rejected);
    assert_eq!(ui.root().text(), "café!");
    assert_eq!(ui.root().selection(), Some(0..6));
    ui.send(Edit::Undo).unwrap();
    settle(&mut ui, &mut text);
    assert_eq!(ui.root().text(), "café");
}
