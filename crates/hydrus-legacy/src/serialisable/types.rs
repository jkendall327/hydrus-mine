//! Serialisable type ids, their names and v688's current versions.
//!
//! The table is transcribed from `hydrus/core/HydrusSerialisable.py` (the
//! `SERIALISABLE_TYPE_*` constants) and each registered class's
//! `SERIALISABLE_VERSION`, and is checked against the reference in the
//! fixture tests.

use std::fmt;

/// A serialisable object's type, as stored in the first slot of its tuple.
///
/// This is an open newtype rather than an enum because objects of types we
/// do not decode must still be carried across verbatim.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SerialisableType(pub u16);

impl SerialisableType {
    pub const SHORTCUT_SET: Self = Self(2);
    pub const PREDICATE: Self = Self(14);
    pub const FILE_SEARCH_CONTEXT: Self = Self(15);
    pub const DICTIONARY: Self = Self(21);
    pub const CLIENT_OPTIONS: Self = Self(22);
    pub const LIST: Self = Self(26);
    pub const BYTES_DICT: Self = Self(33);
    pub const CREDENTIALS: Self = Self(35);
    pub const METADATA: Self = Self(37);
    pub const BANDWIDTH_RULES: Self = Self(38);
    pub const BANDWIDTH_TRACKER: Self = Self(39);
    pub const DUPLICATE_CONTENT_MERGE_OPTIONS: Self = Self(43);
    pub const TAG_FILTER: Self = Self(44);
    pub const MEDIA_SORT: Self = Self(49);
    pub const CLIENT_API_MANAGER: Self = Self(75);
    pub const CLIENT_API_PERMISSIONS: Self = Self(76);
    pub const MEDIA_COLLECT: Self = Self(78);
    pub const TAG_DISPLAY_MANAGER: Self = Self(79);
    pub const TAG_CONTEXT: Self = Self(80);
    pub const FAVOURITE_SEARCH_MANAGER: Self = Self(81);
    pub const TAG_AUTOCOMPLETE_OPTIONS: Self = Self(85);
    pub const NUMBER_TEST: Self = Self(93);
    pub const TAG_SORT: Self = Self(101);
    pub const LOCATION_CONTEXT: Self = Self(103);
    pub const GUI_SESSION_CONTAINER: Self = Self(104);
    pub const GUI_SESSION_PAGE_DATA: Self = Self(105);
    pub const DUPLICATES_AUTO_RESOLUTION_RULE: Self = Self(128);
    pub const SERVICE_SPECIFIER: Self = Self(142);
    pub const NOTE_IMPORT_OPTIONS: Self = Self(153);

    pub const fn code(self) -> u16 {
        self.0
    }

    fn entry(self) -> Option<&'static (u16, &'static str, Option<u32>)> {
        TABLE
            .binary_search_by_key(&self.0, |(code, _, _)| *code)
            .ok()
            .map(|i| &TABLE[i])
    }

    /// The reference's name for this type (the constant's name, lowercased,
    /// without the `SERIALISABLE_TYPE_` prefix), if it is a known type.
    pub fn name(self) -> Option<&'static str> {
        self.entry().map(|(_, name, _)| *name)
    }

    /// The version v688 writes for this type. `None` for unknown types and
    /// for the few ids that no class is registered for.
    pub fn current_version(self) -> Option<u32> {
        self.entry().and_then(|(_, _, version)| *version)
    }

    /// Every type id v688 defines.
    pub fn all() -> impl Iterator<Item = SerialisableType> {
        TABLE.iter().map(|(code, _, _)| SerialisableType(*code))
    }
}

impl fmt::Debug for SerialisableType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => write!(f, "SerialisableType({} {name})", self.0),
            None => write!(f, "SerialisableType({})", self.0),
        }
    }
}

impl fmt::Display for SerialisableType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => write!(f, "{name} ({})", self.0),
            None => write!(f, "unknown type {}", self.0),
        }
    }
}

