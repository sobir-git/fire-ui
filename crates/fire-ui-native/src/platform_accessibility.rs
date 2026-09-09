//! Window integration is platform-specific; semantic trees and actions are shared.
#[cfg(not(target_os = "linux"))]
pub(crate) use accesskit_winit::Adapter;

#[cfg(target_os = "linux")]
pub(crate) use linux::Adapter;

#[cfg(target_os = "linux")]
mod linux {
    use crate::linux_atspi::{self, PendingEdit};
    use accesskit::{
        ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, Rect, TreeUpdate,
    };
    use accesskit_winit::{Event, WindowEvent as AccessibilityEvent};
    use std::sync::Arc;
    use winit::{
        event::WindowEvent,
        event_loop::{ActiveEventLoop, EventLoopProxy},
        window::{Window, WindowId},
    };

    struct Handler<T: From<Event> + Send + 'static> {
        window: WindowId,
        proxy: EventLoopProxy<T>,
    }
    impl<T: From<Event> + Send + 'static> Handler<T> {
        fn send(&self, window_event: AccessibilityEvent) {
            let _ = self.proxy.send_event(
                Event {
                    window_id: self.window,
                    window_event,
                }
                .into(),
            );
        }
    }
    impl<T: From<Event> + Send + 'static> ActivationHandler for Handler<T> {
        fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
            self.send(AccessibilityEvent::InitialTreeRequested);
            None
        }
    }
    impl<T: From<Event> + Send + 'static> ActionHandler for Handler<T> {
        fn do_action(&mut self, request: ActionRequest) {
            self.send(AccessibilityEvent::ActionRequested(request));
        }
    }
    impl<T: From<Event> + Send + 'static> DeactivationHandler for Handler<T> {
        fn deactivate_accessibility(&mut self) {
            self.send(AccessibilityEvent::AccessibilityDeactivated);
        }
    }
    pub(crate) struct Adapter(linux_atspi::Adapter);
    impl Adapter {
        pub(crate) fn with_event_loop_proxy<T: From<Event> + From<PendingEdit> + Send + 'static>(
            _event_loop: &ActiveEventLoop,
            window: &Window,
            proxy: EventLoopProxy<T>,
        ) -> Self {
            let handler = || Handler {
                window: window.id(),
                proxy: proxy.clone(),
            };
            let activation = handler();
            let action = handler();
            let deactivation = handler();
            let mut adapter = Self(linux_atspi::Adapter::new(
                activation,
                action,
                deactivation,
                Arc::new(move |pending| {
                    let _ = proxy.send_event(pending.into());
                }),
            ));
            adapter.bounds(window);
            adapter.0.update_window_focus_state(window.has_focus());
            adapter
        }
        pub(crate) fn synchronize(
            &mut self,
            release_snapshots: bool,
            update: impl FnOnce(bool) -> Option<(TreeUpdate, crate::linux_atspi::text::TextSnapshots)>,
        ) {
            self.0.synchronize(release_snapshots, update);
        }
        fn bounds(&mut self, window: &Window) {
            let outer: (_, _) = window
                .outer_position()
                .unwrap_or_default()
                .cast::<f64>()
                .into();
            let inner: (_, _) = window
                .inner_position()
                .unwrap_or_default()
                .cast::<f64>()
                .into();
            self.0.set_root_window_bounds(
                Rect::from_origin_size(
                    outer,
                    <(f64, f64)>::from(window.outer_size().cast::<f64>()),
                ),
                Rect::from_origin_size(
                    inner,
                    <(f64, f64)>::from(window.inner_size().cast::<f64>()),
                ),
            );
        }
        pub(crate) fn process_event(&mut self, window: &Window, event: &WindowEvent) {
            match event {
                WindowEvent::Moved(_)
                | WindowEvent::Resized(_)
                | WindowEvent::ScaleFactorChanged { .. } => self.bounds(window),
                WindowEvent::Focused(focused) => self.0.update_window_focus_state(*focused),
                _ => {}
            }
        }
    }
}
