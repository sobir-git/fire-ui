use fire_ui::*;
use std::{cell::RefCell, convert::Infallible, rc::Rc};

struct SemanticProbe {
    label: &'static str,
    moves: usize,
}
impl Widget for SemanticProbe {
    type Command = bool;
    type Output = Infallible;
    fn update(&mut self, cx: &mut Update<'_, Self>, change: bool) {
        if change {
            self.label = "changed";
        }
        cx.repaint();
    }
    fn input(&mut self, _cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if phase == Phase::Target && matches!(input, Input::Pointer { .. }) {
            self.moves += 1;
        }
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::Text,
            label: self.label.into(),
            ..Semantics::default()
        }
    }
}

#[test]
fn inert_pointer_moves_do_not_rebuild_semantics_but_updates_do() {
    let mut ui = Ui::new(
        Element::leaf(SemanticProbe {
            label: "same",
            moves: 0,
        }),
        Size::new(100., 40.),
        Limits::default(),
    )
    .unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    ui.layout(&mut TestText);
    let revision = ui.semantic_revision();
    for i in 0..100 {
        ui.dispatch(
            Input::Pointer {
                pointer: 0,
                position: Point::new(i as f32 % 100., 5.),
            },
            &mut TestText,
        );
    }
    assert_eq!(ui.root().moves, 100);
    assert_eq!(ui.semantic_revision(), revision);
    ui.send(false).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    assert!(ui.semantic_revision() > revision);
    let revision = ui.semantic_revision();
    ui.send(true).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    assert!(ui.semantic_revision() > revision);
    assert_eq!(ui.semantics()[0].semantics.label, "changed");
}

struct FramePaint;
impl Widget for FramePaint {
    type Command = Infallible;
    type Output = Infallible;
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        if event == Lifecycle::Mount {
            cx.request_frame();
        }
    }
    fn frame(&mut self, cx: &mut Update<'_, Self>, _: FrameTime) {
        cx.repaint();
    }
}

#[test]
fn frame_repaint_does_not_rebuild_semantics() {
    let mut ui = Ui::new(
        Element::leaf(FramePaint),
        Size::new(100., 40.),
        Limits::default(),
    )
    .unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    ui.layout(&mut TestText);
    let revision = ui.semantic_revision();
    ui.frame(std::time::Duration::from_millis(16), 100);
    assert_eq!(ui.semantic_revision(), revision);
}

struct OrderLeaf(&'static str);
impl Widget for OrderLeaf {
    type Command = Infallible;
    type Output = Infallible;
    fn focusable(&self) -> bool {
        true
    }
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::Button,
            label: self.0.into(),
            ..Semantics::default()
        }
    }
}
struct OrderRoot {
    first: Child<OrderLeaf>,
    second: Child<OrderLeaf>,
}
impl Widget for OrderRoot {
    type Command = Infallible;
    type Output = Infallible;
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        for child in [self.first, self.second] {
            cx.measure(child, Constraints::loose(Size::new(20., 20.)));
        }
        cx.place(self.first, Point::default());
        cx.place(self.second, Point::new(30., 0.));
        Metrics::new(c.max)
    }
}

#[test]
fn semantics_and_tab_order_follow_child_order_not_creation_order() {
    let created_first = Element::leaf(OrderLeaf("second"));
    let created_second = Element::leaf(OrderLeaf("first"));
    let root = Element::build(|c| OrderRoot {
        first: c.add(created_second),
        second: c.add(created_first),
    });
    let mut ui = Ui::new(root, Size::new(100., 40.), Limits::default()).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    ui.layout(&mut TestText);
    let labels: Vec<_> = ui
        .semantics()
        .into_iter()
        .filter(|n| n.semantics.role == Role::Button)
        .map(|n| n.semantics.label)
        .collect();
    assert_eq!(labels, ["first", "second"]);
    ui.dispatch(
        Input::Key {
            key: Key::Tab,
            physical: 1,
            down: true,
            repeat: false,
            modifiers: Modifiers::default(),
        },
        &mut TestText,
    );
    assert!(ui.focused(ui.root().first));
}

