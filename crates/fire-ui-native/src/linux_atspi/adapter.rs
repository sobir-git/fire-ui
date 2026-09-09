// Copyright 2022 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

use accesskit::{ActionHandler, ActivationHandler, DeactivationHandler, Rect, TreeUpdate};
use accesskit_atspi_common::{
    next_adapter_id, ActionHandlerNoMut, ActionHandlerWrapper, Adapter as AdapterImpl,
    AdapterCallback, CacheEvent, Event, FullNodeId, PlatformNode, WindowBounds,
};

use async_channel::Sender;
use async_lock::Mutex;
use atspi::InterfaceSet;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

use crate::linux_atspi::{
    context::{get_or_init_app_context, get_or_init_messages},
    editing::EditDispatcher,
    EditHandler,
};

pub(crate) struct Callback {
    messages: Sender<Message>,
    edits: EditDispatcher,
}

impl Callback {
    pub(crate) fn new(edits: EditDispatcher) -> Self {
        let messages = get_or_init_messages();
        Self { messages, edits }
    }

    fn send_message(&self, message: Message) {
        let _ = self.messages.try_send(message);
    }
}

impl AdapterCallback for Callback {
    fn register_interfaces(&self, adapter: &AdapterImpl, id: FullNodeId, interfaces: InterfaceSet) {
        let node = adapter.platform_node(id);
        self.send_message(Message::RegisterInterfaces {
            node,
            interfaces,
            edits: self.edits.clone(),
        });
    }

    fn unregister_interfaces(
        &self,
        adapter: &AdapterImpl,
        id: FullNodeId,
        interfaces: InterfaceSet,
    ) {
        self.send_message(Message::UnregisterInterfaces {
            adapter_id: adapter.id(),
            node_id: id,
            interfaces,
        })
    }

    fn emit_event(&self, adapter: &AdapterImpl, event: Event) {
        match event {
            Event::Cache(CacheEvent::Added(id)) => {
                let node = adapter.platform_node(id);
                self.send_message(Message::EmitCacheAdd { node });
            }
            Event::Cache(CacheEvent::Removed(id)) => {
                self.send_message(Message::EmitCacheRemove {
                    adapter_id: adapter.id(),
                    node_id: id,
                });
            }
            event => {
                self.send_message(Message::EmitEvent {
                    adapter_id: adapter.id(),
                    event,
                });
            }
        }
    }
}

pub(crate) enum AdapterState {
    Inactive {
        is_window_focused: bool,
        root_window_bounds: WindowBounds,
        action_handler: Arc<dyn ActionHandlerNoMut + Send + Sync>,
    },
    Pending {
        is_window_focused: bool,
        root_window_bounds: WindowBounds,
        action_handler: Arc<dyn ActionHandlerNoMut + Send + Sync>,
    },
    Active(AdapterImpl),
}

impl Debug for AdapterState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AdapterState::Inactive {
                is_window_focused,
                root_window_bounds,
                action_handler: _,
            } => f
                .debug_struct("Inactive")
                .field("is_window_focused", is_window_focused)
                .field("root_window_bounds", root_window_bounds)
                .field("action_handler", &"ActionHandler")
                .finish(),
            AdapterState::Pending {
                is_window_focused,
                root_window_bounds,
                action_handler: _,
            } => f
                .debug_struct("Pending")
                .field("is_window_focused", is_window_focused)
                .field("root_window_bounds", root_window_bounds)
                .field("action_handler", &"ActionHandler")
                .finish(),
            AdapterState::Active(r#impl) => f.debug_tuple("Active").field(r#impl).finish(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Adapter {
    messages: Sender<Message>,
    edits: EditDispatcher,
    id: usize,
    state: Arc<Mutex<AdapterState>>,
}

impl Adapter {
    /// Create a new Unix adapter.
    ///
    /// All of the handlers will always be called from another thread.
    pub fn new(
        activation_handler: impl 'static + ActivationHandler + Send,
        action_handler: impl 'static + ActionHandler + Send,
        deactivation_handler: impl 'static + DeactivationHandler + Send,
        edit_handler: EditHandler,
    ) -> Self {
        let id = next_adapter_id();
        let messages = get_or_init_messages();
        let state = Arc::new(Mutex::new(AdapterState::Inactive {
            is_window_focused: false,
            root_window_bounds: Default::default(),
            action_handler: Arc::new(ActionHandlerWrapper::new(action_handler)),
        }));
        let edits = EditDispatcher::new(edit_handler);
        let adapter = Self {
            edits: edits.clone(),
            id,
            messages,
            state: Arc::clone(&state),
        };
        adapter.send_message(Message::AddAdapter {
            id,
            activation_handler: Box::new(activation_handler),
            deactivation_handler: Box::new(deactivation_handler),
            state,
            edits,
        });
        adapter
    }

