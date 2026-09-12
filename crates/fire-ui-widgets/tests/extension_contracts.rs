use fire_ui::*;
use fire_ui_widgets::*;
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
fn rtl_fragment_covers_the_cell() {
    let mut p = (*TestText.layout(TextRequest {
        previous: None,
        text: "א".into(),
        style: TextStyle::default(),
        width: None,
        revision: 0,
    }))
    .clone();
    p.lines[0].cells = vec![TextCell {
        end: 2,
        x: 10.,
        advance: -8.,
    }];
    p.lines[0].width = 18.;
    let fragments = p.range_fragments(0..2);
    assert_eq!(fragments.len(), 1, "RTL cell disappeared");
    assert_eq!(fragments[0].x, 10.0..18.0);
}

#[test]
fn rejected_extension_key_does_not_fall_through() {
    struct LongKey;
    impl EditorExtension for LongKey {
        fn key(&mut self, _: &EditorView<'_>, key: Key, _: Modifiers) -> Option<Transaction> {
            (key == Key::Enter).then(|| Transaction::replace(0..0, "XX"))
        }
    }
    let mut ui = Ui::new(
        Element::leaf(Editor::new("ab").max_bytes(3).extension(LongKey)),
        Size::new(300., 140.),
        Limits::default(),
    )
    .unwrap();
    let mut text = TestText;
    settle(&mut ui, &mut text);
    ui.send(Edit::Select {
        anchor: 2,
        caret: 1,
    })
    .unwrap();
    settle(&mut ui, &mut text);
    key(&mut ui, &mut text, Key::Enter);
    let mut limits = 0;
    ui.pump(
        4096,
        |output| {
            if matches!(output, EditorOutput::LimitReached) {
                limits += 1;
            }
        },
        |_| {},
    );
    assert_eq!(limits, 1);
    settle(&mut ui, &mut text);
    assert_eq!(
        ui.root().text(),
        "ab",
        "a rejected atomic operation must leave the document unchanged"
    );
    assert_eq!(ui.root().selection(), Some(1..2));
}

struct Owner {
    disabled: bool,
}
impl Widget for Owner {
    type Command = ();
    type Output = ();
    fn layout(&mut self, _: &mut Layout<'_>, c: Constraints) -> Metrics {
        Metrics {
            size: c.constrain(Size::new(100., 40.)),
            baseline: None,
        }
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            key: Some("owner".into()),
            disabled: self.disabled,
            children: vec![Semantics {
                key: Some("child".into()),
                role: Role::CheckBox,
                actions: vec![SemanticActionKind::Activate],
                ..Semantics::default()
            }],
            ..Semantics::default()
        }
    }
    fn accessibility(
        &mut self,
        _: &mut Update<'_, Self>,
        _: SemanticAction,
    ) -> Result<(), SemanticError> {
        Ok(())
    }
}

#[test]
fn child_default_bounds_match_translated_owner() {
    let mut ui = Ui::new(
        Padding::new(Element::leaf(Owner { disabled: false }), Insets::all(20.)),
        Size::new(300., 140.),
        Limits::default(),
    )
    .unwrap();
    settle(&mut ui, &mut TestText);
    let nodes = ui.semantics();
    let owner = nodes
        .iter()
        .find(|n| n.semantics.key.as_deref() == Some("owner"))
        .unwrap();
    let child = nodes
        .iter()
        .find(|n| n.semantics.key.as_deref() == Some("child"))
        .unwrap();
    assert_eq!(child.bounds, owner.bounds);
}

#[test]
fn disabled_owner_blocks_child_activation() {
    let mut ui = Ui::new(
        Element::leaf(Owner { disabled: true }),
        Size::new(300., 140.),
        Limits::default(),
    )
    .unwrap();
    settle(&mut ui, &mut TestText);
    let child = ui
        .semantics()
        .into_iter()
        .find(|n| n.semantics.role == Role::CheckBox)
        .unwrap();
    assert_eq!(
        ui.accessibility(child.id, SemanticAction::Activate),
        Err(SemanticError::Unavailable)
    );
}

#[test]
fn empty_fragment_matches_documented_contract() {
    let p = TestText.layout(TextRequest {
        previous: None,
        text: "abc".into(),
        style: TextStyle::default(),
        width: None,
        revision: 0,
    });
    assert_eq!(p.range_fragments(1..1).len(), 1);
    assert_eq!(p.range_fragments(1..1)[0].x, 9.0..9.0);
    assert_eq!(p.range_fragments(3..3)[0].x, 27.0..27.0);
    let empty = TestText.layout(TextRequest {
        previous: None,
        text: "".into(),
        style: TextStyle::default(),
        width: None,
        revision: 0,
    });
    assert_eq!(empty.range_fragments(0..0)[0].x, 0.0..0.0);
}
