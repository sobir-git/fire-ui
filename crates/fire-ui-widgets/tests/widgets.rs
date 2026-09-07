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

#[test]
fn focus_requested_before_window_activation_is_restored_on_activation() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new("").caret_blink(false)),
        Size::new(300., 100.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    settle(&mut ui, &mut text);
    ui.window_focus(false);
    let id = ui.semantics()[0].id;
    ui.accessibility(id, SemanticAction::Focus);
    assert!(ui.ime_cursor().is_none());
    ui.window_focus(true);
    assert!(ui.ime_cursor().is_some());
    ui.dispatch(
        Input::Text {
            session: ui.session(),
            text: "ready immediately".into(),
        },
        &mut text,
    );
    settle(&mut ui, &mut text);
    assert_eq!(ui.root().text(), "ready immediately");
}

fn modified_key(ui: &mut Ui<Editor>, key: Key, modifiers: Modifiers) {
    ui.dispatch(
        Input::Key {
            key,
            physical: 1,
            down: true,
            repeat: false,
            modifiers,
        },
        &mut TestText,
    );
    settle(ui, &mut TestText);
}
#[test]
fn moving_selected_lines_preserves_selection_and_is_undoable() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new("one\ntwo\nthree\nfour")),
        Size::new(300., 200.),
        Limits::default(),
    )
    .unwrap();
    settle(&mut ui, &mut TestText);
    ui.accessibility(ui.semantics()[0].id, SemanticAction::Focus);
    ui.send(Edit::Select {
        anchor: 4,
        caret: 8,
    })
    .unwrap();
    settle(&mut ui, &mut TestText);
    modified_key(
        &mut ui,
        Key::Down,
        Modifiers {
            alt: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(ui.root().text(), "one\nthree\ntwo\nfour");
    assert_eq!(ui.root().selection(), Some(10..14));
    modified_key(
        &mut ui,
        Key::Up,
        Modifiers {
            alt: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(ui.root().text(), "one\ntwo\nthree\nfour");
    assert_eq!(ui.root().selection(), Some(4..8));
    ui.send(Edit::Undo).unwrap();
    settle(&mut ui, &mut TestText);
    assert_eq!(ui.root().text(), "one\nthree\ntwo\nfour");
}
#[test]
fn restored_scroll_survives_layout_and_scrollbar_drag_does_not_reveal_caret() {
    let text = (0..200).map(|i| format!("line {i}\n")).collect::<String>();
    let state = EditorState {
        caret: Caret::at(0),
        anchor: None,
        scroll: Point::new(0., 600.),
        wrap: false,
    };
    let mut ui = Ui::new(
        Element::leaf(Editor::new(text).padding(16., 8.).restore(state)),
        Size::new(300., 200.),
        Limits::default(),
    )
    .unwrap();
    settle(&mut ui, &mut TestText);
    assert_eq!(ui.root().state().scroll.y, 600.);
    ui.send(Edit::Wrap(false)).unwrap();
    settle(&mut ui, &mut TestText);
    assert_eq!(ui.root().state().scroll.y, 600.);
    ui.resize(Size::new(320., 240.));
    settle(&mut ui, &mut TestText);
    assert_eq!(ui.root().state().scroll.y, 600.);
    ui.dispatch(
        Input::Button {
            pointer: 0,
            button: 1,
            down: true,
            position: Point::new(317., 210.),
        },
        &mut TestText,
    );
    ui.dispatch(
        Input::Pointer {
            pointer: 0,
            position: Point::new(317., 235.),
        },
        &mut TestText,
    );
    ui.dispatch(
        Input::Button {
            pointer: 0,
            button: 1,
            down: false,
            position: Point::new(317., 235.),
        },
        &mut TestText,
    );
    settle(&mut ui, &mut TestText);
    assert!(ui.root().state().scroll.y > 3000.);
    assert_eq!(ui.root().state().caret.byte, 0);
}
#[test]
fn shift_click_double_click_and_triple_click_select_expected_text() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new("one two\nthree").padding(0., 0.)),
        Size::new(300., 200.),
        Limits::default(),
    )
    .unwrap();
    settle(&mut ui, &mut TestText);
    let p = ui.root().paragraph().unwrap().clone();
    let position = p.caret_point(Caret::at(5));
    for clicks in 1..=3 {
        ui.advance(std::time::Duration::from_millis(clicks * 100), 100);
        ui.dispatch(
            Input::Button {
                pointer: 0,
                button: 1,
                down: true,
                position,
            },
            &mut TestText,
        );
        ui.dispatch(
            Input::Button {
                pointer: 0,
                button: 1,
                down: false,
                position,
            },
            &mut TestText,
        );
        settle(&mut ui, &mut TestText);
        if clicks == 2 {
            assert_eq!(ui.root().selection(), Some(4..7));
        }
        if clicks == 3 {
            assert_eq!(ui.root().selection(), Some(0..8));
        }
    }
    ui.send(Edit::Select {
        anchor: 0,
        caret: 0,
    })
    .unwrap();
    settle(&mut ui, &mut TestText);
    ui.dispatch(
        Input::Modifiers(Modifiers {
            shift: true,
            ..Modifiers::default()
        }),
        &mut TestText,
    );
    ui.dispatch(
        Input::Button {
            pointer: 0,
            button: 1,
            down: true,
            position,
        },
        &mut TestText,
    );
    assert_eq!(ui.root().selection(), Some(0..5));
}
#[test]
fn list_navigation_and_hover_update_the_selected_row_environment() {
    let mut ui = Ui::new(
        Element::leaf(
            VirtualList::new(vec![0usize, 1, 2], 30., |k: &usize| {
                Element::leaf(Label::new(k.to_string()))
            })
            .select_on_hover(true),
        ),
        Size::new(200., 90.),
        Limits::default(),
    )
    .unwrap();
    settle(&mut ui, &mut TestText);
    ui.send(ListCommand::Navigate(1)).unwrap();
    ui.send(ListCommand::Navigate(1)).unwrap();
    settle(&mut ui, &mut TestText);
    ui.send(ListCommand::Activate).unwrap();
    let mut selected = None;
    ui.pump(
        100,
        |o| {
            let ListOutput::Selected(k) = o;
            selected = Some(k);
        },
        |_| {},
    );
    assert_eq!(selected, Some(1));
    ui.dispatch(
        Input::Pointer {
            pointer: 0,
            position: Point::new(10., 70.),
        },
        &mut TestText,
    );
    settle(&mut ui, &mut TestText);
    ui.send(ListCommand::Activate).unwrap();
    ui.pump(
        100,
        |o| {
            let ListOutput::Selected(k) = o;
            selected = Some(k);
        },
        |_| {},
    );
    assert_eq!(selected, Some(2));
}

