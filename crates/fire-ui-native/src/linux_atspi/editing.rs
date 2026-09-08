use accesskit_atspi_common::FullNodeId;
use std::{
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use zbus::fdo;

/// AT-SPI positions count Unicode scalar values; Insert's length counts UTF-8 bytes.
#[derive(Debug)]
pub(crate) enum Operation {
    Set(String),
    Insert {
        position: i32,
        text: String,
        length: i32,
    },
    Copy {
        start: i32,
        end: i32,
    },
    Cut {
        start: i32,
        end: i32,
    },
    Delete {
        start: i32,
        end: i32,
    },
    Paste {
        position: i32,
    },
}

/// The UI checks the deadline and current target before performing this operation.
/// Keep this value alive until after replying: dropping it releases the queue slot.
#[derive(Debug)]
pub(crate) struct PendingEdit {
    pub node: FullNodeId,
    pub operation: Operation,
    pub reply: async_channel::Sender<Result<(), String>>,
    pub deadline: Instant,
    outstanding: Arc<AtomicBool>,
}

impl Drop for PendingEdit {
    fn drop(&mut self) {
        self.outstanding.store(false, Ordering::Release);
    }
}

pub(crate) type EditHandler = Arc<dyn Fn(PendingEdit) + Send + Sync>;

#[derive(Clone)]
pub(crate) struct EditDispatcher {
    handler: EditHandler,
    outstanding: Arc<AtomicBool>,
}

impl fmt::Debug for EditDispatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EditDispatcher").finish_non_exhaustive()
    }
}

