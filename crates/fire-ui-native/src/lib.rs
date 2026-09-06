//! Optional native window, text, clipboard and presentation services.
mod accessibility;
mod painter;
mod text;
mod window;
pub(crate) use painter::GlPainter;
pub use text::NativeText;
pub use window::{run, run_with, WakeHandle, WindowOptions};

/// Blocking system file chooser. Hosts may call this from their platform's dialog thread.
pub fn open_file(filters: &[(&str, &[&str])]) -> Result<Option<std::path::PathBuf>, String> {
    let mut dialog = native_dialog::FileDialog::new();
    for (label, extensions) in filters {
        dialog = dialog.add_filter(label, extensions);
    }
    dialog.show_open_single_file().map_err(|e| e.to_string())
}

/// Blocking system save chooser, including the platform's overwrite confirmation.
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
