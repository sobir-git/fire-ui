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
    ui.accessibility(id, SemanticAction::Focus).unwrap();
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
    let mut theme = Theme::default();
    theme.color.foreground = Color::hex(0xff0000);
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
    ui.accessibility(id, SemanticAction::Focus).unwrap();
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
    ui.accessibility(id, SemanticAction::Focus).unwrap();
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
    ui.accessibility(id, SemanticAction::Focus).unwrap();
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
    ui.accessibility(ui.semantics()[0].id, SemanticAction::Focus)
        .unwrap();
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
    assert_eq!(
        ui.accessibility(disabled, SemanticAction::Activate),
        Err(SemanticError::Unavailable)
    );
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

#[test]
fn button_content_and_accessible_name_update_without_remounting() {
    let mut ui = Ui::new(
        Button::new(Element::leaf(Label::new("Pause")), "Pause"),
        Size::new(180., 40.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    settle(&mut ui, &mut text);
    let ids: Vec<_> = ui.semantics().iter().map(|n| n.id).collect();
    ui.send(ButtonCommand::Content("Resume".to_owned()))
        .unwrap();
    ui.send(ButtonCommand::Label("Resume".to_owned())).unwrap();
    settle(&mut ui, &mut text);
    let nodes = ui.semantics();
    assert_eq!(ids, nodes.iter().map(|n| n.id).collect::<Vec<_>>());
    assert!(nodes
        .iter()
        .any(|n| n.semantics.role == Role::Button && n.semantics.label == "Resume"));
    assert!(nodes
        .iter()
        .any(|n| n.semantics.role == Role::Text && n.semantics.label == "Resume"));
}

#[test]
fn semantic_edits_preserve_direction_validate_graphemes_and_use_editor_history() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new("aé e\u{301}!").label("Body").key("body")),
        Size::new(400., 200.),
        Limits::default(),
    )
    .unwrap();
    ui.pump(100, |_| {}, |_| {});
    ui.layout(&mut TestText);
    let id = ui.semantics()[0].id;
    assert_eq!(
        ui.accessibility(
            id,
            SemanticAction::SetSelection {
                anchor: 0,
                caret: 2
            }
        ),
        Err(SemanticError::InvalidSelection)
    );
    ui.accessibility(
        id,
        SemanticAction::SetSelection {
            anchor: 3,
            caret: 1,
        },
    )
    .unwrap();
    let text = ui.semantics()[0].semantics.text.clone().unwrap();
    assert_eq!((text.anchor, text.caret.byte), (3, 1));
    ui.accessibility(id, SemanticAction::ReplaceSelectedText("שלום".into()))
        .unwrap();
    assert_eq!(
        ui.semantics()[0].semantics.value.as_deref(),
        Some("aשלום e\u{301}!")
    );
    ui.send(Edit::Undo).unwrap();
    ui.pump(100, |_| {}, |_| {});
    assert_eq!(
        ui.semantics()[0].semantics.value.as_deref(),
        Some("aé e\u{301}!")
    );
    assert_eq!(
        ui.accessibility(u64::MAX, SemanticAction::Focus),
        Err(SemanticError::Unavailable)
    );
    assert_eq!(
        ui.accessibility(id, SemanticAction::Activate),
        Err(SemanticError::Unsupported)
    );
}

#[test]
fn ime_selection_is_exposed_and_cancelled_without_changing_document() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new("original")),
        Size::new(400., 200.),
        Limits::default(),
    )
    .unwrap();
    ui.pump(100, |_| {}, |_| {});
    ui.layout(&mut TestText);
    let id = ui.semantics()[0].id;
    ui.accessibility(id, SemanticAction::Focus).unwrap();
    let session = ui.session();
    ui.dispatch(
        Input::Preedit {
            session,
            text: "日本".into(),
            selection: Some((0, 3)),
        },
        &mut TestText,
    );
    let node = &ui.semantics()[0];
    assert_eq!(node.semantics.value.as_deref(), Some("original"));
    assert_eq!(
        node.semantics
            .text
            .as_ref()
            .unwrap()
            .composition
            .as_ref()
            .unwrap()
            .selection,
        Some((0, 3))
    );
    ui.dispatch(
        Input::Preedit {
            session,
            text: String::new(),
            selection: None,
        },
        &mut TestText,
    );
    assert!(ui.semantics()[0]
        .semantics
        .text
        .as_ref()
        .unwrap()
        .composition
        .is_none());
}