impl EditDispatcher {
    pub(super) fn new(handler: EditHandler) -> Self {
        Self {
            handler,
            outstanding: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(super) async fn dispatch(&self, node: FullNodeId, operation: Operation) -> fdo::Result<()> {
        self.dispatch_until(node, operation, Instant::now() + Duration::from_secs(5))
            .await
    }

    async fn dispatch_until(
        &self,
        node: FullNodeId,
        operation: Operation,
        deadline: Instant,
    ) -> fdo::Result<()> {
        if self
            .outstanding
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(fdo::Error::LimitsExceeded(
                "an edit is already pending for this window".into(),
            ));
        }
        let (reply, receive) = async_channel::bounded(1);
        (self.handler)(PendingEdit {
            node,
            operation,
            reply,
            deadline,
            outstanding: self.outstanding.clone(),
        });
        futures_lite::future::race(
            async {
                receive
                    .recv()
                    .await
                    .map_err(|_| fdo::Error::Failed("UI is unavailable".into()))?
                    .map_err(fdo::Error::Failed)
            },
            async {
                async_io::Timer::at(deadline).await;
                Err(fdo::Error::TimedOut("UI did not complete the edit".into()))
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{
        ActionHandler, ActionRequest, Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate,
    };
    use accesskit_atspi_common::{Adapter, AdapterCallback, AppContext, Event, WindowBounds};

    struct NoOp;
    impl ActionHandler for NoOp {
        fn do_action(&mut self, _: ActionRequest) {}
    }
    impl AdapterCallback for NoOp {
        fn register_interfaces(&self, _: &Adapter, _: FullNodeId, _: atspi::InterfaceSet) {}
        fn unregister_interfaces(&self, _: &Adapter, _: FullNodeId, _: atspi::InterfaceSet) {}
        fn emit_event(&self, _: &Adapter, _: Event) {}
    }

    fn node_with_id(id: u64) -> FullNodeId {
        let adapter = Adapter::new(
            &AppContext::new(None),
            NoOp,
            TreeUpdate {
                nodes: vec![(NodeId(id), Node::new(Role::Window))],
                tree: Some(TreeInfo::new(NodeId(id))),
                tree_id: TreeId::ROOT,
                focus: NodeId(id),
            },
            false,
            WindowBounds::default(),
            NoOp,
        );
        adapter.root_id()
    }

    fn node() -> FullNodeId {
        node_with_id(42)
    }

    #[test]
    fn request_waits_for_ui_result_and_bounds_concurrent_edits() {
        futures_lite::future::block_on(async {
            let (send, receive) = async_channel::bounded(1);
            let dispatcher = EditDispatcher::new(Arc::new(move |request| {
                send.try_send(request).unwrap();
            }));
            let id = node();
            // The pinned AccessKit common's public u128 conversion encodes the
            // local node in the high word and the root-tree index (zero) below.
            assert_eq!(u128::from(id), 42_u128 << 64);
            let first = dispatcher.dispatch(id, Operation::Set("first".into()));
            let ui = async {
                let pending = receive.recv().await.unwrap();
                assert!(matches!(
                    dispatcher
                        .dispatch(id, Operation::Paste { position: 0 })
                        .await,
                    Err(fdo::Error::LimitsExceeded(_))
                ));
                pending
                    .reply
                    .try_send(Err("editor is disabled".into()))
                    .unwrap();
            };
            let (result, ()) = futures_util::future::join(first, ui).await;
            assert!(
                matches!(result, Err(fdo::Error::Failed(message)) if message == "editor is disabled")
            );
            assert!(!dispatcher.outstanding.load(Ordering::Acquire));
        });
    }

    #[test]
    fn timeout_keeps_queue_slot_until_ui_discards_expired_request() {
        futures_lite::future::block_on(async {
            let (send, receive) = async_channel::bounded(1);
            let dispatcher = EditDispatcher::new(Arc::new(move |request| {
                send.try_send(request).unwrap();
            }));
            let id = node();
            let result = dispatcher
                .dispatch_until(
                    id,
                    Operation::Set("expired".into()),
                    Instant::now() + Duration::from_millis(1),
                )
                .await;
            assert!(matches!(result, Err(fdo::Error::TimedOut(_))));
            let pending = receive.recv().await.unwrap();
            assert!(pending.deadline <= Instant::now());
            assert!(pending.reply.is_closed());
            assert!(matches!(
                dispatcher
                    .dispatch(id, Operation::Paste { position: 0 })
                    .await,
                Err(fdo::Error::LimitsExceeded(_))
            ));
            drop(pending);
            assert!(!dispatcher.outstanding.load(Ordering::Acquire));
        });
    }

    #[test]
    fn unavailable_ui_closes_request_without_waiting_for_timeout() {
        let dispatcher = EditDispatcher::new(Arc::new(drop));
        let result = futures_lite::future::block_on(
            dispatcher.dispatch(node(), Operation::Set("ignored".into())),
        );
        assert!(matches!(result, Err(fdo::Error::Failed(_))));
        assert!(!dispatcher.outstanding.load(Ordering::Acquire));
    }
    #[test]
    fn failed_clipboard_copy_keeps_native_cut_text_and_selection() {
        use fire_ui::{Element, Limits, SemanticAction, Size, TestText, Ui};
        use fire_ui_widgets::Editor;
        let mut ui = Ui::new(
            Element::leaf(Editor::new("aé🔥z")),
            Size::new(300., 140.),
            Limits::default(),
        )
        .unwrap();
        ui.pump(100, |_| {}, |_| {});
        ui.layout(&mut TestText);
        let id = ui.semantics()[0].id;
        ui.accessibility(
            id,
            SemanticAction::SetSelection {
                anchor: 7,
                caret: 1,
            },
        )
        .unwrap();
        let (reply, _receive) = async_channel::bounded(1);
        let pending = PendingEdit {
            node: node_with_id(id),
            operation: Operation::Cut { start: 1, end: 3 },
            reply,
            deadline: Instant::now() + Duration::from_secs(5),
            outstanding: Arc::new(AtomicBool::new(true)),
        };
        let result = crate::linux_edit::apply(&mut ui, &pending, |value| {
            assert_eq!(value, "é🔥");
            Err("native clipboard feature is disabled".into())
        });
        assert_eq!(result, Err("native clipboard feature is disabled".into()));
        assert_eq!(ui.root().text(), "aé🔥z");
        let text = ui.semantics()[0].semantics.text.clone().unwrap();
        assert_eq!((text.anchor, text.caret.byte), (7, 1));
    }
}
