// Copyright 2024 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

use crate::linux_atspi::{editing::EditDispatcher, text::TextSnapshot, Operation};
use accesskit_atspi_common::{PlatformNode, Rect};
use atspi::{CoordType, Granularity, ScrollType};
use std::collections::HashMap;
use zbus::{fdo, interface};

pub(crate) struct TextInterface {
    node: PlatformNode,
    edits: EditDispatcher,
}

impl TextInterface {
    pub fn new(node: PlatformNode, edits: EditDispatcher) -> Self {
        Self { node, edits }
    }

    // Await only lock acquisition. Geometry lookup may read the common tree,
    // so keep snapshot -> common-tree ordering and finish before UI dispatch.
    async fn query<T>(
        &self,
        query: impl FnOnce(&TextSnapshot) -> fdo::Result<T>,
    ) -> fdo::Result<T> {
        let snapshots = self.edits.text.read().await;
        let full = u128::from(self.node.id());
        let snapshot = snapshots
            .get(&((full >> 64) as u64))
            .ok_or_else(|| fdo::Error::UnknownObject("text node is no longer available".into()))?;
        query(snapshot)
    }
    fn coordinates(
        &self,
        s: &TextSnapshot,
        r: fire_ui::Rect,
        kind: CoordType,
    ) -> fdo::Result<Rect> {
        let editor = self.node.extents(kind).map_err(self.map_error())?;
        let r = s.window_rect(r);
        let scale = s.scale;
        Ok(Rect {
            x: ((r.x - s.bounds.x) as f64 * scale) as i32 + editor.x,
            y: ((r.y - s.bounds.y) as f64 * scale) as i32 + editor.y,
            width: (r.width as f64 * scale) as i32,
            height: (r.height as f64 * scale) as i32,
        })
    }
    async fn select(&self, anchor: i32, caret: i32, require_empty: bool) -> fdo::Result<bool> {
        self.query(|s| {
            s.byte(anchor)?;
            s.byte(caret)?;
            Ok(())
        })
        .await?;
        self.edits
            .dispatch(
                self.node.id(),
                Operation::Select {
                    anchor,
                    caret,
                    require_empty,
                },
            )
            .await?;
        Ok(true)
    }
    fn local_delta(s: &TextSnapshot, x: f32, y: f32) -> fdo::Result<fire_ui::Point> {
        let inverse = s
            .transform
            .inverse()
            .ok_or_else(|| fdo::Error::Failed("noninvertible text transform".into()))?;
        let zero = inverse.point(fire_ui::Point::new(0., 0.));
        let point = inverse.point(fire_ui::Point::new(x, y));
        Ok(fire_ui::Point::new(point.x - zero.x, point.y - zero.y))
    }
    fn map_error(&self) -> impl '_ + FnOnce(accesskit_atspi_common::Error) -> fdo::Error {
        |error| crate::linux_atspi::util::map_error_from_node(&self.node, error)
    }
}

#[interface(name = "org.a11y.atspi.Text")]
impl TextInterface {
    #[zbus(property)]
    async fn character_count(&self) -> fdo::Result<i32> {
        self.query(|s| Ok(s.scalar(s.paragraph.text.len()))).await
    }

    #[zbus(property)]
    async fn caret_offset(&self) -> fdo::Result<i32> {
        self.query(|s| Ok(s.scalar(s.caret.byte))).await
    }

    async fn get_string_at_offset(
        &self,
        offset: i32,
        granularity: Granularity,
    ) -> fdo::Result<(String, i32, i32)> {
        self.query(|s| {
            let range = s.text_range(offset, granularity)?;
            Ok((
                s.paragraph.text[range.clone()].into(),
                s.scalar(range.start),
                s.scalar(range.end),
            ))
        })
        .await
    }

    async fn get_text(&self, start_offset: i32, end_offset: i32) -> fdo::Result<String> {
        self.query(|s| Ok(s.paragraph.text[s.range(start_offset, end_offset)?].into()))
            .await
    }

    async fn set_caret_offset(&self, offset: i32) -> fdo::Result<bool> {
        self.select(offset, offset, false).await
    }

    async fn get_attribute_value(&self, offset: i32, attribute_name: &str) -> fdo::Result<String> {
        self.query(|s| {
            s.byte(offset)?;
            self.node
                .text_attribute_value(offset, attribute_name)
                .map_err(self.map_error())
        })
        .await
    }

