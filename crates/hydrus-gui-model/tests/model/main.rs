//! hydrus-gui-model's integration tests, one module per part of the GUI's
//! workings, against the reference's recordings where it has one. They
//! build without the windows, so they (and mutation testing them) run in
//! seconds; the windows' own tests are hydrus-gui's.

mod about;
mod auto_resolution_review;
mod auto_resolution_rules;
mod checker_options;
mod datetime_editor;
mod duplicates_page;
mod edit_subscription;
mod file_log;
mod filename_tagging;
mod filetype_tree;
mod folders;
mod import_options_editor;
mod importer_menu;
mod local_import_dialog;
mod main_menu;
mod manage_notes;
mod merge_options_editor;
mod merge_summaries;
mod options_dialog;
mod ratings_editor;
mod recent_predicates;
mod search_log;
mod selection;
mod session_saving;
mod sidecar_descriptions;
mod sidecar_editors;
mod sidecar_previews;
mod string_converter_editor;
mod string_match_editor;
mod string_processor_editor;
mod string_tag_filter_tests;
mod subscriptions_buttons;
mod subscriptions_dedupe;
mod subscriptions_list;
mod tag_filter_editor;
mod thumbnail_ratings;
mod times_editor;
mod urls_editor;
