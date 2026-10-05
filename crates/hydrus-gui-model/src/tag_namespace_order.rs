//! Raw namespace grouping order: blank and colon are values, not separators.
use crate::delete_files::ReasonQueue;

pub const MESSAGE: &str = "Enter the namespace. Leave blank for unnamespaced tags, use \":\" for all unspecified namespaced tags.";
pub const DESCRIPTION: &str = "You can manage the custom \"(user)\" namespace grouping sort here. This lets you put, say, \"creator\" tags above any other namespace in a tag sort.\nAny namespaces not listed here will be listed afterwards in a-z format, with unnamespaced following, just like the normal (a-z) namespace grouping.";

/// Stable queue keys distinguish identical raw namespace entries.
#[derive(Debug)]
pub struct Editor(pub ReasonQueue);
impl Editor {
    /// Start from the saved raw order without normalization.
    pub fn new(values: &[String]) -> Self {
        Self(ReasonQueue::new(values))
    }
    /// Render only the two namespace sentinels specially.
    pub fn label(value: &str) -> &str {
        match value {
            "" => "unnamespaced",
            ":" => "namespaced",
            raw => raw,
        }
    }
}

/// Apply only changed presentation fields so namespace edits preserve live peers.
pub(crate) fn save_presentation(
    conn: &rusqlite::Connection,
    after: &hydrus_core::tag_presentation::TagPresentation,
    before: &hydrus_core::tag_presentation::TagPresentation,
) -> hydrus_store::Result<()> {
    if after == before {
        return Ok(());
    }
    let mut latest: hydrus_core::tag_presentation::TagPresentation =
        hydrus_store::settings::get(conn)?;
    macro_rules! copy_fields { ($($field:ident),*) => { $(if after.$field != before.$field { latest.$field = after.$field; })* }; }
    copy_fields!(
        show_namespaces,
        show_number_namespaces,
        show_subtag_number_namespaces,
        unselected_tag_limit,
        sidebar_display_type,
        viewer_display_type,
        replace_underscores,
        replace_emojis,
        search_page_sort,
        media_viewer_sort
    );
    for (latest, after, before) in [
        (
            &mut latest.namespace_connector,
            &after.namespace_connector,
            &before.namespace_connector,
        ),
        (
            &mut latest.sibling_connector,
            &after.sibling_connector,
            &before.sibling_connector,
        ),
    ] {
        if after != before {
            latest.clone_from(after);
        }
    }
    if after.user_namespaces != before.user_namespaces {
        latest.user_namespaces.clone_from(&after.user_namespaces);
    }
    hydrus_store::settings::set(conn, &latest)
}