#[test]
fn menu_keyboard_skips_disabled_actions_and_stays_inside_the_window() {
    let mut ui = Ui::new(
        Menu::new(
            vec![
                MenuItem::new(0usize, "Disabled").enabled(false),
                MenuItem::new(1, "Rename"),
                MenuItem::new(2, "Close"),
            ],
            Point::new(298., 190.),
        ),
        Size::new(300., 200.),
        Limits::default(),
    )
    .unwrap();
    settle(&mut ui, &mut TestText);
    ui.frame(std::time::Duration::ZERO, 100);
    settle(&mut ui, &mut TestText);
    for node in ui.semantics() {
        assert!(
            node.bounds.x >= 0.
                && node.bounds.y >= 0.
                && node.bounds.x + node.bounds.width <= 300.
                && node.bounds.y + node.bounds.height <= 200.
        );
    }
    key(&mut ui, &mut TestText, Key::Down);
    key(&mut ui, &mut TestText, Key::Enter);
    let mut selected = vec![];
    ui.pump(
        100,
        |o| {
            if let MenuOutput::Selected(k) = o {
                selected.push(k)
            }
        },
        |_| {},
    );
    assert_eq!(selected, vec![1]);
    key(&mut ui, &mut TestText, Key::Up);
    key(&mut ui, &mut TestText, Key::Enter);
    ui.pump(
        100,
        |o| {
            if let MenuOutput::Selected(k) = o {
                selected.push(k)
            }
        },
        |_| {},
    );
    assert_eq!(selected, vec![1, 2]);
    let disabled = ui
        .semantics()
        .into_iter()
        .find(|n| n.semantics.disabled)
        .unwrap()
        .id;
    ui.accessibility(disabled, SemanticAction::Activate);
    ui.pump(100, |_| panic!("Disabled menu action fired"), |_| {});
    key(&mut ui, &mut TestText, Key::Escape);
    let mut dismissed = false;
    ui.pump(
        100,
        |o| dismissed = matches!(o, MenuOutput::Dismissed),
        |_| {},
    );
    assert!(dismissed);
}
#[test]
fn menu_outside_click_dismisses_without_activating_an_action() {
    let mut ui = Ui::new(
        Menu::new(vec![MenuItem::new(1usize, "Save")], Point::new(20., 20.)),
        Size::new(400., 300.),
        Limits::default(),
    )
    .unwrap();
    settle(&mut ui, &mut TestText);
    ui.dispatch(
        Input::Button {
            pointer: 0,
            button: 1,
            down: true,
            position: Point::new(390., 290.),
        },
        &mut TestText,
    );
    let mut outputs = vec![];
    ui.pump(100, |o| outputs.push(o), |_| {});
    assert!(matches!(&outputs[..], [MenuOutput::Dismissed]));
}
