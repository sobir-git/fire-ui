use fire_ui::*;
use fire_ui_widgets::*;

fn settle(ui: &mut Ui<Editor>) {
    for _ in 0..20 {
        ui.pump(4096, |_| {}, |_| {});
        ui.layout(&mut TestText);
        if !ui.next_work().ready {
            return;
        }
    }
    panic!("did not settle");
}
fn replace(ui: &mut Ui<Editor>, start: usize, end: usize, text: &str) {
    ui.send(Edit::Select {
        anchor: start,
        caret: end,
    })
    .unwrap();
    settle(ui);
    ui.send(Edit::Insert(text.into())).unwrap();
    settle(ui);
}
#[test]
fn packed_history_keeps_removed_bytes_separate_from_normalized_inserted_text() {
    for field in [false, true] {
        let original = "前\ré👩‍👩‍👧‍👦\n後";
        let editor = if field {
            Editor::field(original)
        } else {
            Editor::new(original)
        };
        let mut ui = Ui::new(
            Element::leaf(editor),
            Size::new(500., 180.),
            Limits::default(),
        )
        .unwrap();
        settle(&mut ui);
        replace(
            &mut ui,
            "前".len(),
            original.len() - "後".len(),
            "א\r\n🙂e\u{301}",
        );
        let changed = if field {
            "前א 🙂e\u{301}後"
        } else {
            "前א\n🙂e\u{301}後"
        };
        assert_eq!(ui.root().text(), changed);
        for _ in 0..3 {
            ui.send(Edit::Undo).unwrap();
            settle(&mut ui);
            assert_eq!(ui.root().text().as_bytes(), original.as_bytes());
            ui.send(Edit::Redo).unwrap();
            settle(&mut ui);
            assert_eq!(ui.root().text(), changed);
        }
        // A removal has an empty inserted suffix; its inverse must use removed byte length.
        replace(&mut ui, 0, changed.len(), "");
        assert_eq!(ui.root().text(), "");
        ui.send(Edit::Undo).unwrap();
        settle(&mut ui);
        assert_eq!(ui.root().text(), changed);
        // A new insertion has an empty removed prefix and discards the redo branch.
        replace(&mut ui, changed.len(), changed.len(), "尾");
        ui.send(Edit::Redo).unwrap();
        settle(&mut ui);
        assert_eq!(ui.root().text(), format!("{changed}尾"));
    }
}
#[test]
fn packed_history_replays_hundreds_of_variable_length_unicode_replacements() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new("α")),
        Size::new(200., 100.),
        Limits::default(),
    )
    .unwrap();
    settle(&mut ui);
    let mut versions = vec!["α".to_owned()];
    for i in 0..400 {
        let text = format!("{i} {}", ["🙂", "e\u{301}", "שלום", "👩‍👩‍👧‍👦"][i % 4]);
        let end = ui.root().text().len();
        replace(&mut ui, 0, end, &text);
        versions.push(text);
    }
    for expected in versions[..400].iter().rev() {
        ui.send(Edit::Undo).unwrap();
        settle(&mut ui);
        assert_eq!(ui.root().text(), expected);
    }
    for expected in &versions[1..] {
        ui.send(Edit::Redo).unwrap();
        settle(&mut ui);
        assert_eq!(ui.root().text(), expected);
    }
}