#[test]
fn semantic_range_edits_and_set_value_are_atomic_undoable_and_report_limits() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new("aé e\u{301}!").max_bytes(32)),
        Size::new(400., 200.),
        Limits::default(),
    )
    .unwrap();
    settle(&mut ui, &mut TestText);
    let id = ui.semantics()[0].id;
    let before = ui.semantics()[0].semantics.clone();
    for action in [
        SemanticAction::ReplaceText {
            start: 2,
            end: 3,
            text: "x".into(),
        },
        SemanticAction::ReplaceText {
            start: 3,
            end: 1,
            text: "x".into(),
        },
    ] {
        assert_eq!(
            ui.accessibility(id, action),
            Err(SemanticError::InvalidSelection)
        );
    }
    assert_eq!(
        ui.accessibility(id, SemanticAction::SetValue("x".repeat(33))),
        Err(SemanticError::LimitReached)
    );
    let unchanged = ui.semantics()[0].semantics.clone();
    assert_eq!(unchanged.value, before.value);
    assert_eq!(unchanged.text.unwrap().caret, before.text.unwrap().caret);
    // AT-SPI offsets count Unicode scalars. Deleting a combining mark is legal,
    // while keyboard caret movement continues to use complete graphemes.
    ui.accessibility(
        id,
        SemanticAction::ReplaceText {
            start: 5,
            end: 7,
            text: String::new(),
        },
    )
    .unwrap();
    assert_eq!(ui.semantics()[0].semantics.value.as_deref(), Some("aé e!"));
    ui.accessibility(id, SemanticAction::SetValue("שלום".into()))
        .unwrap();
    assert_eq!(ui.semantics()[0].semantics.value.as_deref(), Some("שלום"));
    for expected in ["aé e!", "aé e\u{301}!"] {
        ui.send(Edit::Undo).unwrap();
        settle(&mut ui, &mut TestText);
        assert_eq!(ui.semantics()[0].semantics.value.as_deref(), Some(expected));
    }
    // Inserting a base before a combining mark must not leave a caret inside it.
    ui.accessibility(
        id,
        SemanticAction::ReplaceText {
            start: 5,
            end: 5,
            text: "x".into(),
        },
    )
    .unwrap();
    let s = ui.semantics()[0].semantics.clone();
    assert_eq!(s.value.as_deref(), Some("aé ex\u{301}!"));
    assert_eq!(s.text.unwrap().caret.byte, 5);
    assert_eq!(
        ui.accessibility(
            id,
            SemanticAction::SetSelection {
                anchor: 6,
                caret: 6
            }
        ),
        Err(SemanticError::InvalidSelection)
    );
    // Consecutive semantic operations are valid before the next layout pass.
    ui.accessibility(id, SemanticAction::SetValue("a longer value".into()))
        .unwrap();
    ui.accessibility(
        id,
        SemanticAction::SetSelection {
            anchor: 14,
            caret: 14,
        },
    )
    .unwrap();
}

#[test]
fn external_edits_invalidate_old_ime_input_only_after_success() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new("old").max_bytes(16)),
        Size::new(400., 200.),
        Limits::default(),
    )
    .unwrap();
    settle(&mut ui, &mut TestText);
    let id = ui.semantics()[0].id;
    ui.accessibility(id, SemanticAction::Focus).unwrap();
    let session = ui.session();
    ui.dispatch(
        Input::Preedit {
            session,
            text: "日".into(),
            selection: Some((3, 3)),
        },
        &mut TestText,
    );
    assert_eq!(
        ui.accessibility(id, SemanticAction::SetValue("x".repeat(17))),
        Err(SemanticError::LimitReached)
    );
    assert_eq!(ui.session(), session);
    assert!(ui.semantics()[0]
        .semantics
        .text
        .as_ref()
        .unwrap()
        .composition
        .is_some());
    ui.accessibility(id, SemanticAction::SetValue("new".into()))
        .unwrap();
    assert_ne!(ui.session(), session);
    ui.dispatch(
        Input::Text {
            session,
            text: "日".into(),
        },
        &mut TestText,
    );
    assert_eq!(ui.semantics()[0].semantics.value.as_deref(), Some("new"));
    assert!(ui.semantics()[0]
        .semantics
        .text
        .as_ref()
        .unwrap()
        .composition
        .is_none());
    ui.dispatch(
        Input::Text {
            session: ui.session(),
            text: "!".into(),
        },
        &mut TestText,
    );
    assert_eq!(ui.semantics()[0].semantics.value.as_deref(), Some("new!"));
}

