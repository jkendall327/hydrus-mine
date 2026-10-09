//! Filtered, add-only most-used/recent suggestions for a captured media selection.
use hydrus_core::tag_sort::sort_tags;
use hydrus_store::{Store, settings};

/// A list preserves selected tags across broadcasts and retires them on service changes.
#[derive(Debug, Default)]
pub struct List {
    service: Option<hydrus_core::ServiceKey>,
    pub tags: Vec<String>,
    selection: crate::list_selection::ListSelection<usize>,
}
impl List {
    pub fn update(&mut self, service: Option<hydrus_core::ServiceKey>, tags: Vec<String>) {
        if self.service == service && self.tags == tags {
            return;
        }
        let selected: Vec<_> = if self.service == service {
            self.selected()
        } else {
            Vec::new()
        };
        self.selection.select_many(
            &tags
                .iter()
                .enumerate()
                .filter_map(|(i, t)| selected.contains(t).then_some(i))
                .collect::<Vec<_>>(),
        );
        self.service = service;
        self.tags = tags;
    }
    pub fn click(&mut self, row: usize, ctrl: bool, shift: bool) {
        self.selection
            .click(&(0..self.tags.len()).collect::<Vec<_>>(), row, ctrl, shift);
    }
    pub fn selected(&self) -> Vec<String> {
        self.tags
            .iter()
            .enumerate()
            .filter(|(i, _)| self.selection.is_selected(*i))
            .map(|(_, t)| t.clone())
            .collect()
    }
    pub fn mask(&self) -> Vec<bool> {
        (0..self.tags.len())
            .map(|i| self.selection.is_selected(i))
            .collect()
    }
}

/// Only useful tags remain; one activation never toggles an existing mapping off.
pub fn useful(
    tags: Vec<String>,
    counts: &std::collections::BTreeMap<String, usize>,
    files: usize,
) -> Vec<String> {
    tags.into_iter()
        .filter(|tag| counts.get(tag).copied().unwrap_or(0) < files)
        .collect()
}

/// Read the currently persisted service list, so immediate menu mutations broadcast.
pub fn most_used(store: &Store, key: &hydrus_core::ServiceKey) -> Vec<String> {
    let options: settings::TagAutocompleteTabs = store.read(settings::get).unwrap_or_default();
    let mut tags = options
        .most_used
        .get(&key.to_hex())
        .cloned()
        .unwrap_or_default();
    let presentation: hydrus_core::tag_presentation::TagPresentation =
        store.read(settings::get).unwrap_or_default();
    tags = tags
        .into_iter()
        .filter_map(|tag| hydrus_core::Tag::new(&tag))
        .map(|tag| tag.as_str().to_owned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    sort_tags(
        &presentation.search_page_sort,
        &mut tags,
        String::as_str,
        |_| 0,
        &presentation.user_namespaces,
    );
    tags
}

/// Imported/native recent history, newest first; ties retain stable tag identity.
/// Reading decays it, as the reference's does: what is older than the newest
/// `limit` is forgotten for good (`GetRecentTags`).
pub fn recent(
    store: &Store,
    service: hydrus_core::ServiceId,
    limit: usize,
) -> hydrus_store::Result<Vec<String>> {
    let all = store.read(|conn| {
        conn.prepare(
            "SELECT tag_id FROM recent_tags WHERE service_id=? ORDER BY used_ms DESC, tag_id",
        )?
        .query_map(rusqlite::params![service], |row| {
            row.get::<_, hydrus_core::TagId>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
    })?;
    let (kept, decayed) = all.split_at(limit.min(all.len()));
    if !decayed.is_empty() {
        let decayed = decayed.to_vec();
        store.write(move |ctx| {
            for tag in &decayed {
                ctx.conn().execute(
                    "DELETE FROM recent_tags WHERE service_id=? AND tag_id=?",
                    rusqlite::params![service, tag],
                )?;
            }
            Ok(())
        })?;
    }
    store.read(|conn| {
        kept.iter()
            .filter_map(|id| hydrus_store::master::tag(conn, *id).transpose())
            .map(|tag| tag.map(|tag| tag.as_str().to_owned()))
            .collect()
    })
}
