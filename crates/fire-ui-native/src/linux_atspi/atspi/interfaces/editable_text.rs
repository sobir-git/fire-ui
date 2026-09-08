// Copyright 2026 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

use accesskit_atspi_common::PlatformNode;
use zbus::{fdo, interface};

use crate::linux_atspi::{editing::EditDispatcher, Operation};

pub(crate) struct EditableTextInterface {
    node: PlatformNode,
    edits: EditDispatcher,
}

impl EditableTextInterface {
    pub fn new(node: PlatformNode, edits: EditDispatcher) -> Self {
        Self { node, edits }
    }

    fn map_error(&self) -> impl '_ + FnOnce(accesskit_atspi_common::Error) -> fdo::Error {
        |error| crate::linux_atspi::util::map_error_from_node(&self.node, error)
    }

    async fn edit(&self, operation: Operation) -> fdo::Result<bool> {
        // Resolve before enqueueing, then validate against the live UI again when
        // executing. No stale PlatformNode or asynchronous clipboard callback can
        // revive a removed or disabled editor.
        if !self
            .node
            .supports_editable_text()
            .map_err(self.map_error())?
        {
            return Ok(false);
        }
        self.edits.dispatch(self.node.id(), operation).await?;
        Ok(true)
    }

    fn check_text(text: &str) -> fdo::Result<()> {
        if text.len() > 1_048_576 {
            Err(fdo::Error::LimitsExceeded(
                "editable text request exceeds 1 MiB".into(),
            ))
        } else {
            Ok(())
        }
    }
}

#[interface(name = "org.a11y.atspi.EditableText")]
impl EditableTextInterface {
    async fn set_text_contents(&self, new_contents: &str) -> fdo::Result<bool> {
        Self::check_text(new_contents)?;
        self.edit(Operation::Set(new_contents.into())).await
    }

    async fn insert_text(&self, position: i32, text: &str, length: i32) -> fdo::Result<bool> {
        Self::check_text(text)?;
        self.edit(Operation::Insert {
            position,
            text: text.into(),
            length,
        })
        .await
    }

    async fn copy_text(&self, start_pos: i32, end_pos: i32) -> fdo::Result<()> {
        if !self
            .edit(Operation::Copy {
                start: start_pos,
                end: end_pos,
            })
            .await?
        {
            return Err(fdo::Error::NotSupported(
                "text is no longer editable".into(),
            ));
        }
        Ok(())
    }

    async fn cut_text(&self, start_pos: i32, end_pos: i32) -> fdo::Result<bool> {
        self.edit(Operation::Cut {
            start: start_pos,
            end: end_pos,
        })
        .await
    }

    async fn delete_text(&self, start_pos: i32, end_pos: i32) -> fdo::Result<bool> {
        self.edit(Operation::Delete {
            start: start_pos,
            end: end_pos,
        })
        .await
    }

    async fn paste_text(&self, position: i32) -> fdo::Result<bool> {
        self.edit(Operation::Paste { position }).await
    }
}
