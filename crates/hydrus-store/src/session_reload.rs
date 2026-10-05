//! Private durable snapshots for the GUI's asynchronous close-and-reload action.
//! They never share mutable page-media rows with the active or named sessions.
use crate::{Result, StoreError, session_backups::Snapshot};
use rusqlite::{Connection, params};

/// Persist an already-frozen reload snapshot under a unique private slot.
/// The slot is not a user saved session and does not create historical backups.
pub fn save(conn: &Connection, slot: &str, snapshot: &Snapshot) -> Result<()> {
    let data = serde_json::to_string(snapshot).expect("session snapshot serialises");
    conn.execute(
        "INSERT INTO session_snapshots(name, timestamp_ms, data) VALUES (?, 0, ?)",
        params![slot, data],
    )?;
    Ok(())
}

/// Read the committed snapshot and remove only its private slot atomically.
/// In particular, active page files, selections and importer queues remain intact.
pub fn take(conn: &Connection, slot: &str) -> Result<Snapshot> {
    let snapshot = crate::session_backups::load(conn, slot, 0)?
        .ok_or_else(|| StoreError::Corrupt("reload snapshot is missing".into()))?;
    remove(conn, slot)?;
    Ok(snapshot)
}

/// Remove a private slot after a read/worker failure without touching live media.
pub fn remove(conn: &Connection, slot: &str) -> Result<()> {
    conn.execute("DELETE FROM session_snapshots WHERE name = ?", [slot])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{session_backups::PageMedia, sessions};
    use hydrus_core::{
        HashId,
        pages::{Page, PageContent, PageKey, Session},
    };

    #[test]
    fn committed_reload_slot_is_independent_and_cleanup_keeps_active_media() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::schema::configure(&conn).unwrap();
        crate::schema::migrate(&mut conn).unwrap();
        let key = PageKey::random();
        let session = Session {
            name: sessions::LAST_SESSION.into(),
            pages: vec![Page {
                key,
                name: "owned snapshot".into(),
                content: PageContent::Pages(Vec::new()),
            }],
        };
        sessions::save(&conn, &session, 1).unwrap();
        sessions::set_page_files(&conn, &key, &[HashId(2), HashId(1)]).unwrap();
        sessions::set_page_selected(&conn, &key, &[HashId(1)]).unwrap();
        let snapshot = Snapshot {
            session,
            media: vec![PageMedia {
                key,
                files: vec![HashId(2), HashId(1)],
                selected: Vec::new(),
            }],
            queues: Vec::new(),
        };
        save(&conn, "private reload", &snapshot).unwrap();
        sessions::set_page_files(&conn, &key, &[HashId(3)]).unwrap();
        let restored = take(&conn, "private reload").unwrap();
        assert_eq!(restored.media[0].files, [HashId(2), HashId(1)]);
        assert!(restored.media[0].selected.is_empty());
        assert_eq!(sessions::page_files(&conn, &key).unwrap(), [HashId(3)]);
        assert_eq!(sessions::page_selected(&conn, &key).unwrap(), [HashId(1)]);
        assert_eq!(
            sessions::names(&conn).unwrap(),
            [(sessions::LAST_SESSION.into(), 1)]
        );
        assert!(
            crate::session_backups::load(&conn, "private reload", 0)
                .unwrap()
                .is_none()
        );
        assert!(take(&conn, "private reload").is_err());
        remove(&conn, "private reload").unwrap();
    }
}
