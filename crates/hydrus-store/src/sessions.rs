//! GUI sessions: named trees of pages ([`hydrus_core::pages`]), and the
//! files each page shows, kept by page key.
//!
//! As in the reference, the open pages are saved as the session
//! [`LAST_SESSION`], which the GUI opens with.

use rusqlite::{Connection, OptionalExtension, params};

use hydrus_core::HashId;
use hydrus_core::pages::{PageKey, Session};

use crate::error::{Result, StoreError};

/// The session the open pages are saved as.
pub const LAST_SESSION: &str = "last session";

/// Save a session, replacing any of the same name. Pages it no longer has
/// lose their files.
pub fn save(conn: &Connection, session: &Session, now: i64) -> Result<()> {
    if let Some(old) = load(conn, &session.name)? {
        let kept: std::collections::HashSet<PageKey> =
            session.all_pages().iter().map(|p| p.key).collect();
        for page in old.all_pages() {
            if !kept.contains(&page.key) {
                conn.execute(
                    "DELETE FROM page_files WHERE page_key = ?",
                    [&page.key.0[..]],
                )?;
            }
        }
    }
    let pages = serde_json::to_string(&session.pages).expect("pages serialise");
    conn.execute(
        "INSERT OR REPLACE INTO sessions (name, saved, pages) VALUES (?, ?, ?)",
        params![session.name, now, pages],
    )?;
    Ok(())
}

/// A session by name.
pub fn load(conn: &Connection, name: &str) -> Result<Option<Session>> {
    let pages: Option<String> = conn
        .query_row("SELECT pages FROM sessions WHERE name = ?", [name], |r| {
            r.get(0)
        })
        .optional()?;
    pages
        .map(|pages| {
            Ok(Session {
                name: name.to_owned(),
                pages: serde_json::from_str(&pages)
                    .map_err(|e| StoreError::Corrupt(format!("session \"{name}\": {e}")))?,
            })
        })
        .transpose()
}

/// Every session's name and when it was saved, by name.
pub fn names(conn: &Connection) -> Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare("SELECT name, saved FROM sessions ORDER BY name")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Delete a session and its pages' files.
pub fn delete(conn: &Connection, name: &str) -> Result<()> {
    if let Some(session) = load(conn, name)? {
        for page in session.all_pages() {
            conn.execute(
                "DELETE FROM page_files WHERE page_key = ?",
                [&page.key.0[..]],
            )?;
        }
    }
    conn.execute("DELETE FROM sessions WHERE name = ?", [name])?;
    Ok(())
}

/// Set the files a page shows, in order.
pub fn set_page_files(conn: &Connection, page: &PageKey, files: &[HashId]) -> Result<()> {
    let packed: Vec<u8> = files.iter().flat_map(|h| h.0.to_le_bytes()).collect();
    conn.execute(
        "INSERT OR REPLACE INTO page_files (page_key, hash_ids) VALUES (?, ?)",
        params![&page.0[..], packed],
    )?;
    Ok(())
}

/// The files a page shows, in order (none if it has none saved).
pub fn page_files(conn: &Connection, page: &PageKey) -> Result<Vec<HashId>> {
    let packed: Option<Vec<u8>> = conn
        .query_row(
            "SELECT hash_ids FROM page_files WHERE page_key = ?",
            [&page.0[..]],
            |r| r.get(0),
        )
        .optional()?;
    Ok(packed
        .unwrap_or_default()
        .chunks_exact(4)
        .map(|b| HashId(u32::from_le_bytes(b.try_into().expect("4 bytes"))))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydrus_core::pages::{Page, PageContent};
    use hydrus_core::search::context::FileSearchContext;

    fn search_page(name: &str) -> Page {
        Page {
            key: PageKey::random(),
            name: name.into(),
            content: PageContent::Search {
                search: FileSearchContext::default(),
                synchronised: true,
                sort: None,
            },
        }
    }

    #[test]
    fn sessions_keep_their_pages_and_files() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::schema::configure(&conn).unwrap();
        crate::schema::migrate(&mut conn).unwrap();
        let (a, b) = (search_page("a"), search_page("b"));
        let mut session = Session {
            name: LAST_SESSION.into(),
            pages: vec![a.clone(), b.clone()],
        };
        save(&conn, &session, 100).unwrap();
        set_page_files(&conn, &a.key, &[HashId(3), HashId(1), HashId(2)]).unwrap();
        set_page_files(&conn, &b.key, &[HashId(5)]).unwrap();
        assert_eq!(load(&conn, LAST_SESSION).unwrap().as_ref(), Some(&session));
        assert_eq!(
            page_files(&conn, &a.key).unwrap(),
            [HashId(3), HashId(1), HashId(2)]
        );
        assert_eq!(names(&conn).unwrap(), [(LAST_SESSION.to_owned(), 100)]);

        // a closed page's files go with it
        session.pages.remove(1);
        save(&conn, &session, 200).unwrap();
        assert!(page_files(&conn, &b.key).unwrap().is_empty());
        assert_eq!(page_files(&conn, &a.key).unwrap().len(), 3);

        delete(&conn, LAST_SESSION).unwrap();
        assert!(load(&conn, LAST_SESSION).unwrap().is_none());
        assert!(page_files(&conn, &a.key).unwrap().is_empty());
    }
}