struct OverlayLeaf {
    name: &'static str,
    hits: Rc<RefCell<Vec<&'static str>>>,
}
impl Widget for OverlayLeaf {
    type Command = Infallible;
    type Output = Infallible;
    fn layout(&mut self, _: &mut Layout<'_>, _: Constraints) -> Metrics {
        Metrics::new(Size::new(20., 20.))
    }
    fn input(&mut self, _: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if phase == Phase::Target && matches!(input, Input::Button { down: true, .. }) {
            self.hits.borrow_mut().push(self.name);
        }
    }
}
struct OverlayOrderRoot {
    first: Child<OverlayLeaf>,
    last: Child<OverlayLeaf>,
}
impl Widget for OverlayOrderRoot {
    type Command = Infallible;
    type Output = Infallible;
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        if event == Lifecycle::Mount {
            cx.anchor(self.first, Anchor::Window).unwrap();
            cx.anchor(self.last, Anchor::Window).unwrap();
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        cx.overlay(c)
    }
}

#[test]
fn overlay_hit_order_follows_tree_order() {
    let hits: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let created_first = Element::leaf(OverlayLeaf {
        name: "last",
        hits: hits.clone(),
    });
    let created_second = Element::leaf(OverlayLeaf {
        name: "first",
        hits: hits.clone(),
    });
    let root = Element::build(|c| OverlayOrderRoot {
        first: c.add(created_second),
        last: c.add(created_first),
    });
    let mut ui = Ui::new(root, Size::new(100., 40.), Limits::default()).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    ui.layout(&mut TestText);
    ui.dispatch(
        Input::Button {
            pointer: 0,
            button: 1,
            down: true,
            position: Point::new(5., 5.),
        },
        &mut TestText,
    );
    assert_eq!(&*hits.borrow(), &["last"]);
}

struct PaintSink;
impl Painter for PaintSink {
    fn save(&mut self) {}
    fn restore(&mut self) {}
    fn transform(&mut self, _: Transform) {}
    fn clip(&mut self, _: Rect) {}
    fn opacity(&mut self, _: f32) {}
    fn rect(&mut self, _: Rect, _: f32, _: Brush) {}
    fn stroke(&mut self, _: Rect, _: f32, _: f32, _: Color) {}
    fn path(&mut self, _: &[Path], _: Brush, _: Option<f32>) {}
    fn paragraph(&mut self, _: &Paragraph, _: Point, _: Brush) {}
}

struct ManyNodes {
    overlay: Child<OverlayLeaf>,
}
impl Widget for ManyNodes {
    type Command = Infallible;
    type Output = Infallible;
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        if event == Lifecycle::Mount {
            cx.anchor(self.overlay, Anchor::Window).unwrap();
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, limits: Constraints) -> Metrics {
        cx.overlay(limits)
    }
}

#[test]
fn pointer_and_paint_do_not_walk_all_nodes_to_find_overlays() {
    let hits = Rc::default();
    let element = Element::build(|children| {
        for _ in 0..200 {
            children.add(Element::leaf(Empty));
        }
        ManyNodes {
            overlay: children.add(Element::leaf(OverlayLeaf {
                name: "overlay",
                hits,
            })),
        }
    });
    let mut ui = Ui::new(element, Size::new(100., 40.), Limits::default()).unwrap();
    ui.pump(300, |v| match v {}, |_| {});
    ui.layout(&mut TestText);
    let walked = ui.stats().tree_walked;
    ui.dispatch(
        Input::Pointer {
            pointer: 0,
            position: Point::new(5., 5.),
        },
        &mut TestText,
    );
    assert_eq!(ui.stats().tree_walked, walked);
    ui.paint(&mut PaintSink);
    assert_eq!(ui.stats().tree_walked, walked);
}

struct RemovedLayoutChild {
    child: Child<Empty>,
}
impl Widget for RemovedLayoutChild {
    type Command = ();
    type Output = Infallible;
    fn update(&mut self, cx: &mut Update<'_, Self>, _: ()) {
        cx.remove(self.child).unwrap();
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        let measured = cx.measure(self.child, c);
        cx.place(self.child, Point::default());
        assert_eq!(measured.size, Size::ZERO);
        Metrics::new(c.max)
    }
}

#[test]
fn layout_skips_a_child_handle_after_removal() {
    let root = Element::build(|c| RemovedLayoutChild {
        child: c.add(Element::leaf(Empty)),
    });
    let mut ui = Ui::new(root, Size::new(100., 40.), Limits::default()).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    ui.send(()).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    ui.layout(&mut TestText);
}

