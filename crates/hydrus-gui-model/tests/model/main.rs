//! hydrus-gui-model's integration tests, one module per part of the GUI's
//! workings, against the reference's recordings where it has one. They
//! build without the windows, so they (and mutation testing them) run in
//! seconds; the windows' own tests are hydrus-gui's.

mod checker_options;
mod local_import_dialog;
mod main_menu;
mod options_dialog;
mod recent_predicates;
mod selection;
mod session_saving;
mod thumbnail_ratings;
