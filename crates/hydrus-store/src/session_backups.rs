//! Bounded immutable saved-session snapshots. Trees, files and selection are
//! stored together so overwriting/deleting live pages cannot damage a backup.

use hydrus_core::HashId;
use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::error::{Result, StoreError};
use crate::{queues, sessions, settings};

/// How many older snapshots to retain in addition to the latest named save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionBackupSettings {
    pub keep: usize,
}
impl Default for SessionBackupSettings {
    fn default() -> Self {
        Self { keep: 10 }
    }
}
impl settings::Setting for SessionBackupSettings {
    const KEY: &'static str = "gui_session_backups";
}

/// A snapshot's page media, independent from the live page-files table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageMedia {
    pub key: PageKey,
    pub files: Vec<HashId>,
    pub selected: Vec<HashId>,
}

/// A frozen importer's settings, auxiliary state and both ordered logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueSnapshot {
    pub queue: queues::Queue,
    pub files: Vec<queues::FileSeed>,
    pub gallery: Vec<queues::GallerySeed>,
}

/// Everything needed to restore one saved tree without consulting live pages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub session: Session,
    pub media: Vec<PageMedia>,
    #[serde(default)]
    pub queues: Vec<QueueSnapshot>,
}

/// Save a named session with rolling backups. Timestamp collision overwrites
/// the same save; a backwards clock advances beyond the latest timestamp.
/// Call within a writer transaction, after staging the new pages' media.
pub fn save(conn: &Connection, session: &Session, now_ms: i64) -> Result<()> {
    let settings: SessionBackupSettings = settings::get(conn)?;
    let mut latest: Option<i64> = conn.query_row(
        "SELECT MAX(timestamp_ms) FROM session_snapshots WHERE name = ?",
        [&session.name],
        |row| row.get(0),
    )?;
    if latest.is_none()
        && let Some(previous) = sessions::load(conn, &session.name)?
    {
        let saved = conn.query_row(
            "SELECT saved FROM sessions WHERE name = ?",
            [&session.name],
            |row| row.get::<_, i64>(0),
        )?;
        let timestamp = saved.saturating_mul(1000);
        insert(conn, &previous, timestamp)?;
        latest = Some(timestamp);
    }
    let timestamp = latest
        .filter(|&latest| latest > now_ms)
        .map_or(now_ms, |latest| latest.saturating_add(1));
    conn.execute("DELETE FROM session_snapshots WHERE name = ? AND timestamp_ms NOT IN
        (SELECT timestamp_ms FROM session_snapshots WHERE name = ? ORDER BY timestamp_ms DESC LIMIT ?)",
        params![session.name, session.name, i64::try_from(settings.keep.clamp(1, 32)).unwrap_or(32)])?;
    insert(conn, session, timestamp)?;
    sessions::save(conn, session, timestamp / 1000)
}

