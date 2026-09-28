#![doc = include_str!("../README.md")]
#[cfg(feature = "accessibility")]
mod accessibility;
#[cfg(feature = "inspection")]
mod inspection;
#[cfg(feature = "accessibility")]
mod linux_atspi;
#[cfg(feature = "accessibility")]
mod linux_edit;
#[cfg(feature = "accessibility")]
mod platform_accessibility;
mod renderer;
mod window;
pub use renderer::{Renderer, RendererFactory};
pub use window::{run, run_with, WakeHandle, WindowKind, WindowOptions};

/// Blocking system file chooser. Hosts may call this from their platform's dialog thread.
#[cfg(feature = "dialogs")]
pub fn open_file(filters: &[(&str, &[&str])]) -> Result<Option<std::path::PathBuf>, String> {
    let mut dialog = native_dialog::FileDialog::new();
    for (label, extensions) in filters {
        dialog = dialog.add_filter(label, extensions);
    }
    dialog.show_open_single_file().map_err(|e| e.to_string())
}

/// Blocking system save chooser, including the platform's overwrite confirmation.
#[cfg(feature = "dialogs")]
pub fn save_file(
    filename: &str,
    filters: &[(&str, &[&str])],
) -> Result<Option<std::path::PathBuf>, String> {
    let mut dialog = native_dialog::FileDialog::new().set_filename(filename);
    for (label, extensions) in filters {
        dialog = dialog.add_filter(label, extensions);
    }
    dialog.show_save_single_file().map_err(|e| e.to_string())
}

/// Native window types used by custom renderer factories.
pub mod renderer_types {
    pub use crate::renderer::{ActiveEventLoop, Window, WindowAttributes};
}
