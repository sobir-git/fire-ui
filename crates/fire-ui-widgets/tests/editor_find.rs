use fire_ui::*;
use fire_ui_widgets::*;

fn editor(text: &str, height: f32) -> Ui<Editor> {
    let mut ui = Ui::new(
        Editor::new(text),
        Size::new(300., height),
        Limits::default(),
    )
    .unwrap();
    settle(&mut ui);
    ui
}
/// Pump until idle and return what the editor reported about occurrences.
fn settle(ui: &mut Ui<Editor>) -> Vec<(Option<usize>, usize)> {
    let mut found = vec![];
    for _ in 0..20 {
        ui.pump(
            4096,
            |o| {
                if let EditorOutput::Found { selected, count } = o {
                    found.push((selected, count))
                }
            },
            |_| {},
        );
        ui.layout(&mut TestText);
        if !ui.next_work().ready {
            return found;
        }
    }
    panic!("editor did not settle");
}
fn send(ui: &mut Ui<Editor>, edit: Edit) -> Vec<(Option<usize>, usize)> {
    ui.send(edit).ok().unwrap();
    settle(ui)
}
fn type_text(ui: &mut Ui<Editor>, text: &str) -> Vec<(Option<usize>, usize)> {
    let id = ui.semantics()[0].id;
    ui.accessibility(id, SemanticAction::Focus).unwrap();
    ui.dispatch(
        Input::Text {
            session: ui.session(),
            text: text.into(),
        },
        &mut TestText,
    );
    settle(ui)
}
#[derive(Default)]
struct Rects(Vec<(Rect, Brush)>);
impl Painter for Rects {
    fn save(&mut self) {}
    fn restore(&mut self) {}
    fn transform(&mut self, _: Transform) {}
    fn clip(&mut self, _: Rect) {}
    fn opacity(&mut self, _: f32) {}
    fn rect(&mut self, rect: Rect, _: f32, brush: Brush) {
        self.0.push((rect, brush))
    }
    fn stroke(&mut self, _: Rect, _: f32, _: f32, _: Color) {}
    fn path(&mut self, _: &[Path], _: Brush, _: Option<f32>) {}
    fn paragraph(&mut self, _: &Paragraph, _: Point, _: Brush) {}
}
fn highlights(ui: &mut Ui<Editor>) -> usize {
    let mut rects = Rects::default();
    ui.paint(&mut rects);
    let highlight = Theme::default().color.highlight;
    rects
        .0
        .iter()
        .filter(|(_, b)| matches!(b, Brush::Solid(c) if *c == highlight))
        .count()
}

#[test]
fn find_selects_the_first_occurrence_from_the_caret_and_steps_around() {
    let mut ui = editor("fire, Fire and FIRE", 140.);
    ui.send(Edit::Select {
        anchor: 3,
        caret: 3,
    })
    .ok()
    .unwrap();
    settle(&mut ui);
    assert_eq!(send(&mut ui, Edit::Find("fire".into())), [(Some(1), 3)]);
    assert_eq!(ui.root().selection(), Some(6..10));
    assert_eq!(send(&mut ui, Edit::FindNext), [(Some(2), 3)]);
    assert_eq!(send(&mut ui, Edit::FindNext), [(Some(0), 3)]);
    assert_eq!(ui.root().selection(), Some(0..4));
    assert_eq!(send(&mut ui, Edit::FindPrevious), [(Some(2), 3)]);
    assert_eq!(ui.root().selection(), Some(15..19));
    // Refining the query keeps the current occurrence when it still matches.
    assert_eq!(send(&mut ui, Edit::Find("FIRE".into())), [(Some(2), 3)]);
    assert_eq!(ui.root().selection(), Some(15..19));
}

#[test]
fn a_missing_query_leaves_the_selection_and_view_alone() {
    let text = (0..60).map(|i| format!("line {i}\n")).collect::<String>();
    let mut ui = editor(&text, 100.);
    ui.send(Edit::Restore(EditorState {
        scroll: Point::new(0., 300.),
        ..EditorState::default()
    }))
    .ok()
    .unwrap();
    settle(&mut ui);
    let scroll = ui.root().scroll_offset();
    assert!(scroll > 0.);
    assert_eq!(send(&mut ui, Edit::Find("ember".into())), [(None, 0)]);
    assert_eq!(send(&mut ui, Edit::FindNext), [(None, 0)]);
    assert_eq!(ui.root().selection(), None);
    assert_eq!(ui.root().scroll_offset(), scroll);
}

#[test]
fn occurrences_follow_edits_and_undo() {
    let mut ui = editor("one fire", 140.);
    send(&mut ui, Edit::Find("fire".into()));
    ui.send(Edit::Select {
        anchor: 0,
        caret: 0,
    })
    .ok()
    .unwrap();
    assert_eq!(settle(&mut ui), [(None, 1)]);
    assert_eq!(type_text(&mut ui, "fire "), [(None, 2)]);
    assert_eq!(send(&mut ui, Edit::FindNext), [(Some(1), 2)]);
    assert_eq!(ui.root().selection(), Some(9..13));
    assert_eq!(send(&mut ui, Edit::Undo), [(None, 1)]);
    // Replacing the text keeps the query and recounts.
    assert_eq!(
        send(&mut ui, Edit::Set("fire fire fire".into())),
        [(None, 3)]
    );
}

#[test]
fn only_visible_occurrences_paint_and_an_empty_query_clears_them() {
    let text = "fire\n".repeat(100);
    let mut ui = editor(&text, 100.);
    assert_eq!(highlights(&mut ui), 0);
    send(&mut ui, Edit::Find("fire".into()));
    let painted = highlights(&mut ui);
    assert!(painted > 0 && painted < 10, "painted {painted} of 100");
    assert_eq!(send(&mut ui, Edit::Find("".into())), [(None, 0)]);
    assert_eq!(highlights(&mut ui), 0);
    // Steps without a query select nothing.
    assert!(send(&mut ui, Edit::FindNext).is_empty());
}