struct EarlyRoot(Rc<RefCell<Vec<&'static str>>>);
impl Widget for EarlyRoot {
    type Command = Infallible;
    type Output = Infallible;
    fn close_requested(&mut self, _: &mut Update<'_, Self>) -> bool {
        self.0.borrow_mut().push("close");
        true
    }
    fn lifecycle(&mut self, _: &mut Update<'_, Self>, event: Lifecycle) {
        if matches!(event, Lifecycle::Moved { .. }) {
            self.0.borrow_mut().push("moved");
        }
    }
    fn input(&mut self, _: &mut Update<'_, Self>, _: Phase, input: &Input) {
        if matches!(input, Input::FileDropped(_)) {
            self.0.borrow_mut().push("file");
        }
    }
}

#[test]
fn root_callbacks_wait_until_mount() {
    let calls: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let mut ui = Ui::new(
        Element::leaf(EarlyRoot(calls.clone())),
        Size::new(100., 40.),
        Limits::default(),
    )
    .unwrap();
    assert!(ui.request_close());
    ui.window_position(1, 2);
    ui.dispatch(Input::FileDropped("file.txt".into()), &mut TestText);
    assert!(calls.borrow().is_empty());
    ui.pump(100, |v| match v {}, |_| {});
    assert!(ui.request_close());
    assert_eq!(&*calls.borrow(), &["close"]);
}

struct LimitProbe(Rc<RefCell<Vec<bool>>>);
impl Widget for LimitProbe {
    type Command = ();
    type Output = Infallible;
    fn update(&mut self, cx: &mut Update<'_, Self>, _: ()) {
        let mut result = self.0.borrow_mut();
        result.push(cx.insert(Element::leaf(Empty), |v| match *v {}).is_ok());
        result.push(cx.insert(Element::leaf(Empty), |v| match *v {}).is_ok());
        result.push(
            cx.after(Timer::new(), std::time::Duration::from_secs(1))
                .is_ok(),
        );
        result.push(
            cx.after(Timer::new(), std::time::Duration::from_secs(1))
                .is_ok(),
        );
        result.push(cx.replace_task(TaskSlot::new()).is_ok());
        result.push(cx.replace_task(TaskSlot::new()).is_ok());
    }
}

#[test]
fn callback_and_node_limits_are_configurable() {
    let results: Rc<RefCell<Vec<bool>>> = Rc::default();
    let mut ui = Ui::new(
        Element::leaf(LimitProbe(results.clone())),
        Size::new(100., 40.),
        Limits {
            mutations_per_callback: 1,
            timers_per_node: 1,
            tasks_per_node: 1,
            ..Limits::default()
        },
    )
    .unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    ui.send(()).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    assert_eq!(&*results.borrow(), &[true, false, true, false, true, false]);
}

type Log = Rc<RefCell<Vec<(usize, Lifecycle)>>>;

struct Probe {
    number: usize,
    log: Log,
    insert_on_hover: bool,
}
impl Widget for Probe {
    type Command = ();
    type Output = Infallible;
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        self.log.borrow_mut().push((self.number, event));
        if self.insert_on_hover && event == Lifecycle::Hover(true) {
            let _ = cx.insert(Element::leaf(Empty), |v| match *v {});
        }
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if phase == Phase::Target && matches!(input, Input::Button { down: true, .. }) {
            cx.focus().unwrap();
            cx.capture(1).unwrap();
        }
    }
    fn focusable(&self) -> bool {
        true
    }
    fn layout(&mut self, _: &mut Layout<'_>, limits: Constraints) -> Metrics {
        Metrics::new(limits.constrain(Size::new(10., 10.)))
    }
}
struct Empty;
impl Widget for Empty {
    type Command = ();
    type Output = Infallible;
}

