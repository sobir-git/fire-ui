//! A handle returned by `insert` is usable in the callback that created it.
use fire_ui::*;
use std::{cell::RefCell, convert::Infallible, rc::Rc};

type Log = Rc<RefCell<Vec<String>>>;

struct Leaf {
    log: Log,
}
impl Widget for Leaf {
    type Command = u32;
    type Output = Infallible;
    fn update(&mut self, _: &mut Update<'_, Self>, command: u32) {
        self.log.borrow_mut().push(format!("command {command}"));
    }
    fn lifecycle(&mut self, _: &mut Update<'_, Self>, event: Lifecycle) {
        if event == Lifecycle::Mount {
            self.log.borrow_mut().push("mount".into());
        }
    }
    fn focusable(&self) -> bool {
        true
    }
}

#[derive(Debug)]
enum Command {
    InsertSendFocus,
    InsertRemove,
}
impl Data for Command {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}
struct Owner {
    log: Log,
    child: Option<Child<Leaf>>,
    on_mount: bool,
}
impl Owner {
    fn insert(&mut self, cx: &mut Update<'_, Self>) -> Child<Leaf> {
        let leaf = Element::leaf(Leaf {
            log: self.log.clone(),
        });
        let child = cx.insert(leaf, |v| match *v {}).ok().unwrap();
        self.child = Some(child);
        child
    }
}
impl Widget for Owner {
    type Command = Command;
    type Output = Infallible;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: Command) {
        let child = self.insert(cx);
        match command {
            Command::InsertSendFocus => {
                cx.send(child, 7).unwrap();
                cx.focus_child(child).unwrap();
            }
            Command::InsertRemove => cx.remove(child).unwrap(),
        }
    }
    fn lifecycle(&mut self, cx: &mut Update<'_, Self>, event: Lifecycle) {
        // Lifecycle callbacks defer their mutations; the handle must still work.
        if event == Lifecycle::Mount && self.on_mount {
            let child = self.insert(cx);
            cx.send(child, 9).unwrap();
        }
    }
}

fn ui(on_mount: bool) -> (Ui<Owner>, Log) {
    let log = Log::default();
    let mut ui = Ui::new(
        Element::leaf(Owner {
            log: log.clone(),
            child: None,
            on_mount,
        }),
        Size::new(100., 100.),
        Limits::default(),
    )
    .unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    (ui, log)
}

#[test]
fn a_new_child_mounts_before_it_receives_commands_and_focus() {
    let (mut ui, log) = ui(false);
    ui.send(Command::InsertSendFocus).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    assert_eq!(*log.borrow(), ["mount", "command 7"]);
    assert!(ui.focused(ui.root().child.unwrap()));
    assert_eq!(ui.stats().stale, 0);
}

#[test]
fn a_child_inserted_during_a_lifecycle_notice_receives_its_command() {
    let (ui, log) = ui(true);
    assert_eq!(*log.borrow(), ["mount", "command 9"]);
    assert_eq!(ui.stats().stale, 0);
}

#[test]
fn a_child_removed_in_its_inserting_callback_mounts_then_retires() {
    let (mut ui, log) = ui(false);
    ui.send(Command::InsertRemove).unwrap();
    ui.pump(100, |v| match v {}, |_| {});
    assert_eq!(*log.borrow(), ["mount"]);
    assert_eq!(ui.stats().nodes, 1);
    assert_eq!(ui.stats().queued, 0);
}