#[test]
fn clipboard_requires_host_opt_in_before_cut_can_mutate_text() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new("aé🔥z")),
        Size::new(300., 140.),
        Limits::default(),
    )
    .unwrap();
    settle(&mut ui, &mut TestText);
    let id = ui.semantics()[0].id;
    ui.accessibility(id, SemanticAction::Focus).unwrap();
    ui.accessibility(
        id,
        SemanticAction::SetSelection {
            anchor: 7,
            caret: 1,
        },
    )
    .unwrap();
    let selection = || Input::Key {
        key: Key::Character('x'),
        physical: 1,
        down: true,
        repeat: false,
        modifiers: Modifiers {
            control: true,
            ..Modifiers::default()
        },
    };
    for ch in ['x', 'c', 'v'] {
        let mut event = selection();
        if let Input::Key { key, .. } = &mut event {
            *key = Key::Character(ch);
        }
        ui.dispatch(event, &mut TestText);
        ui.pump(
            100,
            |_| {},
            |_| panic!("disabled clipboard queued a request"),
        );
        assert_eq!(ui.root().text(), "aé🔥z");
        let text = ui.semantics()[0].semantics.text.clone().unwrap();
        assert_eq!((text.anchor, text.caret.byte), (7, 1));
    }

    ui.set_clipboard_enabled(true);
    ui.dispatch(selection(), &mut TestText);
    let mut copied = vec![];
    ui.pump(
        100,
        |_| {},
        |request| match request {
            HostRequest::Copy(value) => copied.push(value),
            _ => panic!("cut queued an unexpected request"),
        },
    );
    assert_eq!(copied, ["é🔥"]);
    assert_eq!(ui.root().text(), "az");
    ui.send(Edit::Undo).unwrap();
    settle(&mut ui, &mut TestText);
    assert_eq!(ui.root().text(), "aé🔥z");
}

fn click<W: Widget>(ui: &mut Ui<W>, text: &mut dyn TextEngine, at: Point) {
    for down in [true, false] {
        ui.dispatch(
            Input::Button {
                pointer: 0,
                button: 1,
                down,
                position: at,
            },
            text,
        );
    }
}