#[derive(Debug)]
enum Command {
    Remove,
    Hide,
    Anchor,
    Chain,
    RemoveThenAnchor,
}
impl Data for Command {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}
struct Root {
    children: [Child<Probe>; 3],
    overlay: bool,
}
impl Widget for Root {
    type Command = Command;
    type Output = Infallible;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: Command) {
        let [a, b, c] = self.children;
        match command {
            Command::Remove => cx.remove(a).unwrap(),
            Command::Hide => cx.show(a, false).unwrap(),
            Command::Anchor => cx.anchor(b, Anchor::to(a)).unwrap(),
            Command::Chain => {
                cx.anchor(b, Anchor::to(a)).unwrap();
                cx.anchor(c, Anchor::to(b)).unwrap();
            }
            Command::RemoveThenAnchor => {
                cx.remove(b).unwrap();
                cx.anchor(b, Anchor::to(a)).unwrap();
            }
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, limits: Constraints) -> Metrics {
        if self.overlay {
            return cx.overlay(limits);
        }
        for (i, child) in self.children.into_iter().enumerate() {
            cx.measure(child, Constraints::loose(Size::new(10., 10.)));
            cx.place(child, Point::new((i + 1) as f32 * 10., 0.));
        }
        Metrics::new(limits.max)
    }
}
fn setup(overlay: bool) -> (Ui<Root>, Log) {
    let log: Log = Rc::default();
    let root = Element::build(|children| Root {
        children: std::array::from_fn(|number| {
            children.add(Element::leaf(Probe {
                number,
                log: log.clone(),
                insert_on_hover: false,
            }))
        }),
        overlay,
    });
    let mut ui = Ui::new(root, Size::new(100., 100.), Limits::default()).unwrap();
    pump(&mut ui);
    ui.layout(&mut TestText);
    log.borrow_mut().clear();
    (ui, log)
}
fn pump(ui: &mut Ui<Root>) {
    ui.pump(100, |v| match v {}, |_| {});
}

#[test]
fn retirement_delivers_input_cancellation_before_unmount() {
    let (mut ui, log) = setup(false);
    ui.dispatch(
        Input::Button {
            pointer: 1,
            button: 1,
            down: true,
            position: Point::new(12., 2.),
        },
        &mut TestText,
    );
    assert!(ui.focused(ui.root().children[0]));
    assert!(ui.captured(1, ui.root().children[0]));
    log.borrow_mut().clear();
    ui.send(Command::Remove).unwrap();
    pump(&mut ui);
    let events: Vec<_> = log
        .borrow()
        .iter()
        .filter(|(id, _)| *id == 0)
        .map(|(_, event)| *event)
        .collect();
    assert_eq!(
        events,
        vec![
            Lifecycle::CancelKeys,
            Lifecycle::Focus(false),
            Lifecycle::CaptureLost(1),
            Lifecycle::Unmount
        ]
    );
}

#[test]
fn hiding_hovered_child_clears_hover_without_pointer_motion() {
    let (mut ui, log) = setup(false);
    ui.dispatch(
        Input::Pointer {
            pointer: 1,
            position: Point::new(12., 2.),
        },
        &mut TestText,
    );
    assert!(log.borrow().contains(&(0, Lifecycle::Hover(true))));
    log.borrow_mut().clear();
    ui.send(Command::Hide).unwrap();
    pump(&mut ui);
    assert!(log.borrow().contains(&(0, Lifecycle::Hover(false))));
}

#[test]
fn queued_mutation_after_removal_does_not_panic() {
    let (mut ui, _) = setup(false);
    ui.send(Command::RemoveThenAnchor).unwrap();
    pump(&mut ui);
    assert!(ui.geometry(ui.root().children[1]).is_none());
}

#[test]
fn default_overlay_preserves_anchor_availability() {
    let (mut ui, _) = setup(true);
    ui.send(Command::Anchor).unwrap();
    pump(&mut ui);
    ui.layout(&mut TestText);
    ui.send(Command::Hide).unwrap();
    pump(&mut ui);
    ui.layout(&mut TestText);
    assert!(!ui.geometry(ui.root().children[1]).unwrap().visible);
}

#[test]
fn anchor_chains_publish_in_dependency_order() {
    for _ in 0..32 {
        let (mut ui, _) = setup(false);
        ui.send(Command::Chain).unwrap();
        pump(&mut ui);
        ui.layout(&mut TestText);
        assert_eq!(ui.geometry(ui.root().children[2]).unwrap().bounds.x, 60.);
    }
}

#[test]
fn inverse_never_returns_nonfinite_coefficients() {
    let transform = Transform([1e30, 0., 0., 1e30, 1e30, 1e30]);
    if let Some(inverse) = transform.inverse() {
        assert!(inverse.0.into_iter().all(f32::is_finite));
    }
}

