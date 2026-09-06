//! An open widget protocol, persistent tree, and demand-driven UI runtime.
//! No windowing, GPU, filesystem, or application dependencies.

mod geometry;
mod paint;
mod runtime;
mod widget;
pub use geometry::*;
pub use paint::*;
pub use runtime::*;
pub use widget::*;
