//! The generated Slint UI, shared by the desktop client and its test targets.
//!
//! It is ~87 MB of Rust, too much for one rustc invocation on a 16 GB machine,
//! so it is built as several crates (one per group of windows) that this one
//! re-exports, so that users name everything as `hydrus_gui_ui::Name`.
//!
//! (spike) With the `interpreter` feature instead, the same API is generated
//! by slint-build's live-preview generator and backed by Slint's interpreter.

#[cfg(all(feature = "compiled", feature = "interpreter"))]
compile_error!("choose one of the `compiled` and `interpreter` features");

#[cfg(feature = "compiled")]
pub use hydrus_gui_ui_core::*;
#[cfg(feature = "compiled")]
pub use hydrus_gui_ui_dialogs_1::*;
#[cfg(feature = "compiled")]
pub use hydrus_gui_ui_dialogs_2::*;
#[cfg(feature = "compiled")]
pub use hydrus_gui_ui_dialogs_3::*;
#[cfg(feature = "compiled")]
pub use hydrus_gui_ui_dialogs_4::*;
#[cfg(feature = "compiled")]
pub use hydrus_gui_ui_options::*;

#[cfg(feature = "interpreter")]
#[allow(missing_debug_implementations, clippy::all)]
mod ui {
    slint::include_modules!();
}
#[cfg(feature = "interpreter")]
pub use ui::*;

#[cfg(feature = "interpreter-cached")]
pub mod interp;
