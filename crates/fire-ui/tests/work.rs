use fire_ui::*;
use std::{cell::RefCell, convert::Infallible, rc::Rc, time::Duration};

#[derive(Debug)]
enum WorkCommand {
    Replace,
    Cancel,
    Value(String),
    Tasks,
    Timers,
    ReuseTask,
    ReuseTimer,
}
impl Data for WorkCommand {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Value(s) => s.capacity(),
                _ => 0,
            }
    }
}
#[derive(Default)]
struct Worker {
    slot: TaskSlot,
    tickets: Vec<Ticket<Self>>,
    accepted: usize,
    rejected: usize,
    values: Vec<String>,
    fired: usize,
}
impl Widget for Worker {
    type Command = WorkCommand;
    type Output = Infallible;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: WorkCommand) {
        match command {
            WorkCommand::Replace => self.tickets.push(cx.replace_task(self.slot).unwrap()),
            WorkCommand::Cancel => cx.cancel_task(self.slot),
            WorkCommand::Value(value) => self.values.push(value),
            WorkCommand::Tasks => {
                for _ in 0..65 {
                    match cx.replace_task(TaskSlot::new()) {
                        Ok(ticket) => {
                            self.tickets.push(ticket);
                            self.accepted += 1;
                        }
                        Err(Error::Full) => self.rejected += 1,
                        Err(error) => panic!("unexpected {error:?}"),
                    }
                }
            }
            WorkCommand::Timers => {
                for _ in 0..65 {
                    match cx.after(Timer::new(), Duration::from_secs(1)) {
                        Ok(()) => self.accepted += 1,
                        Err(Error::Full) => self.rejected += 1,
                        Err(error) => panic!("unexpected {error:?}"),
                    }
                }
            }
            WorkCommand::ReuseTask => {
                let slots: Vec<_> = (0..64).map(|_| TaskSlot::new()).collect();
                for slot in &slots {
                    cx.replace_task(*slot).unwrap();
                }
                cx.cancel_task(slots[0]);
                self.tickets.push(cx.replace_task(TaskSlot::new()).unwrap());
            }
            WorkCommand::ReuseTimer => {
                let timers: Vec<_> = (0..64).map(|_| Timer::new()).collect();
                for timer in &timers {
                    cx.after(*timer, Duration::from_secs(1)).unwrap();
                }
                cx.cancel_timer(timers[0]);
                cx.after(Timer::new(), Duration::from_secs(1)).unwrap();
            }
        }
    }
    fn timer(&mut self, _: &mut Update<'_, Self>, _: Timer) {
        self.fired += 1;
    }
}
fn worker() -> Ui<Worker> {
    let mut ui = Ui::new(
        Element::leaf(Worker::default()),
        Size::new(20., 20.),
        Limits::default(),
    )
    .unwrap();
    drain(&mut ui);
    ui
}
fn drain<W: Widget<Output = Infallible>>(ui: &mut Ui<W>) {
    ui.pump(1000, |v| match v {}, |_| {});
}
fn command(ui: &mut Ui<Worker>, command: WorkCommand) {
    ui.send(command).unwrap();
    drain(ui);
}

#[test]
fn replaced_and_cancelled_task_tickets_reject_at_admission() {
    let mut ui = worker();
    command(&mut ui, WorkCommand::Replace);
    let first = ui.root().tickets[0];
    command(&mut ui, WorkCommand::Replace);
    let second = ui.root().tickets[1];
    let Err((Error::Stale, WorkCommand::Value(original))) =
        ui.post(first, WorkCommand::Value("first".into()))
    else {
        panic!("old ticket admitted")
    };
    assert_eq!(original, "first");
    ui.post(second, WorkCommand::Value("second".into()))
        .unwrap();
    drain(&mut ui);
    assert_eq!(ui.root().values, ["second"]);
    command(&mut ui, WorkCommand::Cancel);
    assert!(matches!(
        ui.post(second, WorkCommand::Value("cancelled".into())),
        Err((Error::Stale, _))
    ));
    assert_eq!(ui.stats().queued, 0);
}

#[test]
fn queued_task_results_are_revalidated_after_replacement_and_cancellation() {
    for mutation in [WorkCommand::Replace, WorkCommand::Cancel] {
        let mut ui = worker();
        command(&mut ui, WorkCommand::Replace);
        let ticket = ui.root().tickets[0];
        ui.send(mutation).unwrap();
        ui.post(ticket, WorkCommand::Value("stale result".into()))
            .unwrap();
        drain(&mut ui);
        assert!(ui.root().values.is_empty());
        assert_eq!(ui.stats().stale, 1);
        assert_eq!(ui.stats().queued_bytes, 0);
    }
}

