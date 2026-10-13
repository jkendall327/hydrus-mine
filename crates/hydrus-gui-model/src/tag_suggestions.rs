//! Filtered, add-only most-used/recent suggestions for a captured media selection.
use hydrus_core::tag_sort::sort_tags;
use hydrus_store::{Store, settings};

/// A list preserves selected tags across broadcasts and retires them on
/// service changes. It selects, keys and decorates as the reference's
/// suggestion lists do (`crate::tag_list_box`).
#[derive(Debug, Default)]
pub struct List {
    service: Option<hydrus_core::ServiceKey>,
    pub tags: Vec<String>,
    pub list: crate::tag_list_box::TagListBox,
}
impl List {
    /// A list drawing these decorators until its menu changes them.
    pub fn new(decorators: crate::tag_list_box::Decorators) -> Self {
        Self {
            list: crate::tag_list_box::TagListBox::new(decorators),
            ..Self::default()
        }
    }
    /// New tags, undecorated.
    pub fn update(&mut self, service: Option<hydrus_core::ServiceKey>, tags: Vec<String>) {
        self.update_with(service, tags, |tags| {
            tags.iter()
                .map(|tag| crate::tag_list_box::Row {
                    tag: tag.clone(),
                    ..Default::default()
                })
                .collect()
        });
    }
    /// New tags, decorated by `decorate` when they changed.
    pub fn update_with(
        &mut self,
        service: Option<hydrus_core::ServiceKey>,
        tags: Vec<String>,
        decorate: impl FnOnce(&[String]) -> Vec<crate::tag_list_box::Row>,
    ) {
        if self.service == service && self.tags == tags {
            return;
        }
        if self.service != service {
            self.list.deselect_all();
        }
        self.list.set_rows(decorate(&tags));
        self.service = service;
        self.tags = tags;
    }
    /// A click on a drawn row.
    pub fn click(&mut self, row: usize, ctrl: bool, shift: bool) {
        self.list.click(Some(row), ctrl, shift);
    }
    pub fn selected(&self) -> Vec<String> {
        self.list.selected_tags()
    }
    /// Whether each drawn row is selected.
    pub fn mask(&self) -> Vec<bool> {
        self.list.mask()
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

/// Make these the newest recent tags of a service (`push_recent_tags`).
/// Each push is later than the last, even within a millisecond.
pub fn push_recent(
    store: &Store,
    service: hydrus_core::ServiceId,
    tags: &[hydrus_core::Tag],
) -> hydrus_store::Result<()> {
    let tags = tags.to_vec();
    store.write(move |ctx| {
        let latest: Option<i64> = ctx.conn().query_row(
            "SELECT MAX(used_ms) FROM recent_tags WHERE service_id=?",
            rusqlite::params![service],
            |row| row.get(0),
        )?;
        let now = hydrus_core::time::TimestampMs::now()
            .0
            .max(latest.map_or(0, |l| l + 1));
        for tag in &tags {
            let id = hydrus_store::master::intern_tag(ctx.conn(), tag)?;
            ctx.conn().execute(
                "INSERT INTO recent_tags(service_id,tag_id,used_ms) VALUES(?,?,?) ON CONFLICT(service_id,tag_id) DO UPDATE SET used_ms=excluded.used_ms",
                rusqlite::params![service, id, now],
            )?;
        }
        Ok(())
    })
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
