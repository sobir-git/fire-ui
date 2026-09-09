use fire_ui::*;
use fire_ui_widgets::*;
use std::{cell::Cell, ops::Range, rc::Rc, sync::Arc};

struct CustomDocument {
    text: String,
    revision: u64,
    reads: Rc<Cell<usize>>,
}
impl Document for CustomDocument {
    fn text(&self) -> &str {
        self.reads.set(self.reads.get() + 1);
        &self.text
    }
    fn revision(&self) -> u64 {
        self.revision
    }
    fn replace(&mut self, range: Range<usize>, text: &str) {
        self.text.replace_range(range, text);
        self.revision += 7;
    }
}
fn settle<D: Document>(ui: &mut Ui<Editor<D>>, changed: &mut Vec<Arc<str>>) {
    for _ in 0..20 {
        ui.pump(
            4096,
            |output| {
                if let EditorOutput::Changed { text, .. } = output {
                    changed.push(text);
                }
            },
            |_| {},
        );
        ui.layout(&mut TestText);
        if !ui.next_work().ready {
            return;
        }
    }
    panic!("did not settle");
}
#[test]
fn custom_document_snapshot_is_lazy_and_shared_with_output_and_reflow() {
    let reads = Rc::new(Cell::new(0));
    let editor = Editor::with_document(CustomDocument {
        text: "שלום e\u{301}\n".repeat(30),
        revision: 41,
        reads: reads.clone(),
    });
    assert_eq!(reads.get(), 0, "construction must not copy document data");
    let mut ui = Ui::new(
        Element::leaf(editor),
        Size::new(300., 100.),
        Limits::default(),
    )
    .unwrap();
    let mut changed = vec![];
    settle(&mut ui, &mut changed);
    let first = ui.root().paragraph().unwrap().text.clone();
    ui.send(Edit::Insert("é🙂".into())).unwrap();
    settle(&mut ui, &mut changed);
    assert_eq!(changed.len(), 1);
    let snapshot = changed[0].clone();
    assert!(Arc::ptr_eq(&snapshot, &ui.root().paragraph().unwrap().text));
    assert!(!Arc::ptr_eq(&first, &snapshot));
    assert!(!first.starts_with("é🙂"));
    ui.resize(Size::new(150., 60.));
    settle(&mut ui, &mut changed);
    assert!(Arc::ptr_eq(&snapshot, &ui.root().paragraph().unwrap().text));
    ui.send(Edit::Undo).unwrap();
    settle(&mut ui, &mut changed);
    assert_eq!(ui.root().text(), first.as_ref());
    assert!(Arc::ptr_eq(
        changed.last().unwrap(),
        &ui.root().paragraph().unwrap().text
    ));
    ui.send(Edit::Redo).unwrap();
    settle(&mut ui, &mut changed);
    assert_eq!(ui.root().text(), snapshot.as_ref());
    assert!(Arc::ptr_eq(
        changed.last().unwrap(),
        &ui.root().paragraph().unwrap().text
    ));
}
#[test]
fn hidden_editor_drops_cached_text_even_after_programmatic_edits() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new("old")),
        Size::new(300., 100.),
        Limits::default(),
    )
    .unwrap();
    let mut changed = vec![];
    settle(&mut ui, &mut changed);
    let initial = Arc::downgrade(&ui.root().paragraph().unwrap().text);
    ui.window_visible(false);
    settle(&mut ui, &mut changed);
    assert!(initial.upgrade().is_none());
    ui.send(Edit::Insert("hidden ".into())).unwrap();
    settle(&mut ui, &mut changed);
    assert_eq!(changed.len(), 1);
    let hidden = Arc::downgrade(&changed[0]);
    changed.clear();
    assert!(
        hidden.upgrade().is_none(),
        "hidden edits should leave ownership with output consumers only"
    );
    ui.window_visible(true);
    settle(&mut ui, &mut changed);
    assert_eq!(ui.root().paragraph().unwrap().text.as_ref(), "hidden old");
}
#[test]
fn silent_set_invalidates_snapshot_without_emitting_changed() {
    let mut ui = Ui::new(
        Element::leaf(Editor::new("old")),
        Size::new(300., 100.),
        Limits::default(),
    )
    .unwrap();
    let mut changed = vec![];
    settle(&mut ui, &mut changed);
    let old = ui.root().paragraph().unwrap().text.clone();
    ui.send(Edit::Set("new 👩‍👩‍👧‍👦".into())).unwrap();
    settle(&mut ui, &mut changed);
    assert!(changed.is_empty());
    assert_eq!(ui.root().paragraph().unwrap().text.as_ref(), "new 👩‍👩‍👧‍👦");
    assert_eq!(old.as_ref(), "old");
}