#[test]
fn full_task_admission_preserves_owned_command_storage() {
    let mut ui = Ui::new(
        Element::leaf(Worker::default()),
        Size::new(20., 20.),
        Limits {
            messages: 1,
            ..Limits::default()
        },
    )
    .unwrap();
    drain(&mut ui);
    command(&mut ui, WorkCommand::Replace);
    let ticket = ui.root().tickets[0];
    ui.send(WorkCommand::Cancel).unwrap();
    let value = String::from("owned result");
    let allocation = value.as_ptr();
    let Err((Error::Full, WorkCommand::Value(returned))) =
        ui.post(ticket, WorkCommand::Value(value))
    else {
        panic!("a full mailbox must return the original command");
    };
    assert_eq!(returned, "owned result");
    assert_eq!(returned.as_ptr(), allocation);
    drain(&mut ui);
    assert!(ui.root().values.is_empty());
}

#[test]
fn task_requests_obey_the_limit_within_one_callback() {
    let mut ui = worker();
    command(&mut ui, WorkCommand::Tasks);
    assert_eq!((ui.root().accepted, ui.root().rejected), (64, 1));
}

#[test]
fn timer_requests_obey_the_limit_within_one_callback() {
    let mut ui = worker();
    command(&mut ui, WorkCommand::Timers);
    assert_eq!((ui.root().accepted, ui.root().rejected), (64, 1));
    ui.advance(Duration::from_secs(1), 100);
    assert_eq!(ui.root().fired, 64);
    assert_eq!(ui.next_work().deadline, None);
}

#[test]
fn cancelling_requests_releases_capacity_in_the_same_callback() {
    let mut ui = worker();
    command(&mut ui, WorkCommand::ReuseTask);
    ui.post(
        ui.root().tickets[0],
        WorkCommand::Value("replacement".into()),
    )
    .unwrap();
    drain(&mut ui);
    assert_eq!(ui.root().values, ["replacement"]);
    command(&mut ui, WorkCommand::ReuseTimer);
    ui.advance(Duration::from_secs(1), 100);
    assert_eq!(ui.root().fired, 64);
}

struct Original(String);
impl Data for Original {
    fn bytes(&self) -> usize {
        self.0.bytes()
    }
}
struct Source {
    original: Option<Original>,
    rejected: Rc<RefCell<Option<(Error, Original)>>>,
}
impl Widget for Source {
    type Command = ();
    type Output = Original;
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        if event == Lifecycle::Mount {
            if let Err(rejected) = cx.emit(self.original.take().unwrap()) {
                *self.rejected.borrow_mut() = Some(rejected);
            }
        }
    }
}
struct Sink {
    received: Vec<String>,
}
impl Widget for Sink {
    type Command = String;
    type Output = Infallible;
    fn update(&mut self, _: &mut Update<'_, Self>, value: String) {
        self.received.push(value);
    }
}

#[test]
fn mapped_output_full_returns_the_original_owned_value() {
    let rejected: Rc<RefCell<Option<(Error, Original)>>> = Rc::default();
    let original = String::from("original");
    let allocation = original.as_ptr();
    let element = Element::build(|children| {
        children.connect(
            Element::leaf(Source {
                original: Some(Original(original)),
                rejected: rejected.clone(),
            }),
            |value| value.0.repeat(100),
        );
        Sink { received: vec![] }
    });
    let mut ui = Ui::new(
        element,
        Size::new(20., 20.),
        Limits {
            message_bytes: 64,
            ..Limits::default()
        },
    )
    .unwrap();
    drain(&mut ui);
    let rejected = rejected.borrow();
    let (error, original) = rejected
        .as_ref()
        .expect("expanded command must exceed byte limit");
    assert_eq!(*error, Error::Full);
    assert_eq!(original.0, "original");
    assert_eq!(original.0.as_ptr(), allocation);
    assert!(ui.root().received.is_empty());
    assert_eq!(ui.stats().queued_bytes, 0);
}