    async fn get_attributes(
        &self,
        offset: i32,
    ) -> fdo::Result<(HashMap<&'static str, String>, i32, i32)> {
        self.query(|s| {
            s.byte(offset)?;
            self.node.text_attributes(offset).map_err(self.map_error())
        })
        .await
    }

    async fn get_default_attributes(&self) -> fdo::Result<HashMap<&'static str, String>> {
        self.query(|_| {
            self.node
                .default_text_attributes()
                .map_err(self.map_error())
        })
        .await
    }

    async fn get_character_extents(&self, offset: i32, coord_type: CoordType) -> fdo::Result<Rect> {
        self.query(|s| self.coordinates(s, s.character_rect(s.byte(offset)?), coord_type))
            .await
    }

    async fn get_offset_at_point(&self, x: i32, y: i32, coord_type: CoordType) -> fdo::Result<i32> {
        self.query(|s| {
            let editor = self.node.extents(coord_type).map_err(self.map_error())?;
            let world = fire_ui::Point::new(
                (x - editor.x) as f32 / s.scale as f32 + s.bounds.x,
                (y - editor.y) as f32 / s.scale as f32 + s.bounds.y,
            );
            let local = s
                .transform
                .inverse()
                .ok_or_else(|| fdo::Error::Failed("noninvertible text transform".into()))?
                .point(world);
            let point = fire_ui::Point::new(local.x - s.origin.x, local.y - s.origin.y);
            Ok(s.scalar(s.paragraph.hit(point).byte))
        })
        .await
    }

    async fn get_n_selections(&self) -> fdo::Result<i32> {
        self.query(|s| Ok(i32::from(s.anchor != s.caret.byte)))
            .await
    }

    async fn get_selection(&self, selection_num: i32) -> fdo::Result<(i32, i32)> {
        self.query(|s| {
            if selection_num != 0 || s.anchor == s.caret.byte {
                return Err(fdo::Error::InvalidArgs("selection unavailable".into()));
            }
            Ok((
                s.scalar(s.anchor.min(s.caret.byte)),
                s.scalar(s.anchor.max(s.caret.byte)),
            ))
        })
        .await
    }

    async fn add_selection(&self, start_offset: i32, end_offset: i32) -> fdo::Result<bool> {
        if self.get_n_selections().await? != 0 {
            return Ok(false);
        }
        self.select(start_offset, end_offset, true).await
    }

    async fn remove_selection(&self, selection_num: i32) -> fdo::Result<bool> {
        if selection_num != 0 || self.get_n_selections().await? == 0 {
            return Ok(false);
        }
        self.edits
            .dispatch(self.node.id(), Operation::ClearSelection)
            .await?;
        Ok(true)
    }

    async fn set_selection(
        &self,
        selection_num: i32,
        start_offset: i32,
        end_offset: i32,
    ) -> fdo::Result<bool> {
        if selection_num != 0 {
            return Ok(false);
        }
        self.select(start_offset, end_offset, false).await
    }

    async fn get_range_extents(
        &self,
        start_offset: i32,
        end_offset: i32,
        coord_type: CoordType,
    ) -> fdo::Result<Rect> {
        self.query(|s| {
            self.coordinates(
                s,
                s.range_rect(s.range(start_offset, end_offset)?),
                coord_type,
            )
        })
        .await
    }

    async fn get_attribute_run(
        &self,
        offset: i32,
        include_defaults: bool,
    ) -> fdo::Result<(HashMap<&'static str, String>, i32, i32)> {
        self.query(|s| {
            s.byte(offset)?;
            self.node
                .text_attribute_run(offset, include_defaults)
                .map_err(self.map_error())
        })
        .await
    }

    async fn scroll_substring_to(
        &self,
        start_offset: i32,
        end_offset: i32,
        scroll_type: ScrollType,
    ) -> fdo::Result<bool> {
        let delta = self
            .query(|s| {
                let rect = s.window_rect(s.range_rect(s.range(start_offset, end_offset)?));
                let viewport = s.bounds;
                let near = |start: f32, length: f32, edge: f32, extent: f32| {
                    if start < edge {
                        edge - start
                    } else if start + length > edge + extent {
                        edge + extent - start - length
                    } else {
                        0.
                    }
                };
                let left = viewport.x - rect.x;
                let top = viewport.y - rect.y;
                let right = viewport.x + viewport.width - rect.x - rect.width;
                let bottom = viewport.y + viewport.height - rect.y - rect.height;
                let (x, y) = match scroll_type {
                    ScrollType::TopLeft => (left, top),
                    ScrollType::BottomRight => (right, bottom),
                    ScrollType::TopEdge => (0., top),
                    ScrollType::BottomEdge => (0., bottom),
                    ScrollType::LeftEdge => (left, 0.),
                    ScrollType::RightEdge => (right, 0.),
                    ScrollType::Anywhere => (
                        near(rect.x, rect.width, viewport.x, viewport.width),
                        near(rect.y, rect.height, viewport.y, viewport.height),
                    ),
                };
                Self::local_delta(s, x, y)
            })
            .await?;
        self.edits
            .dispatch(
                self.node.id(),
                Operation::Scroll {
                    x: delta.x,
                    y: delta.y,
                },
            )
            .await?;
        Ok(true)
    }

    async fn scroll_substring_to_point(
        &self,
        start_offset: i32,
        end_offset: i32,
        coord_type: CoordType,
        x: i32,
        y: i32,
    ) -> fdo::Result<bool> {
        let delta = self
            .query(|s| {
                let rect = self.coordinates(
                    s,
                    s.range_rect(s.range(start_offset, end_offset)?),
                    coord_type,
                )?;
                Self::local_delta(
                    s,
                    (x as f64 - rect.x as f64) as f32 / s.scale as f32,
                    (y as f64 - rect.y as f64) as f32 / s.scale as f32,
                )
            })
            .await?;
        self.edits
            .dispatch(
                self.node.id(),
                Operation::Scroll {
                    x: delta.x,
                    y: delta.y,
                },
            )
            .await?;
        Ok(true)
    }
}