/// `(type id, name, current version)`, sorted by id.
const TABLE: &[(u16, &str, Option<u32>)] = &[
    (0, "base", None),
    (1, "base_named", None),
    (2, "shortcut_set", Some(2)),
    (3, "subscription_legacy", Some(10)),
    (4, "periodic", None),
    (5, "gallery_identifier", Some(1)),
    (6, "tag_import_options_legacy", Some(9)),
    (7, "file_import_options_legacy", Some(15)),
    (8, "file_seed_cache", Some(8)),
    (9, "hdd_import", Some(4)),
    (10, "server_to_client_content_update_package", None),
    (11, "server_to_client_service_update_package", None),
    (12, "management_controller", Some(17)),
    (13, "gui_session_legacy", Some(4)),
    (14, "predicate", Some(8)),
    (15, "file_search_context", Some(5)),
    (16, "export_folder", Some(9)),
    (17, "watcher_import", Some(10)),
    (18, "simple_downloader_import", Some(6)),
    (19, "import_folder", Some(11)),
    (20, "multiple_gallery_import", Some(10)),
    (21, "dictionary", Some(2)),
    (22, "client_options", Some(8)),
    (23, "content", Some(1)),
    (24, "petition", Some(3)),
    (25, "account_identifier", Some(1)),
    (26, "list", Some(3)),
    (27, "parse_formula_html", Some(8)),
    (28, "urls_import", Some(5)),
    (29, "parse_node_content_link", Some(1)),
    (30, "content_parser", Some(7)),
    (31, "parse_formula_json", Some(4)),
    (32, "parse_root_file_lookup", Some(2)),
    (33, "bytes_dict", Some(1)),
    (34, "content_update", Some(1)),
    (35, "credentials", Some(1)),
    (36, "definitions_update", Some(1)),
    (37, "metadata", Some(1)),
    (38, "bandwidth_rules", Some(1)),
    (39, "bandwidth_tracker", Some(1)),
    (40, "client_to_server_update", Some(1)),
    (41, "shortcut", Some(3)),
    (42, "application_command", Some(7)),
    (43, "duplicate_content_merge_options", Some(8)),
    (44, "tag_filter", Some(1)),
    (45, "network_bandwidth_manager_legacy", Some(1)),
    (46, "network_session_manager_legacy", Some(1)),
    (47, "network_context", Some(2)),
    (48, "network_login_manager", Some(1)),
    (49, "media_sort", Some(3)),
    (50, "url_class", Some(15)),
    (51, "string_match", Some(1)),
    (52, "checker_options", Some(1)),
    (53, "network_domain_manager", Some(7)),
    (54, "subscription_query_legacy", Some(3)),
    (55, "string_converter", Some(2)),
    (56, "filename_tagging_options", Some(2)),
    (57, "file_seed", Some(8)),
    (58, "page_parser", Some(3)),
    (59, "parse_formula_zipper", Some(3)),
    (60, "parse_formula_context_variable", Some(3)),
    (61, "tag_summary_generator", Some(2)),
    (62, "parse_rule_html", Some(3)),
    (63, "simple_downloader_parse_formula", Some(1)),
    (64, "multiple_watcher_import", Some(4)),
    (65, "service_tag_import_options", Some(4)),
    (66, "gallery_seed", Some(4)),
    (67, "gallery_seed_log", Some(1)),
    (68, "gallery_import", Some(4)),
    (69, "gallery_url_generator", Some(1)),
    (70, "nested_gallery_url_generator", Some(1)),
    (71, "domain_metadata_package", Some(1)),
    (72, "login_credential_definition", Some(1)),
    (73, "login_script_domain", Some(2)),
    (74, "login_step", Some(2)),
    (75, "client_api_manager", Some(1)),
    (76, "client_api_permissions", Some(2)),
    (77, "service_keys_to_tags", Some(1)),
    (78, "media_collect", Some(2)),
    (79, "tag_display_manager", Some(4)),
    (80, "tag_context", Some(2)),
    (81, "favourite_search_manager", Some(1)),
    (82, "note_import_options_legacy", Some(2)),
    (83, "string_splitter", Some(2)),
    (84, "string_processor", Some(1)),
    (85, "tag_autocomplete_options", Some(5)),
    (86, "subscription_query_log_container", Some(1)),
    (87, "subscription_query_header", Some(3)),
    (88, "subscription", Some(4)),
    (89, "file_seed_cache_status", Some(1)),
    (90, "subscription_container", Some(1)),
    (91, "column_list_status", Some(1)),
    (92, "column_list_manager", Some(1)),
    (93, "number_test", Some(2)),
    (94, "network_bandwidth_manager", Some(1)),
    (95, "network_session_manager", Some(1)),
    (96, "network_session_manager_session_container", Some(2)),
    (97, "network_bandwidth_manager_tracker_container", Some(1)),
    (98, "sidecar_exporter", None),
    (99, "string_sorter", Some(1)),
    (100, "string_slicer", Some(1)),
    (101, "tag_sort", Some(1)),
    (102, "account_type", Some(2)),
    (103, "location_context", Some(1)),
    (104, "gui_session_container", Some(1)),
    (105, "gui_session_page_data", Some(1)),
    (106, "gui_session_container_page_notebook", Some(1)),
    (107, "gui_session_container_page_single", Some(1)),
    (108, "presentation_import_options", Some(2)),
    (109, "metadata_single_file_router", Some(3)),
    (110, "metadata_single_file_importer_txt", Some(4)),
    (111, "metadata_single_file_importer_media_tags", Some(3)),
    (112, "string_tag_filter", Some(1)),
    (113, "metadata_single_file_exporter_json", Some(2)),
    (114, "metadata_single_file_importer_json", Some(3)),
    (115, "metadata_single_file_exporter_media_tags", Some(1)),
    (116, "metadata_single_file_exporter_txt", Some(3)),
    (117, "metadata_single_file_exporter_media_urls", Some(1)),
    (118, "metadata_single_file_importer_media_urls", Some(2)),
    (119, "metadata_single_file_exporter_media_notes", Some(2)),
    (120, "metadata_single_file_importer_media_notes", Some(1)),
    (121, "timestamp_data", Some(2)),
    (
        122,
        "metadata_single_file_exporter_media_timestamps",
        Some(1),
    ),
    (
        123,
        "metadata_single_file_importer_media_timestamps",
        Some(1),
    ),
    (124, "petition_header", Some(1)),
    (125, "string_joiner", Some(2)),
    (126, "file_filter", Some(1)),
    (127, "url_class_parameter_fixed_name", Some(2)),
    (128, "duplicates_auto_resolution_rule", Some(3)),
    (129, "duplicates_auto_resolution_pair_selector", Some(1)),
    (
        130,
        "duplicates_auto_resolution_pair_comparator_one_file_metadata_conditional",
        Some(1),
    ),
    (
        131,
        "duplicates_auto_resolution_pair_comparator_two_files_relative_file_info",
        Some(1),
    ),
    (132, "metadata_conditional", Some(1)),
    (133, "parse_formula_nested", Some(2)),
    (134, "potential_duplicates_search_context", Some(1)),
    (135, "subsidiary_page_parser", Some(2)),
    (136, "parse_formula_static", Some(1)),
    (
        137,
        "duplicates_auto_resolution_pair_comparator_two_files_relative_hardcoded",
        Some(1),
    ),
    (
        138,
        "duplicates_auto_resolution_pair_comparator_two_files_relative_visual_duplicates",
        Some(1),
    ),
    (139, "url_domain_mask", Some(1)),
    (
        140,
        "duplicates_auto_resolution_pair_comparator_or",
        Some(1),
    ),
    (
        141,
        "duplicates_auto_resolution_pair_comparator_and",
        Some(1),
    ),
    (142, "service_specifier", Some(1)),
    (143, "import_options_container", Some(1)),
    (144, "import_options_manager", Some(1)),
    (145, "prefetch_import_options", Some(2)),
    (146, "network_context_settings", Some(1)),
    (147, "network_context_status", Some(1)),
    (148, "file_filtering_import_options", Some(1)),
    (149, "location_import_options", Some(1)),
    (150, "tag_filtering_import_options", Some(1)),
    (151, "tag_import_options", Some(1)),
    (
        152,
        "duplicates_auto_resolution_pair_comparator_one_file_hardcoded",
        Some(1),
    ),
    (153, "note_import_options", Some(1)),
    (154, "network_context_record", Some(1)),
    (155, "executable_manager", Some(1)),
    (156, "executable_callable", Some(1)),
    (157, "executable_call_local_process_call", Some(1)),
    (158, "id_and_name", Some(1)),
    (
        159,
        "executable_call_local_process_input_template_param_processing_rule",
        Some(1),
    ),
    (
        160,
        "executable_call_local_process_default_launch_file",
        Some(1),
    ),
    (
        161,
        "executable_call_local_process_default_launch_url",
        Some(1),
    ),
    (162, "external_programs_import_options", Some(1)),
    (
        163,
        "external_programs_import_options_single_entry",
        Some(1),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_unique() {
        assert!(TABLE.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(SerialisableType::DICTIONARY.name(), Some("dictionary"));
        assert_eq!(SerialisableType::DICTIONARY.current_version(), Some(2));
        assert_eq!(SerialisableType(9999).name(), None);
    }
}
