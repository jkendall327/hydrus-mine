//! The generated Slint UI, shared by the desktop client and its test targets.
//!
//! It is ~87 MB of Rust, too much for one rustc invocation on a 16 GB machine,
//! so it is built as several crates (one per group of windows) that this one
//! re-exports, so that users name everything as `hydrus_gui_ui::Name`.

pub use hydrus_gui_ui_core::*;
pub use hydrus_gui_ui_dialogs_1::*;
pub use hydrus_gui_ui_dialogs_2::*;
pub use hydrus_gui_ui_dialogs_3::*;
pub use hydrus_gui_ui_dialogs_4::*;
pub use hydrus_gui_ui_options::*;