#[test]
fn deferred_insertions_share_the_node_admission_limit() {
    let log: Log = Rc::default();
    let root = Element::build(|children| Root {
        children: std::array::from_fn(|number| {
            children.add(Element::leaf(Probe {
                number,
                log: log.clone(),
                insert_on_hover: true,
            }))
        }),
        overlay: false,
    });
    let limits = Limits {
        nodes: 5,
        ..Limits::default()
    };
    let mut ui = Ui::new(root, Size::new(100., 100.), limits).unwrap();
    pump(&mut ui);
    ui.dispatch(
        Input::Pointer {
            pointer: 1,
            position: Point::new(12., 2.),
        },
        &mut TestText,
    );
    ui.dispatch(
        Input::Pointer {
            pointer: 1,
            position: Point::new(22., 2.),
        },
        &mut TestText,
    );
    pump(&mut ui);
    assert!(
        ui.stats().nodes <= limits.nodes,
        "{} nodes admitted with limit {}",
        ui.stats().nodes,
        limits.nodes
    );
}

#[test]
fn window_focus_loss_clears_keys_even_without_a_focused_widget() {
    let (mut ui, _) = setup(false);
    ui.dispatch(
        Input::Key {
            key: Key::Character('a'),
            physical: 30,
            down: true,
            repeat: false,
            modifiers: Modifiers::default(),
        },
        &mut TestText,
    );
    assert!(ui.pressed(30));
    ui.window_focus(false);
    assert!(!ui.pressed(30));
}

#[derive(Debug)]
enum FocusCommand {
    Focus(usize),
    Open(usize),
    Close,
    Hide(usize),
    Remove(usize),
    Send(usize, Box<FocusCommand>),
    PreviewFocus(usize),
    Noop,
}
impl Data for FocusCommand {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Send(_, command) => command.bytes(),
                _ => 0,
            }
    }
}
struct FocusTree {
    number: usize,
    children: Vec<Child<FocusTree>>,
    preview_focus: Option<usize>,
    reclaim_focus: bool,
    log: Log,
    inputs: Rc<RefCell<Vec<(usize, Phase)>>>,
}
impl Widget for FocusTree {
    type Command = FocusCommand;
    type Output = Infallible;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: FocusCommand) {
        match command {
            FocusCommand::Focus(index) => cx.focus_child(self.children[index]).unwrap(),
            FocusCommand::Open(index) => cx.open_modal(self.children[index]).unwrap(),
            FocusCommand::Close => cx.close_modal().unwrap(),
            FocusCommand::Hide(index) => cx.show(self.children[index], false).unwrap(),
            FocusCommand::Remove(index) => cx.remove(self.children.remove(index)).unwrap(),
            FocusCommand::Send(index, command) => cx.send(self.children[index], *command).unwrap(),
            FocusCommand::PreviewFocus(index) => self.preview_focus = Some(index),
            FocusCommand::Noop => {}
        }
    }
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        self.log.borrow_mut().push((self.number, event));
        if self.reclaim_focus && event == Lifecycle::Focus(false) {
            let _ = cx.focus();
        }
    }
    fn focusable(&self) -> bool {
        self.children.is_empty()
    }
    fn input(&mut self, cx: &mut Update<'_, Self>, phase: Phase, input: &Input) {
        if matches!(input, Input::Key { .. }) {
            self.inputs.borrow_mut().push((self.number, phase));
            if phase == Phase::Preview {
                if let Some(index) = self.preview_focus {
                    cx.focus_child(self.children[index]).unwrap();
                }
            }
        }
    }
}
fn focus_leaf(
    number: usize,
    log: &Log,
    inputs: &Rc<RefCell<Vec<(usize, Phase)>>>,
    reclaim_focus: bool,
) -> Element<FocusTree> {
    focus_branch(number, vec![], log, inputs, reclaim_focus)
}
fn focus_branch(
    number: usize,
    elements: Vec<Element<FocusTree>>,
    log: &Log,
    inputs: &Rc<RefCell<Vec<(usize, Phase)>>>,
    reclaim_focus: bool,
) -> Element<FocusTree> {
    Element::build(|children| FocusTree {
        number,
        children: elements
            .into_iter()
            .map(|element| children.add(element))
            .collect(),
        preview_focus: None,
        reclaim_focus,
        log: log.clone(),
        inputs: inputs.clone(),
    })
}
fn focus_pump(ui: &mut Ui<FocusTree>, command: FocusCommand) {
    ui.send(command).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
}