#[cfg(test)]
mod transaction_query_tests {
    use super::*;
    use accesskit::{
        ActionHandler, ActionRequest, Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate,
    };
    use accesskit_atspi_common::{
        Adapter, AdapterCallback, AppContext, Event, FullNodeId, WindowBounds,
    };
    use fire_ui::{Element, Limits, Size, TestText, Ui};
    use fire_ui_widgets::Editor;
    use std::sync::Arc;
    struct NoOp;
    impl ActionHandler for NoOp {
        fn do_action(&mut self, _: ActionRequest) {}
    }
    impl AdapterCallback for NoOp {
        fn register_interfaces(&self, _: &Adapter, _: FullNodeId, _: atspi::InterfaceSet) {}
        fn unregister_interfaces(&self, _: &Adapter, _: FullNodeId, _: atspi::InterfaceSet) {}
        fn emit_event(&self, _: &Adapter, _: Event) {}
    }
    fn interface(value: &str) -> (Adapter, TextInterface) {
        let adapter = Adapter::new(
            &AppContext::new(None),
            NoOp,
            TreeUpdate {
                nodes: vec![(NodeId(1), Node::new(Role::Window))],
                tree: Some(TreeInfo::new(NodeId(1))),
                tree_id: TreeId::ROOT,
                focus: NodeId(1),
            },
            false,
            WindowBounds::default(),
            NoOp,
        );
        let edits = EditDispatcher::new(Arc::new(drop));
        let mut ui = Ui::new(
            Element::leaf(Editor::new(value)),
            Size::new(300., 140.),
            Limits::default(),
        )
        .unwrap();
        ui.pump(100, |_| {}, |_| {});
        ui.layout(&mut TestText);
        let snapshot = crate::linux_atspi::text::snapshots(&ui.semantics(), 1.)
            .into_values()
            .next()
            .unwrap();
        edits.text.write_blocking().insert(1, snapshot);
        let interface = TextInterface::new(adapter.platform_node(adapter.root_id()), edits);
        (adapter, interface)
    }
    #[test]
    fn one_windows_layout_does_not_block_another_windows_text_query() {
        let (_first_adapter, first) = interface("first");
        let (_second_adapter, second) = interface("second");
        let mut transaction = first.edits.text.write_blocking();
        let mut snapshot = transaction.remove(&1).unwrap();
        snapshot.anchor = 2;
        futures_lite::future::block_on(async {
            let pending = first.get_text(0, -1);
            futures_util::pin_mut!(pending);
            assert!(futures_lite::future::poll_once(pending.as_mut())
                .await
                .is_none());
            assert_eq!(second.get_text(0, -1).await.unwrap(), "second");
            transaction.insert(1, snapshot);
            drop(transaction);
            assert_eq!(pending.await.unwrap(), "first");
        });
    }
}
