//! Bounded immutable saved-session snapshots. Trees, files and selection are
//! stored together so overwriting/deleting live pages cannot damage a backup.

use hydrus_core::HashId;
use hydrus_core::pages::{PageKey, Session};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::error::{Result, StoreError};
use crate::{sessions, settings};

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

/// Everything needed to restore one saved tree without consulting live pages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub session: Session,
    pub media: Vec<PageMedia>,
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

fn insert(conn: &Connection, session: &Session, timestamp: i64) -> Result<()> {
    let media = session
        .all_pages()
        .into_iter()
        .map(|page| {
            Ok(PageMedia {
                key: page.key,
                files: sessions::page_files(conn, &page.key)?,
                selected: sessions::page_selected(conn, &page.key)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let snapshot = Snapshot {
        session: session.clone(),
        media,
    };
    let data = serde_json::to_string(&snapshot).expect("session snapshot serialises");
    conn.execute(
        "INSERT INTO session_snapshots(name, timestamp_ms, data) VALUES (?, ?, ?)
        ON CONFLICT(name, timestamp_ms) DO UPDATE SET data = excluded.data",
        params![session.name, timestamp, data],
    )?;
    Ok(())
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