#[test]
fn preview_focus_transfer_cancels_the_old_keyboard_route() {
    let log = Rc::default();
    let inputs = Rc::default();
    let element = focus_branch(
        0,
        vec![
            focus_leaf(1, &log, &inputs, false),
            focus_leaf(2, &log, &inputs, false),
        ],
        &log,
        &inputs,
        false,
    );
    let mut ui = Ui::new(element, Size::new(100., 100.), Limits::default()).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    focus_pump(&mut ui, FocusCommand::Focus(0));
    focus_pump(&mut ui, FocusCommand::PreviewFocus(1));
    ui.dispatch(
        Input::Key {
            key: Key::Enter,
            physical: 13,
            down: true,
            repeat: false,
            modifiers: Modifiers::default(),
        },
        &mut TestText,
    );
    assert_eq!(*inputs.borrow(), vec![(0, Phase::Preview)]);
    assert!(ui.focused(ui.root().children[1]));
    assert!(!ui.pressed(13));
}

#[test]
fn nested_modal_close_hide_and_remove_restore_eligible_focus() {
    for finish in [
        FocusCommand::Close,
        FocusCommand::Hide(1),
        FocusCommand::Remove(1),
    ] {
        let log = Rc::default();
        let inputs = Rc::default();
        let mut inner_leaf = None;
        let inner = Element::build(|children| {
            inner_leaf = Some(children.add(focus_leaf(5, &log, &inputs, false)));
            FocusTree {
                number: 4,
                children: vec![inner_leaf.unwrap()],
                preview_focus: None,
                reclaim_focus: false,
                log: log.clone(),
                inputs: inputs.clone(),
            }
        });
        let mut outer_leaf = None;
        let outer = Element::build(|children| {
            outer_leaf = Some(children.add(focus_leaf(3, &log, &inputs, false)));
            FocusTree {
                number: 2,
                children: vec![outer_leaf.unwrap(), children.add(inner)],
                preview_focus: None,
                reclaim_focus: false,
                log: log.clone(),
                inputs: inputs.clone(),
            }
        });
        let element = focus_branch(
            0,
            vec![focus_leaf(1, &log, &inputs, false), outer],
            &log,
            &inputs,
            false,
        );
        let mut ui = Ui::new(element, Size::new(100., 100.), Limits::default()).unwrap();
        ui.pump(100, |v| match v {}, |_| {});
        focus_pump(&mut ui, FocusCommand::Focus(0));
        focus_pump(&mut ui, FocusCommand::Open(1));
        assert!(ui.focused(outer_leaf.unwrap()));
        focus_pump(
            &mut ui,
            FocusCommand::Send(1, Box::new(FocusCommand::Open(1))),
        );
        assert!(ui.focused(inner_leaf.unwrap()));
        focus_pump(&mut ui, FocusCommand::Send(1, Box::new(finish)));
        assert!(ui.focused(outer_leaf.unwrap()));
        focus_pump(&mut ui, FocusCommand::Close);
        assert!(ui.focused(ui.root().children[0]));
    }
}

#[test]
fn notification_focus_changes_yield_with_a_bounded_mailbox() {
    let log = Rc::default();
    let inputs = Rc::default();
    let element = focus_branch(
        0,
        vec![
            focus_leaf(1, &log, &inputs, true),
            focus_leaf(2, &log, &inputs, true),
        ],
        &log,
        &inputs,
        false,
    );
    let limits = Limits {
        messages: 3,
        ..Limits::default()
    };
    let mut ui = Ui::new(element, Size::new(100., 100.), limits).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    focus_pump(&mut ui, FocusCommand::Focus(0));
    ui.send(FocusCommand::Focus(1)).unwrap();
    ui.send(FocusCommand::Noop).unwrap();
    ui.send(FocusCommand::Noop).unwrap();
    assert_eq!(ui.pump(1, |v| match v {}, |_| {}), 1);
    assert!(ui.focused(ui.root().children[1]));
    assert_eq!(ui.stats().queued, 3);
    assert_eq!(ui.pump(9, |v| match v {}, |_| {}), 9);
    assert!(ui.next_work().ready);
    assert!(ui.stats().queued <= limits.messages);
    assert!(log.borrow().len() < 50);
}

