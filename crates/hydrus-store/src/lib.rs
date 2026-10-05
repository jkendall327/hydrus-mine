//! The native hydrus-rs database.
//!
//! See `docs/rust/STORE.md` for the schema design and consistency rules.

pub mod animation_start;
pub mod archive_delete_preferences;
pub mod autocomplete;
pub mod bandwidth;
pub mod command_palette;
pub mod conn;
pub mod content;
pub mod counts;
pub mod delete_lock;
pub mod display;
pub mod domains;
pub mod downloader_update_times;
pub mod duplicates;
pub mod error;
pub mod file_maintenance;
pub mod folder_activity;
pub mod gallery;
pub mod gui_colours;
pub mod import;
pub mod import_folders;
pub mod legacy;
pub mod live;
pub mod login_runtime;
pub mod logins;
pub mod maintenance;
pub mod maintenance_gates;
pub mod manage_tags_sort;
pub mod master;
pub mod media;
pub mod menu_choice_wheel;
pub mod metadata_jobs;
pub mod network;
pub mod network_runtime;
pub mod page_layout;
pub mod paths;
pub mod pending;
pub mod physical_delete;
pub mod popup_actions;
pub mod popup_freeze;
pub mod popup_width;
pub mod popups;
pub mod preview_zoom;
pub mod queues;
pub mod radio_return;
pub mod regex_favourites;
pub mod related_tags;
pub mod schema;
pub mod services;
pub mod services_management;
pub mod session_backups;
pub mod sessions;
pub mod settings;
pub mod similar;
pub mod stats;
pub mod storage;
pub mod store;
pub mod string_conversion;
pub mod subscription_quality;
pub mod subscriptions;
pub mod synth;
pub mod tag_display;
pub mod tag_display_config;
pub mod tag_editing;
pub mod tag_migration;
pub mod text;
pub mod thumbnail_appearance;
pub mod thumbnail_preview_selection;
pub mod transfer;
pub mod trash;
pub mod urls;
pub mod viewing_maintenance;
pub mod watchers;

pub use conn::{Db, Paused, WriteCtx};
pub use error::{Result, StoreError};
pub use store::{Snapshot, Store};

pub mod api_activity;
pub mod api_permissions;
pub mod api_update_toasts;

pub mod archive_repair;

pub mod file_history;

pub mod or_connector;

pub mod image_cache;
pub mod image_colour;

pub mod duplicates_progress;