    pub(crate) fn send_message(&self, message: Message) {
        let _ = self.messages.try_send(message);
    }

    /// Set the bounds of the top-level window. The outer bounds contain any
    /// window decoration and borders.
    ///
    /// # Caveats
    ///
    /// Since an application can not get the position of its window under
    /// Wayland, calling this method only makes sense under X11.
    pub fn set_root_window_bounds(&mut self, outer: Rect, inner: Rect) {
        let new_bounds = WindowBounds::new(outer, inner);
        let mut state = self.state.lock_blocking();
        match &mut *state {
            AdapterState::Inactive {
                root_window_bounds, ..
            } => {
                *root_window_bounds = new_bounds;
            }
            AdapterState::Pending {
                root_window_bounds, ..
            } => {
                *root_window_bounds = new_bounds;
            }
            AdapterState::Active(r#impl) => r#impl.set_root_window_bounds(new_bounds),
        }
    }

    /// Run the UI update even when accessibility is inactive. The factory gets
    /// the activation state and returns a complete update only when needed.
    ///
    /// Before layout, release the old paragraph snapshots under the publication
    /// lock. Active readers see either the previous or the replacement snapshot,
    /// never the empty map. Releasing snapshots requires a replacement update.
    ///
    /// Lock order is adapter state, text snapshots, then the common tree. Native
    /// UI calls block; accessibility tasks await these locks so another window's
    /// queries can run while layout is in progress. Query closures must not await
    /// after acquiring the snapshot read guard or dispatch work to the UI.
    pub fn synchronize(
        &mut self,
        release_snapshots: bool,
        update_factory: impl FnOnce(bool) -> Option<(TreeUpdate, super::text::TextSnapshots)>,
    ) {
        let mut state = self.state.lock_blocking();
        if matches!(*state, AdapterState::Inactive { .. }) {
            update_factory(false);
            return;
        }
        let mut current_text = self.edits.text.write_blocking();
        if release_snapshots {
            current_text.clear();
        }
        let Some((update, text)) = update_factory(true) else {
            assert!(!release_snapshots, "released snapshots must be replaced");
            return;
        };
        *current_text = text;
        match &mut *state {
            AdapterState::Inactive { .. } => unreachable!(),
            AdapterState::Pending {
                is_window_focused,
                root_window_bounds,
                action_handler,
            } => {
                let adapter = AdapterImpl::with_wrapped_action_handler(
                    self.id,
                    get_or_init_app_context(),
                    Callback::new(self.edits.clone()),
                    update,
                    *is_window_focused,
                    *root_window_bounds,
                    Arc::clone(action_handler),
                );
                *state = AdapterState::Active(adapter);
            }
            AdapterState::Active(adapter) => adapter.update(update),
        }
    }

    /// Update the tree state based on whether the window is focused.
    pub fn update_window_focus_state(&mut self, is_focused: bool) {
        let mut state = self.state.lock_blocking();
        match &mut *state {
            AdapterState::Inactive {
                is_window_focused, ..
            } => {
                *is_window_focused = is_focused;
            }
            AdapterState::Pending {
                is_window_focused, ..
            } => {
                *is_window_focused = is_focused;
            }
            AdapterState::Active(r#impl) => r#impl.update_window_focus_state(is_focused),
        }
    }
}

impl Drop for Adapter {
    fn drop(&mut self) {
        self.edits.text.write_blocking().clear();
        self.send_message(Message::RemoveAdapter { id: self.id });
    }
}

pub(crate) enum Message {
    AddAdapter {
        id: usize,
        activation_handler: Box<dyn ActivationHandler + Send>,
        deactivation_handler: Box<dyn DeactivationHandler + Send>,
        state: Arc<Mutex<AdapterState>>,
        edits: EditDispatcher,
    },
    RemoveAdapter {
        id: usize,
    },
    RegisterInterfaces {
        edits: EditDispatcher,
        node: PlatformNode,
        interfaces: InterfaceSet,
    },
    UnregisterInterfaces {
        adapter_id: usize,
        node_id: FullNodeId,
        interfaces: InterfaceSet,
    },
    EmitEvent {
        adapter_id: usize,
        event: Event,
    },
    EmitCacheAdd {
        node: PlatformNode,
    },
    EmitCacheRemove {
        adapter_id: usize,
        node_id: FullNodeId,
    },
}

#[cfg(test)]
mod transaction_review_tests {
    use super::*;
    use accesskit::{ActionRequest, Node, NodeId, Role, TreeId, TreeInfo};
    use fire_ui::{Element, Limits, Size, TestText, Ui};
    use fire_ui_widgets::Editor;
    use std::sync::mpsc;

    struct NoOp;
    impl ActionHandler for NoOp {
        fn do_action(&mut self, _: ActionRequest) {}
    }
    impl AdapterCallback for NoOp {
        fn register_interfaces(&self, _: &AdapterImpl, _: FullNodeId, _: InterfaceSet) {}
        fn unregister_interfaces(&self, _: &AdapterImpl, _: FullNodeId, _: InterfaceSet) {}
        fn emit_event(&self, _: &AdapterImpl, _: Event) {}
    }
    fn tree() -> TreeUpdate {
        TreeUpdate {
            nodes: vec![(NodeId(1), Node::new(Role::Window))],
            tree: Some(TreeInfo::new(NodeId(1))),
            tree_id: TreeId::ROOT,
            focus: NodeId(1),
        }
    }
    fn text(value: &str) -> super::super::text::TextSnapshots {
        let mut ui = Ui::new(
            Element::leaf(Editor::new(value)),
            Size::new(300., 140.),
            Limits::default(),
        )
        .unwrap();
        ui.pump(100, |_| {}, |_| {});
        ui.layout(&mut TestText);
        super::super::text::snapshots(&ui.semantics(), 1.)
    }
    #[test]
    fn inactive_synchronization_still_runs_the_ui_factory() {
        let (messages, _receive) = async_channel::unbounded();
        let mut adapter = Adapter {
            messages,
            edits: EditDispatcher::new(Arc::new(drop)),
            id: next_adapter_id(),
            state: Arc::new(Mutex::new(AdapterState::Inactive {
                is_window_focused: false,
                root_window_bounds: WindowBounds::default(),
                action_handler: Arc::new(ActionHandlerWrapper::new(NoOp)),
            })),
        };
        let mut calls = 0;
        adapter.synchronize(true, |active| {
            assert!(!active);
            calls += 1;
            None
        });
        assert_eq!(calls, 1);
    }

    #[test]
    fn transaction_drops_old_paragraph_and_blocks_readers_until_publication() {
        let edits = EditDispatcher::new(Arc::new(drop));
        *edits.text.write_blocking() = text("old paragraph");
        let old = Arc::downgrade(
            &edits
                .text
                .read_blocking()
                .values()
                .next()
                .unwrap()
                .paragraph,
        );
        let common = AdapterImpl::new(
            &accesskit_atspi_common::AppContext::new(None),
            NoOp,
            tree(),
            false,
            WindowBounds::default(),
            NoOp,
        );
        let (messages, _receive) = async_channel::unbounded();
        let mut adapter = Adapter {
            messages,
            edits: edits.clone(),
            id: next_adapter_id(),
            state: Arc::new(Mutex::new(AdapterState::Active(common))),
        };
        let (start, started) = mpsc::channel();
        let (attempt, attempted) = mpsc::channel();
        let (result, received) = mpsc::channel();
        std::thread::scope(|scope| {
            let reader_edits = edits.clone();
            scope.spawn(move || {
                started.recv().unwrap();
                attempt.send(()).unwrap();
                let store = reader_edits.text.read_blocking();
                result
                    .send(store.values().next().unwrap().paragraph.text.to_string())
                    .unwrap();
            });
            adapter.synchronize(true, |active| {
                assert!(active);
                assert!(
                    old.upgrade().is_none(),
                    "old snapshot must be released before layout"
                );
                assert!(
                    edits.text.try_read().is_none(),
                    "publication lock must remain held"
                );
                start.send(()).unwrap();
                attempted.recv().unwrap();
                assert!(received.try_recv().is_err());
                Some((tree(), text("new paragraph")))
            });
            assert_eq!(received.recv().unwrap(), "new paragraph");
        });
    }
}