#[test]
fn nested_modal_hide_or_remove_finds_fallback_when_saved_focus_is_hidden() {
    for finish in [FocusCommand::Hide(1), FocusCommand::Remove(1)] {
        let log = Rc::default();
        let inputs = Rc::default();
        let inner = focus_branch(
            4,
            vec![focus_leaf(5, &log, &inputs, false)],
            &log,
            &inputs,
            false,
        );
        let mut fallback = None;
        let outer = Element::build(|children| {
            let first = children.add(focus_leaf(3, &log, &inputs, false));
            let inner = children.add(inner);
            fallback = Some(children.add(focus_leaf(6, &log, &inputs, false)));
            FocusTree {
                number: 2,
                children: vec![first, inner, fallback.unwrap()],
                preview_focus: None,
                reclaim_focus: false,
                log: log.clone(),
                inputs: inputs.clone(),
            }
        });
        let element = focus_branch(
            0,
            vec![focus_leaf(1, &log, &inputs, false), outer],
            &log,
            &inputs,
            false,
        );
        let mut ui = Ui::new(element, Size::new(100., 100.), Limits::default()).unwrap();
        ui.pump(100, |v| match v {}, |_| {});
        focus_pump(&mut ui, FocusCommand::Focus(0));
        focus_pump(&mut ui, FocusCommand::Open(1));
        focus_pump(
            &mut ui,
            FocusCommand::Send(1, Box::new(FocusCommand::Open(1))),
        );
        focus_pump(
            &mut ui,
            FocusCommand::Send(1, Box::new(FocusCommand::Hide(0))),
        );
        focus_pump(&mut ui, FocusCommand::Send(1, Box::new(finish)));
        assert!(
            ui.focused(fallback.unwrap()),
            "remaining modal must focus its eligible fallback"
        );
    }
}

struct HoverContainer {
    children: [Child<Probe>; 2],
    log: Log,
}
impl Widget for HoverContainer {
    type Command = ();
    type Output = Infallible;
    fn lifecycle(&mut self, _: &mut Update<'_, Self>, event: Lifecycle) {
        self.log.borrow_mut().push((9, event));
    }
    fn update(&mut self, cx: &mut Update<'_, Self>, _: ()) {
        cx.show(self.children[0], false).unwrap();
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        for (i, child) in self.children.iter().enumerate() {
            cx.measure(*child, Constraints::tight(Size::new(10., 10.)));
            cx.place(*child, Point::new(10. + i as f32 * 20., 0.));
        }
        Metrics::new(c.max)
    }
}
#[test]
fn hover_includes_the_container_without_flickering_between_its_children() {
    let log: Log = Rc::default();
    let root = Element::build(|c| HoverContainer {
        children: std::array::from_fn(|number| {
            c.add(Element::leaf(Probe {
                number,
                log: log.clone(),
                insert_on_hover: false,
            }))
        }),
        log: log.clone(),
    });
    let mut ui = Ui::new(root, Size::new(100., 100.), Limits::default()).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    log.borrow_mut().clear();
    for x in [2., 12., 32., 2., 120.] {
        ui.dispatch(
            Input::Pointer {
                pointer: 1,
                position: Point::new(x, 2.),
            },
            &mut TestText,
        );
    }
    let hover: Vec<_> = log
        .borrow()
        .iter()
        .copied()
        .filter(|(_, e)| matches!(e, Lifecycle::Hover(_)))
        .collect();
    assert_eq!(
        hover,
        vec![
            (9, Lifecycle::Hover(true)),
            (0, Lifecycle::Hover(true)),
            (0, Lifecycle::Hover(false)),
            (1, Lifecycle::Hover(true)),
            (1, Lifecycle::Hover(false)),
            (9, Lifecycle::Hover(false)),
        ]
    );
    log.borrow_mut().clear();
    ui.dispatch(
        Input::Pointer {
            pointer: 1,
            position: Point::new(12., 2.),
        },
        &mut TestText,
    );
    ui.send(()).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    assert!(log.borrow().contains(&(0, Lifecycle::Hover(false))));
    assert!(log.borrow().contains(&(9, Lifecycle::Hover(false))));
}