#[test]
fn tabs_report_the_tab_that_was_clicked() {
    let mut ui = Ui::new(
        Tabs::new(["One", "Two", "Three"]),
        Size::new(400., 60.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    settle(&mut ui, &mut text);
    let bounds: Vec<Rect> = ui
        .semantics()
        .into_iter()
        .filter(|n| n.semantics.role == Role::Tab)
        .map(|n| n.bounds)
        .collect();
    assert_eq!(bounds.len(), 3, "three tabs are published");
    let target = bounds[2];
    let mut chosen = vec![];
    click(
        &mut ui,
        &mut text,
        Point::new(target.x + target.width / 2., target.y + target.height / 2.),
    );
    ui.pump(100, |o| chosen.push(o), |_| {});
    assert_eq!(chosen, vec![2], "clicking the third tab selects it");
}

fn point<W: Widget>(ui: &mut Ui<W>, text: &mut dyn TextEngine, at: Point) {
    ui.dispatch(
        Input::Pointer {
            pointer: 0,
            position: at,
        },
        text,
    );
}
fn center(r: Rect) -> Point {
    Point::new(r.x + r.width / 2., r.y + r.height / 2.)
}
fn node<W: Widget>(ui: &Ui<W>, role: Role, label: &str) -> Option<SemanticNode> {
    ui.semantics()
        .into_iter()
        .find(|n| n.semantics.role == role && n.semantics.label == label)
}

#[test]
fn dropdown_opens_a_list_below_itself_and_reports_the_chosen_option() {
    let mut ui = Ui::new(
        Dropdown::new("Palette", ["Ember", "Charcoal", "Paper"]),
        Size::new(320., 400.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    settle(&mut ui, &mut text);
    let field = node(&ui, Role::Menu, "Palette").expect("the field publishes itself");
    assert_eq!(field.semantics.value.as_deref(), Some("Ember"));
    assert!(
        node(&ui, Role::MenuItem, "Paper").is_none(),
        "closed to begin with"
    );

    click(&mut ui, &mut text, center(field.bounds));
    settle(&mut ui, &mut text);
    let row = node(&ui, Role::MenuItem, "Paper").expect("the list opened");
    assert!(
        row.bounds.y > field.bounds.y,
        "the list opens below its field, not at the window origin: {:?}",
        row.bounds
    );

    point(&mut ui, &mut text, center(row.bounds));
    click(&mut ui, &mut text, center(row.bounds));
    let mut chosen = vec![];
    ui.pump(100, |o| chosen.push(o), |_| {});
    assert_eq!(chosen, vec![2], "the chosen index is reported");
    settle(&mut ui, &mut text);
    assert!(
        node(&ui, Role::MenuItem, "Paper").is_none(),
        "the list closed"
    );
    assert_eq!(
        node(&ui, Role::Menu, "Palette")
            .unwrap()
            .semantics
            .value
            .as_deref(),
        Some("Paper"),
    );
}

#[test]
fn a_dropdown_inside_a_scrolling_panel_still_opens_over_it() {
    // The studio's shape: the field is deep inside a clipping viewport, so the list
    // is only usable if it escapes that clip as a window overlay.
    let mut ui = Ui::new(
        Scroll::new(Padding::new(
            Dropdown::new("Palette", ["Ember", "Charcoal", "Paper"]),
            Insets::all(28.),
        )),
        Size::new(420., 300.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    settle(&mut ui, &mut text);
    let field = node(&ui, Role::Menu, "Palette").expect("the field publishes itself");
    click(&mut ui, &mut text, center(field.bounds));
    settle(&mut ui, &mut text);
    let row = node(&ui, Role::MenuItem, "Paper").expect("the list opened");
    assert!(
        row.bounds.height > 0. && row.bounds.width > 0.,
        "the list was clipped away by the panel: {:?}",
        row.bounds
    );
    assert!(
        row.bounds.y > field.bounds.y,
        "the list opens below its field: {:?} vs {:?}",
        row.bounds,
        field.bounds
    );
    point(&mut ui, &mut text, center(row.bounds));
    click(&mut ui, &mut text, center(row.bounds));
    let mut chosen = vec![];
    ui.pump(100, |o| chosen.push(o), |_| {});
    assert_eq!(chosen, vec![2], "the chosen index reaches the owner");
}

#[derive(Default)]
struct Recorder {
    /// Solid colours text was drawn in.
    fills: Vec<Color>,
    /// Every filled rectangle, with the brush it was filled by.
    rects: Vec<(Rect, Brush)>,
    /// Every stroked rectangle and its colour.
    strokes: Vec<(Rect, Color)>,
}
impl Painter for Recorder {
    fn save(&mut self) {}
    fn restore(&mut self) {}
    fn transform(&mut self, _: Transform) {}
    fn clip(&mut self, _: Rect) {}
    fn rect(&mut self, rect: Rect, _: f32, brush: Brush) {
        self.rects.push((rect, brush))
    }
    fn stroke(&mut self, rect: Rect, _: f32, _: f32, color: Color) {
        self.strokes.push((rect, color))
    }
    fn path(&mut self, _: &[Path], _: Brush, _: Option<f32>) {}
    fn paragraph(&mut self, _: &Paragraph, _: Point, brush: Brush) {
        if let Brush::Solid(c) = brush {
            self.fills.push(c)
        }
    }
}

#[test]
fn swapping_the_theme_reaches_content_a_control_styles_for_itself() {
    // A filled button publishes a derived theme to its label so the text resolves
    // to `on_accent`. That override must not freeze the label on the old palette.
    let mut ui = Ui::new(
        Button::styled(
            Element::leaf(Label::new("Save")),
            "Save",
            ButtonStyle::Primary,
        ),
        Size::new(200., 80.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    ui.set_environment(Rc::new(Theme::dark()), true);
    settle(&mut ui, &mut text);
    let mut painter = Recorder::default();
    ui.paint(&mut painter);
    assert_eq!(
        painter.fills,
        vec![Theme::dark().color.on_accent],
        "the label draws in the dark palette's on-accent colour"
    );

    ui.set_environment(Rc::new(Theme::light()), true);
    settle(&mut ui, &mut text);
    let mut painter = Recorder::default();
    ui.paint(&mut painter);
    assert_eq!(
        painter.fills,
        vec![Theme::light().color.on_accent],
        "swapping the palette re-derives the colour the button gave its label"
    );
}

#[test]
fn a_focus_ring_is_a_ring_and_does_not_wash_the_control_it_marks() {
    // `glow` fills its interior as well as its edge. Drawing one after a control
    // has painted itself tints the whole control, which is what a focused dropdown
    // used to look like.
    let mut ui = Ui::new(
        Dropdown::new("Palette", ["Ember", "Charcoal"]),
        Size::new(320., 200.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    ui.set_environment(Rc::new(Theme::dark()), true);
    settle(&mut ui, &mut text);

    let field = node(&ui, Role::Menu, "Palette").expect("the field publishes itself");
    let mut before = Recorder::default();
    ui.paint(&mut before);
    key(&mut ui, &mut text, Key::Tab);
    settle(&mut ui, &mut text);
    assert!(
        node(&ui, Role::Menu, "Palette").unwrap().focused,
        "tab moved focus to the field"
    );
    let mut after = Recorder::default();
    ui.paint(&mut after);

    let focus = Theme::dark().color.focus;
    assert!(
        after.strokes.iter().any(|(_, c)| *c == focus),
        "focus is marked by a stroked ring"
    );
    let washed = |r: &Recorder| {
        r.rects.iter().any(|(rect, brush)| {
            rect.width >= field.bounds.width
                && matches!(brush, Brush::Box { from, .. } if from.0 == focus.0 && from.1 == focus.1)
        })
    };
    assert!(
        !washed(&before),
        "nothing washes the field before it is focused"
    );
    assert!(
        !washed(&after),
        "focusing must not fill the field with the focus colour"
    );
}

/// A heading, a caption and a panel: the shape of every studio section.
struct Section {
    #[allow(dead_code)]
    heading: Child<Label>,
    caption: Child<Label>,
    panel: Child<Surface<Greedy>>,
}

/// Takes every pixel it is offered, like the editor does.
struct Greedy;
impl Widget for Greedy {
    type Command = std::convert::Infallible;
    type Output = std::convert::Infallible;
    fn layout(&mut self, _: &mut Layout<'_>, c: Constraints) -> Metrics {
        Metrics::new(c.constrain(c.max))
    }
}
impl Section {
    fn new() -> Element<Self> {
        Element::build(|c| Self {
            heading: c.add(Element::leaf(Label::styled("Editing", TextRole::Heading))),
            caption: c.add(Element::leaf(Label::styled(
                "A supporting line",
                TextRole::Small,
            ))),
            // An editor fills the height it is offered, which is what turns a
            // measurement against the wrong extent into a visible overflow.
            panel: c.discard(surface(SurfaceStyle::Sunken).wrap(Element::leaf(Greedy))),
        })
    }
}
impl Widget for Section {
    type Command = std::convert::Infallible;
    type Output = std::convert::Infallible;
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        column(
            cx,
            c,
            Flow::gap(8.).align(Align::Stretch),
            &[
                Entry::natural(self.heading),
                Entry::natural(self.caption),
                Entry::natural(self.panel).aligned(Align::Stretch),
            ],
        )
    }
}

#[test]
fn a_column_never_measures_a_child_past_the_room_that_is_left() {
    // The panel must fit under the heading and caption. Measuring every natural
    // child against the full height instead of the remainder pushed the panel out
    // of the bottom, where an ancestor clipped its border away.
    let height = 160.;
    let mut ui = Ui::new(Section::new(), Size::new(400., height), Limits::default()).unwrap();
    let mut text = TestText;
    settle(&mut ui, &mut text);
    // Published semantic bounds are already intersected with the clip, so an
    // overflow is invisible there. The unclipped geometry is what shows it.
    let panel = ui.root().panel;
    let bounds = ui.geometry(panel).expect("the panel is placed").bounds;
    assert!(
        bounds.y + bounds.height <= height + 0.5,
        "the panel overflows the column and its bottom edge is clipped away: \
         {bounds:?} in a column {height} tall"
    );
    assert!(bounds.height > 0., "and it is still given real room");
}

#[test]
fn a_scrollbar_takes_its_own_strip_and_the_content_below_it_does_not_claim_the_pointer() {
    // The editor asks for a text caret. If the bar floats over it, the pointer
    // resolves through the editor and shows a caret above the scrollbar.
    let mut ui = Ui::new(
        Scroll::new(Element::leaf(Editor::new(
            "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\neleven\ntwelve",
        ))),
        Size::new(240., 90.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    ui.set_environment(Rc::new(Theme::dark()), true);
    settle(&mut ui, &mut text);

    let bar = Scrollbar::width(&Theme::dark());
    let over_text = Point::new(20., 40.);
    let over_bar = Point::new(240. - bar / 2., 40.);
    point(&mut ui, &mut text, over_text);
    assert_eq!(
        ui.cursor(over_text),
        CursorIcon::Text,
        "text asks for a caret"
    );
    point(&mut ui, &mut text, over_bar);
    assert_eq!(
        ui.cursor(over_bar),
        CursorIcon::Arrow,
        "the scrollbar strip is not part of the content"
    );
}

#[test]
fn a_list_scrollbar_can_be_dragged() {
    let mut ui = Ui::new(
        Element::leaf(VirtualList::new(
            (0..500usize).collect(),
            24.,
            |key: &usize| Element::leaf(Label::new(key.to_string())),
        )),
        Size::new(300., 240.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    ui.set_environment(Rc::new(Theme::dark()), true);
    settle(&mut ui, &mut text);
    assert_eq!(ui.root().offset(), 0., "starts at the top");

    // Take hold of the thumb and drag it most of the way down the track.
    let bar = Scrollbar::width(&Theme::dark());
    let x = 300. - bar / 2.;
    ui.dispatch(
        Input::Button {
            pointer: 0,
            button: 1,
            down: true,
            position: Point::new(x, 20.),
        },
        &mut text,
    );
    settle(&mut ui, &mut text);
    ui.dispatch(
        Input::Pointer {
            pointer: 0,
            position: Point::new(x, 200.),
        },
        &mut text,
    );
    settle(&mut ui, &mut text);
    let dragged = ui.root().offset();
    assert!(
        dragged > 0.,
        "dragging the thumb scrolls the list, but the offset stayed at {dragged}"
    );
    ui.dispatch(
        Input::Button {
            pointer: 0,
            button: 1,
            down: false,
            position: Point::new(x, 200.),
        },
        &mut text,
    );
    settle(&mut ui, &mut text);
    assert_eq!(ui.root().offset(), dragged, "releasing keeps the position");
}

#[test]
fn an_editors_own_scrollbar_matches_the_others_and_is_not_part_of_the_text() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new(
            "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\neleven\ntwelve",
        )),
        Size::new(240., 90.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    ui.set_environment(Rc::new(Theme::dark()), true);
    settle(&mut ui, &mut text);

    let bar = Scrollbar::width(&Theme::dark());
    let over_text = Point::new(20., 40.);
    let over_bar = Point::new(240. - bar / 2., 40.);
    point(&mut ui, &mut text, over_text);
    assert_eq!(ui.cursor(over_text), CursorIcon::Text);
    point(&mut ui, &mut text, over_bar);
    assert_eq!(
        ui.cursor(over_bar),
        CursorIcon::Arrow,
        "the editor's own scrollbar is not text"
    );
}

#[test]
fn a_list_scrollbar_is_not_a_row_and_answers_the_pointer() {
    let mut ui = Ui::new(
        Element::leaf(VirtualList::new(
            (0..500usize).collect(),
            24.,
            |key: &usize| Element::leaf(Label::new(key.to_string())),
        )),
        Size::new(300., 240.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    ui.set_environment(Rc::new(Theme::dark()), true);
    settle(&mut ui, &mut text);

    let bar = Scrollbar::width(&Theme::dark());
    let over_rows = Point::new(40., 100.);
    let over_bar = Point::new(300. - bar / 2., 100.);
    point(&mut ui, &mut text, over_rows);
    assert_eq!(
        ui.cursor(over_rows),
        CursorIcon::Pointer,
        "rows are choices, so the pointer is a hand"
    );
    point(&mut ui, &mut text, over_bar);
    assert_eq!(
        ui.cursor(over_bar),
        CursorIcon::Arrow,
        "the scrollbar is not a row"
    );

    // Hovering the bar has to change how it draws, or there is no feedback at all.
    let paint = |ui: &mut Ui<VirtualList<usize, Label, _>>| {
        let mut r = Recorder::default();
        ui.paint(&mut r);
        r.rects
            .into_iter()
            .map(|(rect, _)| rect)
            .collect::<Vec<_>>()
    };
    point(&mut ui, &mut text, over_rows);
    settle(&mut ui, &mut text);
    let resting = paint(&mut ui);
    point(&mut ui, &mut text, over_bar);
    settle(&mut ui, &mut text);
    let hovered = paint(&mut ui);
    assert_ne!(
        resting, hovered,
        "the scrollbar looks the same hovered as at rest"
    );
}

#[test]
fn moving_the_pointer_over_an_editor_does_not_drag_the_view_back_to_the_caret() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new(
            "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\neleven\ntwelve",
        )),
        Size::new(240., 90.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    ui.set_environment(Rc::new(Theme::dark()), true);
    settle(&mut ui, &mut text);

    // Scroll away from the caret, which is at the very start.
    ui.dispatch(
        Input::Scroll {
            position: Point::new(40., 40.),
            delta: Point::new(0., -60.),
        },
        &mut text,
    );
    settle(&mut ui, &mut text);
    let scrolled = ui.root().scroll_offset();
    assert!(scrolled > 0., "the wheel scrolled the editor");

    point(&mut ui, &mut text, Point::new(60., 50.));
    settle(&mut ui, &mut text);
    assert_eq!(
        ui.root().scroll_offset(),
        scrolled,
        "moving the pointer must not scroll the view back to the caret"
    );
}

#[test]
fn a_scale_change_relays_the_window_while_a_palette_change_only_repaints() {
    // Palette and scale are independent axes. Swapping colours must not cost a
    // layout; swapping metrics must, or controls keep their old geometry.
    assert!(
        !Theme::light().metrics_changed(&Theme::dark()),
        "the two ember themes share a scale"
    );
    assert!(
        Theme::compact().metrics_changed(&Theme::dark()),
        "the compact theme changes metrics"
    );

    // Centred, so the button takes its natural size rather than the window's.
    let mut ui = Ui::new(
        Aligned::center(Button::styled(
            Element::leaf(Label::new("Save")),
            "Save",
            ButtonStyle::Primary,
        )),
        Size::new(300., 120.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    let button = |ui: &Ui<Aligned<Button<Label>>>| {
        ui.semantics()
            .into_iter()
            .find(|n| n.semantics.role == Role::Button)
            .expect("the button publishes itself")
            .bounds
    };
    ui.set_environment(Rc::new(Theme::dark()), true);
    settle(&mut ui, &mut text);
    let spacious = button(&ui);

    ui.set_environment(Rc::new(Theme::light()), false);
    settle(&mut ui, &mut text);
    assert_eq!(
        button(&ui),
        spacious,
        "a palette swap leaves geometry alone"
    );

    ui.set_environment(Rc::new(Theme::compact()), true);
    settle(&mut ui, &mut text);
    let compact = button(&ui);
    assert!(
        compact.height < spacious.height,
        "the compact scale gives a shorter control: {compact:?} against {spacious:?}"
    );

    // Any palette pairs with any scale; the shipped pairs are only conveniences.
    let mixed = Theme {
        color: Palette::ember_dark(),
        scale: Scale::compact(),
    };
    assert!(mixed.metrics_changed(&Theme::dark()));
    assert_eq!(mixed.color.accent, Theme::dark().color.accent);
}

#[test]
fn theme_is_a_small_value() {
    assert_eq!(std::mem::size_of::<Palette>(), 17 * 16);
    assert!(
        std::mem::size_of::<Theme>() <= 320,
        "{}",
        std::mem::size_of::<Theme>()
    );
}

/// A widget that knows nothing about themes: it draws with the core's `Color` and
/// `Painter` alone. This is what any consumer can write, and what `fire-ui` on its
/// own supports — the core has no theme concept at all.
struct Unthemed;
impl Widget for Unthemed {
    type Command = std::convert::Infallible;
    type Output = std::convert::Infallible;
    fn layout(&mut self, _: &mut Layout<'_>, c: Constraints) -> Metrics {
        Metrics::new(c.constrain(Size::new(40., 20.)))
    }
    fn paint(&self, cx: &mut Paint<'_>) {
        cx.painter.rect(cx.bounds, 0., Color::hex(0x00ff00).into())
    }
}

#[test]
fn widgets_work_with_no_theme_installed_and_with_an_invisible_one() {
    // 1. A custom widget can ignore themes entirely.
    let mut ui = Ui::new(
        Element::leaf(Unthemed),
        Size::new(80., 40.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    settle(&mut ui, &mut text);
    let mut painter = Recorder::default();
    ui.paint(&mut painter);
    assert_eq!(painter.rects.len(), 1, "it drew, with no theme in the tree");

    // 2. A shipped control with no theme installed falls back to the default rather
    //    than failing, so a consumer can use one without opting into theming.
    let mut ui = Ui::new(
        Button::styled(Element::leaf(Label::new("Go")), "Go", ButtonStyle::Primary),
        Size::new(200., 80.),
        Limits::default(),
    )
    .unwrap();
    settle(&mut ui, &mut text);
    let mut bare = Recorder::default();
    ui.paint(&mut bare);
    assert!(!bare.rects.is_empty(), "an unthemed control still draws");

    // 3. A theme whose surfaces are transparent is expressible, which is how a
    //    consumer asks a control for behaviour without chrome.
    let invisible = Theme {
        color: Palette {
            accent: Color::hex(0x000000).alpha(0.),
            accent_light: Color::hex(0x000000).alpha(0.),
            border: Color::hex(0x000000).alpha(0.),
            ..Palette::ember_dark()
        },
        scale: Scale::default(),
    };
    ui.set_environment(Rc::new(invisible), false);
    settle(&mut ui, &mut text);
    let mut clear = Recorder::default();
    ui.paint(&mut clear);
    let visible = |brush: &Brush| match brush {
        Brush::Solid(c) => c.3 > 0.,
        Brush::Linear { from, to, .. } => from.3 > 0. || to.3 > 0.,
        Brush::Radial { from, to, .. } => from.3 > 0. || to.3 > 0.,
        Brush::Box { from, to, .. } => from.3 > 0. || to.3 > 0.,
    };
    let opaque = |r: &Recorder| r.rects.iter().filter(|(_, b)| visible(b)).count();
    assert!(
        opaque(&clear) < opaque(&bare),
        "a transparent palette removes painted chrome without removing the control"
    );
    assert!(
        ui.semantics()
            .iter()
            .any(|n| n.semantics.role == Role::Button),
        "and the control is still a button to assistive technology"
    );
}

#[test]
fn a_row_height_rule_follows_the_theme_while_a_number_does_not() {
    let rows = |height: RowHeight| {
        VirtualList::new((0..200usize).collect(), height, |key: &usize| {
            Element::leaf(Label::new(key.to_string()))
        })
    };
    for (rule, follows) in [
        (RowHeight::Scaled(|scale| scale.space(4.)), true),
        (RowHeight::Fixed(32.), false),
    ] {
        let mut ui = Ui::new(
            Element::leaf(rows(rule)),
            Size::new(200., 200.),
            Limits::default(),
        )
        .unwrap();
        let mut text = TestText;
        ui.set_environment(Rc::new(Theme::dark()), true);
        settle(&mut ui, &mut text);
        let spacious = ui.root().mounted_rows();
        ui.set_environment(Rc::new(Theme::compact()), true);
        settle(&mut ui, &mut text);
        let compact = ui.root().mounted_rows();
        assert_eq!(
            compact > spacious,
            follows,
            "shorter rows should mean more of them mounted: {spacious} then {compact}"
        );
    }
}
