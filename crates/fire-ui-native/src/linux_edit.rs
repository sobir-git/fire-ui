//! Apply native edits on the UI thread against the current widget state.
use crate::linux_atspi::{Operation, PendingEdit};
use fire_ui::{SemanticAction, SemanticActionKind, Ui, Widget};
use std::time::Instant;

pub(crate) fn apply<W: Widget>(
    ui: &mut Ui<W>,
    pending: &PendingEdit,
    mut copy: impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    if Instant::now() >= pending.deadline || pending.reply.is_closed() {
        return Err("request expired".into());
    }
    // accesskit_consumer 0.39 encodes (local node << 64) | tree index.
    // Fire UI publishes one root tree, with index zero. This boundary is checked
    // by the native probe and the pinned AT-SPI translation dependency.
    let full = u128::from(pending.node);
    if full as u64 != 0 {
        return Err("unsupported accessibility subtree".into());
    }
    let id = (full >> 64) as u64;
    let node = ui
        .semantics()
        .into_iter()
        .find(|n| n.id == id)
        .ok_or("target unavailable")?;
    let s = node.semantics;
    if s.disabled || s.text.is_none() {
        return Err("text unavailable".into());
    }
    let value = s.value.as_deref().ok_or("text unavailable")?;
    let operation = &pending.operation;
    let required = match operation {
        Operation::Set(_) => Some(SemanticActionKind::SetValue),
        Operation::Copy { .. } => None,
        _ => Some(SemanticActionKind::ReplaceText),
    };
    if required.is_some_and(|a| !s.actions.contains(&a)) {
        return Err("text is read-only".into());
    }
    let range = |start, end| -> Result<std::ops::Range<usize>, String> {
        if start > end {
            return Err("invalid text range".into());
        }
        Ok(byte_offset(value, start)?..byte_offset(value, end)?)
    };
    let action = match operation {
        Operation::Set(text) => SemanticAction::SetValue(text.clone()),
        Operation::Insert {
            position,
            text,
            length,
        } => {
            let at = byte_offset(value, *position)?;
            let text = insert_prefix(text, *length)?;
            SemanticAction::ReplaceText {
                start: at,
                end: at,
                text: text.into(),
            }
        }
        Operation::Delete { start, end } => {
            let r = range(*start, *end)?;
            SemanticAction::ReplaceText {
                start: r.start,
                end: r.end,
                text: String::new(),
            }
        }
        Operation::Copy { start, end } | Operation::Cut { start, end } => {
            let r = range(*start, *end)?;
            // Keep the clipboard owner alive in the host. A failed copy must
            // never turn CutText into a destructive delete.
            copy(&value[r.clone()])?;
            if matches!(operation, Operation::Copy { .. }) {
                return Ok(());
            }
            SemanticAction::ReplaceText {
                start: r.start,
                end: r.end,
                text: String::new(),
            }
        }
        Operation::Paste { .. } => return Err("paste needs clipboard completion".into()),
    };
    // Clipboard access can take time. Never mutate after a caller timed out.
    if Instant::now() >= pending.deadline || pending.reply.is_closed() {
        return Err("request expired".into());
    }
    ui.accessibility(id, action).map_err(|e| format!("{e:?}"))
}

#[cfg(feature = "clipboard")]
pub(crate) fn prepare_paste<W: Widget>(
    ui: &Ui<W>,
    pending: &PendingEdit,
) -> Result<String, String> {
    if Instant::now() >= pending.deadline || pending.reply.is_closed() {
        return Err("request expired".into());
    }
    let full = u128::from(pending.node);
    if full as u64 != 0 {
        return Err("unsupported accessibility subtree".into());
    }
    let node = ui
        .semantics()
        .into_iter()
        .find(|n| n.id == (full >> 64) as u64)
        .ok_or("target unavailable")?;
    let s = node.semantics;
    if s.disabled || s.text.is_none() || !s.actions.contains(&SemanticActionKind::ReplaceText) {
        return Err("text is unavailable or read-only".into());
    }
    let value = s.value.ok_or("text unavailable")?;
    let Operation::Paste { position } = pending.operation else {
        return Err("not a paste request".into());
    };
    byte_offset(&value, position)?;
    Ok(value)
}

fn byte_offset(text: &str, offset: i32) -> Result<usize, String> {
    let offset = usize::try_from(offset).map_err(|_| "negative character offset")?;
    text.char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .nth(offset)
        .ok_or_else(|| "character offset outside text".into())
}
fn insert_prefix(text: &str, length: i32) -> Result<&str, String> {
    // AT-SPI specifies UTF-8 bytes for length, Unicode characters for position.
    // -1 is the all-text convention used by GTK's native implementation.
    let length = if length == -1 {
        text.len()
    } else {
        usize::try_from(length).map_err(|_| "invalid insertion length")?
    };
    text.get(..length.min(text.len()))
        .ok_or_else(|| "insertion length is not a UTF-8 boundary".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atspi_offsets_are_scalars_but_insert_lengths_are_bytes() {
        let text = "é🔥e\u{301}";
        assert_eq!(byte_offset(text, 2), Ok(6));
        assert_eq!(byte_offset(text, 4), Ok(9));
        assert!(byte_offset(text, -1).is_err());
        assert!(byte_offset(text, 5).is_err());
        assert_eq!(insert_prefix(text, 2), Ok("é"));
        assert_eq!(insert_prefix(text, -1), Ok(text));
        assert!(insert_prefix(text, 3).is_err());
        assert_eq!(insert_prefix(text, 10), Ok(text));
        assert!(insert_prefix(text, -2).is_err());
    }
}