/// Capture a complete session in a stable writer/reader transaction.
pub fn capture(conn: &Connection, session: &Session) -> Result<Snapshot> {
    let pages = session.all_pages();
    let media = pages
        .iter()
        .map(|page| {
            Ok(PageMedia {
                key: page.key,
                files: sessions::page_files(conn, &page.key)?,
                selected: sessions::page_selected(conn, &page.key)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let ids: std::collections::BTreeSet<_> = pages
        .iter()
        .flat_map(|page| match &page.content {
            PageContent::Downloader { queues, .. } => queues.clone(),
            _ => Vec::new(),
        })
        .collect();
    let queues = ids
        .into_iter()
        .map(|id| {
            let queue = queues::queue(conn, id)?.ok_or_else(|| {
                StoreError::Corrupt(format!("session importer queue {id} is missing"))
            })?;
            Ok(QueueSnapshot {
                queue,
                files: queues::file_seeds(conn, id)?,
                gallery: queues::gallery_seeds(conn, id)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Snapshot {
        session: session.clone(),
        media,
        queues,
    })
}

fn insert(conn: &Connection, session: &Session, timestamp: i64) -> Result<()> {
    let data =
        serde_json::to_string(&capture(conn, session)?).expect("session snapshot serialises");
    conn.execute(
        "INSERT INTO session_snapshots(name, timestamp_ms, data) VALUES (?, ?, ?)
        ON CONFLICT(name, timestamp_ms) DO UPDATE SET data = excluded.data",
        params![session.name, timestamp, data],
    )?;
    Ok(())
}

/// Restore fresh page and importer identities, with their media and immutable
/// logs. Runtime transfer/live-job state starts fresh, as on a reference load.
/// Call in a writer transaction so failed restoration rolls back together.
pub fn restore_pages(conn: &Connection, snapshot: Snapshot) -> Result<Vec<Page>> {
    fn restore(
        conn: &Connection,
        pages: &mut [Page],
        media: &[PageMedia],
        importers: &[QueueSnapshot],
    ) -> Result<()> {
        for page in pages {
            let old = page.key;
            page.key = PageKey::random();
            if let Some(media) = media.iter().find(|media| media.key == old) {
                sessions::set_page_files(conn, &page.key, &media.files)?;
                sessions::set_page_selected(conn, &page.key, &media.selected)?;
            }
            match &mut page.content {
                PageContent::Pages(children) => restore(conn, children, media, importers)?,
                PageContent::Downloader {
                    queues: ids,
                    page: state,
                    ..
                } => {
                    let mut mapped = std::collections::HashMap::new();
                    for id in ids {
                        let importer = importers
                            .iter()
                            .find(|entry| entry.queue.id == *id)
                            .ok_or_else(|| {
                                StoreError::Corrupt(format!(
                                    "snapshot importer queue {id} is missing"
                                ))
                            })?;
                        let queue = &importer.queue;
                        let new = queues::create_queue(
                            conn,
                            queue.kind,
                            &queue.name,
                            Some(&page.key.0),
                            &queue.options,
                            queue.created,
                        )?;
                        queues::set_queue_extra(conn, new, &queue.extra)?;
                        queues::set_paused(
                            conn,
                            new,
                            Some(queue.files_paused),
                            Some(queue.gallery_paused),
                        )?;
                        queues::restore_file_seeds(conn, new, &importer.files)?;
                        queues::restore_gallery_seeds(conn, new, &importer.gallery)?;
                        mapped.insert(*id, new);
                        *id = new;
                    }
                    if let Some(state) = state {
                        state.highlighted =
                            state.highlighted.and_then(|id| mapped.get(&id).copied());
                    }
                }
                _ => (),
            }
        }
        Ok(())
    }
    let mut pages = snapshot.session.pages;
    restore(conn, &mut pages, &snapshot.media, &snapshot.queues)?;
    Ok(pages)
}

/// The newest archived session snapshot, for ordinary freshest append/load.
pub fn latest(conn: &Connection, name: &str) -> Result<Option<Snapshot>> {
    let timestamp: Option<i64> = conn.query_row(
        "SELECT MAX(timestamp_ms) FROM session_snapshots WHERE name = ?",
        [name],
        |row| row.get(0),
    )?;
    timestamp
        .map(|timestamp| load(conn, name, timestamp))
        .transpose()
        .map(Option::flatten)
}

/// Older snapshot timestamps grouped by session name and ordered oldest first,
/// excluding the freshest snapshot which ordinary append already loads.
pub fn names(conn: &Connection) -> Result<Vec<(String, Vec<i64>)>> {
    let mut statement = conn.prepare("SELECT name, timestamp_ms FROM session_snapshots AS snapshot
        WHERE timestamp_ms < (SELECT MAX(timestamp_ms) FROM session_snapshots WHERE name = snapshot.name)
        ORDER BY name, timestamp_ms")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut groups: Vec<(String, Vec<i64>)> = Vec::new();
    for row in rows {
        let (name, timestamp) = row?;
        if let Some((_, timestamps)) = groups.last_mut().filter(|(last, _)| *last == name) {
            timestamps.push(timestamp);
        } else {
            groups.push((name, vec![timestamp]));
        }
    }
    Ok(groups)
}

/// Fetch an exact timestamp; missing/deleted snapshots are reported as absent.
pub fn load(conn: &Connection, name: &str, timestamp: i64) -> Result<Option<Snapshot>> {
    let data: Option<String> = conn
        .query_row(
            "SELECT data FROM session_snapshots WHERE name = ? AND timestamp_ms = ?",
            params![name, timestamp],
            |row| row.get(0),
        )
        .optional()?;
    data.map(|data| {
        serde_json::from_str(&data)
            .map_err(|error| StoreError::Corrupt(format!("session backup {name}: {error}")))
    })
    .transpose()
}

/// Delete all historical snapshots with their named session.
pub fn delete(conn: &Connection, name: &str) -> Result<()> {
    conn.execute("DELETE FROM session_snapshots WHERE name = ?", [name])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydrus_core::pages::{Page, PageContent};

    fn connection() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::schema::configure(&conn).unwrap();
        crate::schema::migrate(&mut conn).unwrap();
        conn
    }
    fn session(name: &str) -> Session {
        Session {
            name: name.into(),
            pages: vec![Page {
                key: PageKey::random(),
                name: "first".into(),
                content: PageContent::Pages(Vec::new()),
            }],
        }
    }

    #[test]
    fn rolling_timestamps_match_the_reference_including_clock_reversal() {
        let conn = connection();
        let fixture = hydrus_testkit::fixture_json("session_backups.json");
        assert_eq!(
            SessionBackupSettings::default().keep,
            fixture["default_keep"].as_u64().unwrap() as usize
        );
        settings::set(&conn, &SessionBackupSettings { keep: 2 }).unwrap();
        let mut saved = session("backup test");
        for step in fixture["steps"].as_array().unwrap() {
            saved.pages[0].name = format!("version {}", step["version"].as_u64().unwrap());
            save(&conn, &saved, step["now"].as_i64().unwrap()).unwrap();
            let actual = names(&conn)
                .unwrap()
                .first()
                .map(|(_, times)| times.clone())
                .unwrap_or_default();
            assert_eq!(serde_json::json!(actual), step["backups"]);
        }
        let snapshot = load(&conn, "backup test", fixture["timestamp"].as_i64().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            snapshot.session.pages[0].name,
            fixture["appended"]["children"][0].as_str().unwrap()
        );
    }

    #[test]
    fn imported_previous_media_is_independent_and_deletion_clears_backups() {
        let conn = connection();
        let previous = session("work");
        let key = previous.pages[0].key;
        sessions::save(&conn, &previous, 100).unwrap();
        sessions::set_page_files(&conn, &key, &[HashId(3), HashId(1)]).unwrap();
        sessions::set_page_selected(&conn, &key, &[HashId(1)]).unwrap();
        let next = session("work");
        save(&conn, &next, 200_000).unwrap();
        assert!(sessions::page_files(&conn, &key).unwrap().is_empty());
        let snapshot = load(&conn, "work", 100_000).unwrap().unwrap();
        assert_eq!(snapshot.session.pages, previous.pages);
        assert_eq!(snapshot.media[0].files, [HashId(3), HashId(1)]);
        assert_eq!(snapshot.media[0].selected, [HashId(1)]);
        sessions::delete(&conn, "work").unwrap();
        assert!(names(&conn).unwrap().is_empty());
        assert!(load(&conn, "work", 100_000).unwrap().is_none());
    }
}

#[cfg(test)]
mod importer_tests {
    use super::*;
    use hydrus_core::import_options::ImportOptionsSlice;
    use hydrus_core::pages::{DownloaderKind, DownloaderPageState};
    use queues::{NewFileSeed, QueueKind, SeedStatus, SeedType};

    fn facts(conn: &Connection, id: i64) -> serde_json::Value {
        let queue = queues::queue(conn, id).unwrap().unwrap();
        serde_json::json!({"paused": queue.files_paused,
            "files": queues::file_seeds(conn, id).unwrap().iter().map(|seed| serde_json::json!({"url":seed.data,"status":seed.status.code(),"note":seed.note})).collect::<Vec<_>>(),
            "gallery": queues::gallery_seeds(conn,id).unwrap().iter().map(|seed| serde_json::json!({"url":seed.url,"status":seed.status.code(),"note":seed.note})).collect::<Vec<_>>()})
    }

    #[test]
    fn historical_importers_restore_independent_settings_and_seed_logs() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::schema::configure(&conn).unwrap();
        crate::schema::migrate(&mut conn).unwrap();
        let fixture = hydrus_testkit::fixture_json("session_importers.json");
        let key = PageKey::random();
        let old = queues::create_queue(
            &conn,
            QueueKind::Urls,
            "url history",
            Some(&key.0),
            &ImportOptionsSlice::default(),
            100,
        )
        .unwrap();
        queues::set_paused(&conn, old, Some(true), Some(true)).unwrap();
        queues::set_queue_extra(&conn, old, &serde_json::json!({"stored marker":"original"}))
            .unwrap();
        for file in fixture["initial"]["files"].as_array().unwrap() {
            let url = file["url"].as_str().unwrap().to_owned();
            queues::add_file_seeds(
                &conn,
                old,
                &[NewFileSeed {
                    seed_type: SeedType::Url,
                    data: url.clone(),
                    data_for_comparison: url,
                    source_time: None,
                    referral_url: None,
                    meta: queues::FileSeedMeta {
                        tags: ["source:recorded".into()].into(),
                        ..Default::default()
                    },
                }],
                false,
                100,
            )
            .unwrap();
            let mut seed = queues::file_seeds(&conn, old).unwrap().pop().unwrap();
            seed.status = SeedStatus::from_code(file["status"].as_i64().unwrap()).unwrap();
            seed.note = file["note"].as_str().unwrap().into();
            queues::update_file_seed(&conn, &seed).unwrap();
        }
        for gallery in fixture["initial"]["gallery"].as_array().unwrap() {
            queues::restore_gallery_seeds(
                &conn,
                old,
                &[queues::GallerySeed {
                    id: 0,
                    queue_id: old,
                    url: gallery["url"].as_str().unwrap().into(),
                    can_generate_more_pages: true,
                    created: 100,
                    modified: 100,
                    status: SeedStatus::from_code(gallery["status"].as_i64().unwrap()).unwrap(),
                    note: gallery["note"].as_str().unwrap().into(),
                    referral_url: None,
                    meta: Default::default(),
                }],
            )
            .unwrap();
        }
        let saved = Session {
            name: "importer history".into(),
            pages: vec![Page {
                key,
                name: "url history".into(),
                content: PageContent::Downloader {
                    kind: DownloaderKind::Urls,
                    queues: vec![old],
                    sort: None,
                    page: Some(Box::new(DownloaderPageState {
                        highlighted: Some(old),
                        ..Default::default()
                    })),
                },
            }],
        };
        assert_eq!(facts(&conn, old), fixture["initial"]);
        save(&conn, &saved, 100_000).unwrap();
        let mut seed = queues::file_seeds(&conn, old).unwrap()[0].clone();
        seed.status = SeedStatus::SuccessfulButRedundant;
        seed.note = "later state".into();
        queues::update_file_seed(&conn, &seed).unwrap();
        let later = fixture["source_changed"]["files"][2]["url"]
            .as_str()
            .unwrap();
        queues::add_file_seeds(
            &conn,
            old,
            &[NewFileSeed {
                seed_type: SeedType::Url,
                data: later.into(),
                data_for_comparison: later.into(),
                source_time: None,
                referral_url: None,
                meta: Default::default(),
            }],
            false,
            110,
        )
        .unwrap();
        save(&conn, &saved, 110_000).unwrap();
        queues::delete_queue(&conn, old).unwrap();
        let snapshot = load(&conn, &saved.name, 100_000).unwrap().unwrap();
        let first = restore_pages(&conn, snapshot.clone()).unwrap();
        let second = restore_pages(&conn, snapshot).unwrap();
        let restored = |pages: &[Page]| match &pages[0].content {
            PageContent::Downloader {
                queues,
                page: Some(state),
                ..
            } => {
                assert_eq!(state.highlighted, Some(queues[0]));
                queues[0]
            }
            _ => panic!("restored importer"),
        };
        let (first_id, second_id) = (restored(&first), restored(&second));
        assert_ne!(first_id, second_id);
        assert_eq!(facts(&conn, first_id), fixture["loaded_backup"]);
        assert_eq!(facts(&conn, second_id), fixture["second_copy"]);
        let copy = fixture["first_copy_changed"]["files"][2]["url"]
            .as_str()
            .unwrap();
        queues::add_file_seeds(
            &conn,
            first_id,
            &[NewFileSeed {
                seed_type: SeedType::Url,
                data: copy.into(),
                data_for_comparison: copy.into(),
                source_time: None,
                referral_url: None,
                meta: Default::default(),
            }],
            false,
            120,
        )
        .unwrap();
        assert_eq!(facts(&conn, first_id), fixture["first_copy_changed"]);
        assert_eq!(facts(&conn, second_id), fixture["second_copy"]);
        let restored_queue = queues::queue(&conn, first_id).unwrap().unwrap();
        assert_eq!(
            restored_queue.extra,
            serde_json::json!({"stored marker":"original"})
        );
        assert_eq!(restored_queue.page_key, Some(first[0].key.0.to_vec()));
        assert!(!restored_queue.page_closed);
        assert_eq!(
            queues::file_seeds(&conn, first_id).unwrap()[0].meta.tags,
            ["source:recorded".into()].into()
        );
    }
}

#[cfg(test)]
mod queue_kind_tests {
    use super::*;
    use hydrus_core::import_options::ImportOptionsSlice;
    use hydrus_core::pages::DownloaderKind;

    #[test]
    fn every_native_page_importer_restores_its_auxiliary_state_and_both_logs() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::schema::configure(&conn).unwrap();
        crate::schema::migrate(&mut conn).unwrap();
        for (kind, page_kind) in [
            (queues::QueueKind::Urls, DownloaderKind::Urls),
            (queues::QueueKind::Gallery, DownloaderKind::Gallery),
            (queues::QueueKind::Watcher, DownloaderKind::Watchers),
            (queues::QueueKind::LocalImport, DownloaderKind::Local),
            (queues::QueueKind::SimpleDownloader, DownloaderKind::Simple),
        ] {
            let old = queues::create_queue(
                &conn,
                kind,
                "stateful",
                None,
                &ImportOptionsSlice::default(),
                100,
            )
            .unwrap();
            let extra = serde_json::json!({"history":"original", "pending":["unchanged"]});
            queues::set_queue_extra(&conn, old, &extra).unwrap();
            queues::set_paused(&conn, old, Some(true), Some(false)).unwrap();
            let session = Session {
                name: "all kinds".into(),
                pages: vec![Page {
                    key: PageKey::random(),
                    name: "stateful".into(),
                    content: PageContent::Downloader {
                        kind: page_kind,
                        queues: vec![old],
                        sort: None,
                        page: None,
                    },
                }],
            };
            let snapshot = capture(&conn, &session).unwrap();
            let encoded = serde_json::to_string(&snapshot).unwrap();
            let decoded = serde_json::from_str(&encoded).unwrap();
            queues::set_queue_extra(&conn, old, &serde_json::json!({"history":"changed"})).unwrap();
            let pages = restore_pages(&conn, decoded).unwrap();
            let PageContent::Downloader { queues: ids, .. } = &pages[0].content else {
                panic!("importer")
            };
            let restored = queues::queue(&conn, ids[0]).unwrap().unwrap();
            assert_ne!(restored.id, old);
            assert_eq!(restored.kind, kind);
            assert_eq!(restored.extra, extra);
            assert!(restored.files_paused);
            assert!(!restored.gallery_paused);
            assert_eq!(restored.options, ImportOptionsSlice::default());
            assert_eq!(restored.created, 100);
            assert_eq!(restored.page_key, Some(pages[0].key.0.to_vec()));
        }
    }
}
