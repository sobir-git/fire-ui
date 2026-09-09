//! Linux AT-SPI transport, derived from AccessKit's MIT-licensed Unix adapter.
//!
//! Upstream: accesskit_unix 0.23.0, commit
//! 42e53b0d829e7a0b34dc8803bb06012db2e80cc6, adapters/unix/src.
//! The published accesskit_atspi_common owns tree translation and event semantics.
//! We own this transport because upstream's EditableText interface only implements
//! SetTextContents and does not expose an interface-extension hook. All six edit
//! methods here dispatch a single operation to the existing UI thread.
//! Original copyright notices are retained; see LICENSE-MIT in this directory.

mod adapter;
mod atspi;
mod context;
mod editing;
mod executor;
pub(crate) mod text;
mod util;

pub(crate) use adapter::Adapter;
pub(crate) use editing::{EditHandler, Operation, PendingEdit};