#[test]
fn mapped_output_reserves_the_expanded_command_bytes_until_delivery() {
    let rejected: Rc<RefCell<Option<(Error, Original)>>> = Rc::default();
    let element = Element::build(|children| {
        children.connect(
            Element::leaf(Source {
                original: Some(Original("x".into())),
                rejected: rejected.clone(),
            }),
            |value| value.0.repeat(100),
        );
        Sink { received: vec![] }
    });
    let mut ui = Ui::new(
        element,
        Size::new(20., 20.),
        Limits {
            message_bytes: 128,
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(ui.pump(2, |v| match v {}, |_| {}), 2);
    assert_eq!(
        ui.stats().queued_bytes,
        String::from("x").repeat(100).bytes()
    );
    assert!(matches!(ui.send("another".into()), Err((Error::Full, _))));
    drain(&mut ui);
    assert_eq!(ui.root().received, ["x".repeat(100)]);
    assert!(rejected.borrow().is_none());
}

struct Animated {
    number: usize,
    frames: Rc<RefCell<Vec<usize>>>,
}
impl Widget for Animated {
    type Command = ();
    type Output = Infallible;
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        if event == Lifecycle::Mount {
            cx.request_frame();
        }
    }
    fn frame(&mut self, cx: &mut Update<'_, Self>, _: FrameTime) {
        self.frames.borrow_mut().push(self.number);
        cx.request_frame();
    }
}
struct AnimationRoot {
    children: Vec<Child<Animated>>,
    hide_on_frame: bool,
}
impl Widget for AnimationRoot {
    type Command = ();
    type Output = Infallible;
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        if self.hide_on_frame && event == Lifecycle::Mount {
            cx.request_frame();
        }
    }
    fn frame(&mut self, cx: &mut Update<'_, Self>, _: FrameTime) {
        for child in &self.children {
            cx.show(*child, false).unwrap();
        }
    }
}
fn animation(hide_on_frame: bool, frames: Rc<RefCell<Vec<usize>>>) -> Ui<AnimationRoot> {
    let element = Element::build(|children| AnimationRoot {
        children: (0..3)
            .map(|number| {
                children.add(Element::leaf(Animated {
                    number,
                    frames: frames.clone(),
                }))
            })
            .collect(),
        hide_on_frame,
    });
    let mut ui = Ui::new(element, Size::new(20., 20.), Limits::default()).unwrap();
    drain(&mut ui);
    ui
}

#[test]
fn continuously_requested_frames_do_not_starve_siblings() {
    let frames: Rc<RefCell<Vec<usize>>> = Rc::default();
    let mut ui = animation(false, frames.clone());
    for step in 1..=9 {
        ui.frame(Duration::from_millis(step * 16), 1);
    }
    assert_eq!(*frames.borrow(), vec![0, 1, 2, 0, 1, 2, 0, 1, 2]);
}

#[test]
fn frame_callback_can_cancel_remaining_selected_subtree_frames() {
    let frames: Rc<RefCell<Vec<usize>>> = Rc::default();
    let mut ui = animation(true, frames.clone());
    ui.frame(Duration::from_millis(16), 100);
    assert!(frames.borrow().is_empty());
    assert!(!ui.next_work().frame);
}

struct MountBeforeLayout {
    mounted: bool,
    events: Rc<RefCell<Vec<&'static str>>>,
}
impl Widget for MountBeforeLayout {
    type Command = ();
    type Output = Infallible;
    fn lifecycle(&mut self, _: &mut Update<'_, Self>, event: Lifecycle) {
        if event == Lifecycle::Mount {
            self.mounted = true;
            self.events.borrow_mut().push("mount");
        }
    }
    fn layout(&mut self, cx: &mut Layout<'_>, limits: Constraints) -> Metrics {
        assert!(self.mounted, "layout ran before Mount");
        self.events.borrow_mut().push("layout");
        cx.overlay(limits)
    }
}

#[test]
fn layout_does_not_visit_widgets_waiting_for_mount() {
    let events: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let element = Element::build(|children| {
        children.add(Element::leaf(MountBeforeLayout {
            mounted: false,
            events: events.clone(),
        }));
        MountBeforeLayout {
            mounted: false,
            events: events.clone(),
        }
    });
    let mut ui = Ui::new(element, Size::new(20., 20.), Limits::default()).unwrap();
    ui.layout(&mut TestText);
    assert!(events.borrow().is_empty());
    ui.pump(1, |v| match v {}, |_| {});
    ui.layout(&mut TestText);
    assert_eq!(*events.borrow(), vec!["mount", "layout"]);
    drain(&mut ui);
    ui.layout(&mut TestText);
    assert_eq!(
        *events.borrow(),
        vec!["mount", "layout", "mount", "layout", "layout"]
    );
}

#[test]
fn dropping_the_host_unmounts_children_before_their_owner() {
    use std::{cell::RefCell, convert::Infallible, rc::Rc};
    struct Tracked(&'static str, Rc<RefCell<Vec<&'static str>>>);
    impl Widget for Tracked {
        type Command = ();
        type Output = Infallible;
        fn lifecycle(&mut self, _: &mut Update<'_, Self>, event: Lifecycle) {
            if event == Lifecycle::Unmount {
                self.1.borrow_mut().push(self.0);
            }
        }
    }
    let log = Rc::new(RefCell::new(vec![]));
    let element = Element::build(|children| {
        children.add(Element::leaf(Tracked("child", log.clone())));
        Tracked("owner", log.clone())
    });
    let mut ui = Ui::new(element, Size::new(100., 100.), Limits::default()).unwrap();
    ui.pump(100, |_| {}, |_| {});
    drop(ui);
    assert_eq!(*log.borrow(), ["child", "owner"]);
}
