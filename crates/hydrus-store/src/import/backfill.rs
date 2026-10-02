//! Gallery and watcher pages' own state (their downloader, file limit,
//! checker options, the queue they show) for stores imported before it
//! was carried over: read again from the reference's sessions, which the
//! import keeps verbatim.

use std::collections::{HashMap, HashSet};

use hydrus_core::pages::{DownloaderKind, DownloaderPageState, Page, PageContent};
use hydrus_legacy::objects::gui_sessions::{PageContent as LegacyContent, page_data, session};
use hydrus_legacy::serialisable::{SerialisableObject, SerialisableType};
use rusqlite::Connection;

use crate::error::Result;
use crate::{queues, sessions, settings};

/// Whether the backfill has been done (once a store).
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct Filled {
    done: bool,
}

impl settings::Setting for Filled {
    const KEY: &'static str = "downloader_page_state_filled";
}

/// What tells a downloader page's queues apart: each search's query and
/// downloader, or each watcher's thread.
type Identity = Vec<(String, String)>;

/// A reference gallery or watcher page, from a kept session.
struct KeptPage {
    name: String,
    kind: DownloaderKind,
    identity: Identity,
    state: DownloaderPageState,
    /// The queue it showed, by its place among its queues.
    highlighted: Option<usize>,
}

/// Fill in the own state of the open session's gallery and watcher pages
/// an earlier import left without it, from the reference's sessions the
/// import kept: the page with the same name and the same searches (or
/// threads) in the same order, wherever it now is. Done once a store; how
/// many pages were filled.
pub fn fill_downloader_page_state(conn: &Connection, now: i64) -> Result<usize> {
    if settings::get::<Filled>(conn)?.done {
        return Ok(0);
    }
    let filled = fill(conn, now)?;
    settings::set(conn, &Filled { done: true })?;
    Ok(filled)
}

fn fill(conn: &Connection, now: i64) -> Result<usize> {
    let Some(mut session) = sessions::load(conn, sessions::LAST_SESSION)? else {
        return Ok(0);
    };
    let mut wanting = Vec::new();
    for page in session.all_pages() {
        if let PageContent::Downloader {
            kind: kind @ (DownloaderKind::Gallery | DownloaderKind::Watchers),
            queues,
            page: None,
            ..
        } = &page.content
        {
            wanting.push((
                page.key,
                *kind,
                page.name.clone(),
                identity(conn, *kind, queues)?,
            ));
        }
    }
    if wanting.is_empty() {
        return Ok(0);
    }
    let kept = kept_pages(conn)?;
    let mut states = HashMap::new();
    for (key, kind, name, identity) in wanting {
        if let Some(found) = kept
            .iter()
            .find(|k| k.kind == kind && k.name == name && k.identity == identity)
        {
            states.insert(key, (found.state.clone(), found.highlighted));
        }
    }
    if states.is_empty() {
        return Ok(0);
    }
    let filled = states.len();
    set_states(&mut session.pages, &states);
    sessions::save(conn, &session, now)?;
    Ok(filled)
}

/// Give the pages their state, the highlighted place becoming that queue.
fn set_states(
    pages: &mut [Page],
    states: &HashMap<hydrus_core::pages::PageKey, (DownloaderPageState, Option<usize>)>,
) {
    for page in pages {
        match &mut page.content {
            PageContent::Pages(children) => set_states(children, states),
            PageContent::Downloader {
                queues, page: own, ..
            } => {
                if let Some((state, highlighted)) = states.get(&page.key) {
                    let mut state = state.clone();
                    state.highlighted = highlighted.and_then(|i| queues.get(i).copied());
                    *own = Some(Box::new(state));
                }
            }
            _ => {}
        }
    }
}

/// Our page's queues, as the reference's pages are told apart.
fn identity(conn: &Connection, kind: DownloaderKind, ids: &[i64]) -> Result<Identity> {
    let mut out = Vec::new();
    for &id in ids {
        let Some(queue) = queues::queue(conn, id)? else {
            continue;
        };
        let item = match kind {
            DownloaderKind::Gallery => {
                serde_json::from_value::<hydrus_core::gallery::GallerySearch>(queue.extra)
                    .ok()
                    .map(|s| (s.query, s.source_name))
            }
            _ => serde_json::from_value::<hydrus_core::watchers::WatcherState>(queue.extra)
                .ok()
                .map(|w| (w.url, String::new())),
        };
        out.extend(item);
    }
    Ok(out)
}

/// Every gallery and watcher page in the reference's kept sessions, the
/// latest saves first.
fn kept_pages(conn: &Connection) -> Result<Vec<KeptPage>> {
    let containers: Vec<(String, i64, String)> = conn
        .prepare(
            "SELECT name, version, dump FROM legacy_objects
             WHERE source = 'json_dumps_named' AND type_id = ?
             ORDER BY timestamp_ms DESC",
        )?
        .query_map(
            [i64::from(SerialisableType::GUI_SESSION_CONTAINER.0)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?
        .collect::<rusqlite::Result<_>>()?;
    let mut page_data_statement = conn.prepare(
        "SELECT version, dump FROM legacy_objects
         WHERE source = 'json_dumps_hashed' AND name = ?",
    )?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (name, version, dump) in containers {
        let Ok(object) = SerialisableObject::from_stored(
            SerialisableType::GUI_SESSION_CONTAINER,
            Some(name),
            u32::try_from(version).unwrap_or(0),
            &dump,
        ) else {
            continue;
        };
        let Ok(kept) = session(&object) else {
            continue;
        };
        for hash in kept.top.page_data_hashes() {
            // (the same page in several saves is read once)
            let hash = hex::encode_upper(hash);
            if !seen.insert(hash.clone()) {
                continue;
            }
            let found: Option<(i64, String)> = page_data_statement
                .query_row([&hash], |r| Ok((r.get(0)?, r.get(1)?)))
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    e => Err(e),
                })?;
            let Some((version, dump)) = found else {
                continue;
            };
            let Ok(object) = SerialisableObject::from_stored(
                SerialisableType::GUI_SESSION_PAGE_DATA,
                None,
                u32::try_from(version).unwrap_or(0),
                &dump,
            ) else {
                continue;
            };
            let Ok(data) = page_data(&object) else {
                continue;
            };
            let page = data.page;
            let (kind, identity, (state, highlighted)) = match &page.content {
                LegacyContent::Gallery(m) => (
                    DownloaderKind::Gallery,
                    m.gallery_imports
                        .iter()
                        .map(|g| (g.query.clone(), g.source_name.clone()))
                        .collect(),
                    super::decode::gallery_page_state(m),
                ),
                LegacyContent::Watchers(m) => (
                    DownloaderKind::Watchers,
                    m.watchers
                        .iter()
                        .filter(|w| !w.url.is_empty())
                        .map(|w| (w.url.clone(), String::new()))
                        .collect(),
                    super::decode::watcher_page_state(m),
                ),
                _ => continue,
            };
            out.push(KeptPage {
                name: page.name,
                kind,
                identity,
                state,
                highlighted,
            });
        }
    }
    Ok(out)
}
