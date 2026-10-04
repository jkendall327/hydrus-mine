//! GUI sessions: named trees of pages ([`hydrus_core::pages`]), and the
//! files each page shows, kept by page key.
//!
//! As in the reference, the open pages are saved as the session
//! [`LAST_SESSION`], which the GUI opens with. The GUI keeps it up to date
//! as its pages change (with the page shown and each page's selection), so
//! the Client API's `/manage_pages` answers from it; what the API asks of
//! the pages goes to the GUI as [`PageCommand`]s, or, with no GUI open, is
//! done to the session here ([`apply_command`]).

use rusqlite::{Connection, OptionalExtension, params};

use hydrus_core::HashId;
use hydrus_core::pages::{PageKey, Session};

use crate::error::{Result, StoreError};

/// Notebook workflow preferences read by the close/send consumers.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct NotebookSettings {
    /// Select the tab on the left when closing the selected tab.
    pub close_focus_left: bool,
    /// Ask the new notebook's name after sending pages into it.
    pub rename_sent_notebooks: bool,
}
impl crate::settings::Setting for NotebookSettings {
    const KEY: &'static str = "gui_notebooks";
}

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
    // (a new session's top notebook gets its key; an old one keeps its key
    // and the page it shows)
    conn.execute(
        "INSERT INTO sessions (name, saved, pages, top_key) VALUES (?, ?, ?, randomblob(32))
         ON CONFLICT (name) DO UPDATE SET saved = excluded.saved, pages = excluded.pages",
        params![session.name, now, pages],
    )?;
    Ok(())
}

/// The key of a session's top notebook (the Client API's "top page
/// notebook"), kept as long as the session is.
pub fn top_key(conn: &Connection, name: &str) -> Result<Option<PageKey>> {
    let key: Option<Option<Vec<u8>>> = conn
        .query_row("SELECT top_key FROM sessions WHERE name = ?", [name], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(key.flatten().and_then(|k| k.try_into().ok().map(PageKey)))
}

/// The page a session shows: the deepest on the way to it (`None`: each
/// notebook's first).
pub fn shown(conn: &Connection, name: &str) -> Result<Option<PageKey>> {
    let key: Option<Option<Vec<u8>>> = conn
        .query_row("SELECT shown FROM sessions WHERE name = ?", [name], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(key.flatten().and_then(|k| k.try_into().ok().map(PageKey)))
}

/// Set the page a session shows.
pub fn set_shown(conn: &Connection, name: &str, page: Option<&PageKey>) -> Result<()> {
    conn.execute(
        "UPDATE sessions SET shown = ? WHERE name = ?",
        params![page.map(|p| &p.0[..]), name],
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
    crate::session_backups::delete(conn, name)?;
    conn.execute("DELETE FROM sessions WHERE name = ?", [name])?;
    Ok(())
}

fn pack(files: &[HashId]) -> Vec<u8> {
    files.iter().flat_map(|h| h.0.to_le_bytes()).collect()
}

fn unpack(packed: &[u8]) -> Vec<HashId> {
    packed
        .chunks_exact(4)
        .map(|b| HashId(u32::from_le_bytes(b.try_into().expect("4 bytes"))))
        .collect()
}

/// Set the files a page shows, in order.
pub fn set_page_files(conn: &Connection, page: &PageKey, files: &[HashId]) -> Result<()> {
    conn.execute(
        "INSERT INTO page_files (page_key, hash_ids) VALUES (?, ?)
         ON CONFLICT (page_key) DO UPDATE SET hash_ids = excluded.hash_ids",
        params![&page.0[..], pack(files)],
    )?;
    Ok(())
}

/// How many files each page with any kept shows.
pub fn page_file_counts(conn: &Connection) -> Result<std::collections::HashMap<PageKey, usize>> {
    let mut stmt = conn.prepare("SELECT page_key, length(hash_ids) / 4 FROM page_files")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?)))?;
    let mut counts = std::collections::HashMap::new();
    for row in rows {
        let (key, n) = row?;
        if let Ok(key) = key.try_into() {
            counts.insert(PageKey(key), usize::try_from(n).unwrap_or(0));
        }
    }
    Ok(counts)
}

/// Set the files selected on a page, in the page's order.
pub fn set_page_selected(conn: &Connection, page: &PageKey, files: &[HashId]) -> Result<()> {
    conn.execute(
        "INSERT INTO page_files (page_key, hash_ids, selected) VALUES (?, X'', ?)
         ON CONFLICT (page_key) DO UPDATE SET selected = excluded.selected",
        params![&page.0[..], pack(files)],
    )?;
    Ok(())
}

/// The files selected on a page, in the page's order.
pub fn page_selected(conn: &Connection, page: &PageKey) -> Result<Vec<HashId>> {
    let packed: Option<Vec<u8>> = conn
        .query_row(
            "SELECT selected FROM page_files WHERE page_key = ?",
            [&page.0[..]],
            |r| r.get(0),
        )
        .optional()?;
    Ok(unpack(&packed.unwrap_or_default()))
}

/// What the Client API asks of a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageCommand {
    /// Show it (`/manage_pages/focus_page`).
    Focus,
    /// Add these files at its end, those it doesn't have, in order
    /// (`/manage_pages/add_files`).
    AddFiles(Vec<HashId>),
    /// Search again (`/manage_pages/refresh_page`).
    Refresh,
}

/// Ask the open GUI to do something to a page.
pub fn push_command(conn: &Connection, page: &PageKey, command: &PageCommand) -> Result<()> {
    let (name, files) = match command {
        PageCommand::Focus => ("focus", Vec::new()),
        PageCommand::AddFiles(files) => ("add_files", pack(files)),
        PageCommand::Refresh => ("refresh", Vec::new()),
    };
    conn.execute(
        "INSERT INTO page_commands (page_key, command, hash_ids) VALUES (?, ?, ?)",
        params![&page.0[..], name, files],
    )?;
    Ok(())
}

/// What has been asked of the GUI's pages, in order, taken off the queue.
pub fn take_commands(conn: &Connection) -> Result<Vec<(PageKey, PageCommand)>> {
    let mut stmt =
        conn.prepare("SELECT page_key, command, hash_ids FROM page_commands ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Vec<u8>>(2)?,
        ))
    })?;
    let mut commands = Vec::new();
    for row in rows {
        let (key, name, files) = row?;
        let Ok(key) = key.try_into().map(PageKey) else {
            continue;
        };
        let command = match name.as_str() {
            "focus" => PageCommand::Focus,
            "add_files" => PageCommand::AddFiles(unpack(&files)),
            "refresh" => PageCommand::Refresh,
            _ => continue,
        };
        commands.push((key, command));
    }
    conn.execute("DELETE FROM page_commands", [])?;
    Ok(commands)
}

/// Do what the Client API asks of a page of the last session to the session
/// itself, as the GUI would, for when no GUI is open: files join the page's
/// end; a focused page is the one the GUI opens on; a refresh has nothing
/// to do until the page is open.
pub fn apply_command(conn: &Connection, page: &PageKey, command: &PageCommand) -> Result<()> {
    match command {
        PageCommand::Focus => set_shown(conn, LAST_SESSION, Some(page)),
        PageCommand::AddFiles(files) => {
            let mut shown = page_files(conn, page)?;
            let mut have: std::collections::HashSet<HashId> = shown.iter().copied().collect();
            shown.extend(files.iter().filter(|f| have.insert(**f)));
            set_page_files(conn, page, &shown)
        }
        PageCommand::Refresh => Ok(()),
    }
}

/// A media viewer the GUI has open, as the Client API lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaViewer {
    /// Random, for as long as it is open.
    pub canvas_key: [u8; 32],
    /// The reference's canvas type (`CANVAS_MEDIA_VIEWER` is 0).
    pub canvas_type: i64,
    /// The file it shows.
    pub file: Option<HashId>,
}

/// Set the GUI's open media viewers, in the order they opened.
pub fn set_media_viewers(conn: &Connection, viewers: &[MediaViewer]) -> Result<()> {
    conn.execute("DELETE FROM media_viewers", [])?;
    for (position, viewer) in viewers.iter().enumerate() {
        conn.execute(
            "INSERT INTO media_viewers (position, canvas_key, canvas_type, hash_id)
             VALUES (?, ?, ?, ?)",
            params![
                i64::try_from(position).unwrap_or(i64::MAX),
                &viewer.canvas_key[..],
                viewer.canvas_type,
                viewer.file.map(|f| f.0)
            ],
        )?;
    }
    Ok(())
}

/// The GUI's open media viewers, as it last set them.
pub fn media_viewers(conn: &Connection) -> Result<Vec<MediaViewer>> {
    let mut stmt = conn
        .prepare("SELECT canvas_key, canvas_type, hash_id FROM media_viewers ORDER BY position")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Option<u32>>(2)?,
        ))
    })?;
    let mut viewers = Vec::new();
    for row in rows {
        let (key, canvas_type, file) = row?;
        if let Ok(canvas_key) = key.try_into() {
            viewers.push(MediaViewer {
                canvas_key,
                canvas_type,
                file: file.map(HashId),
            });
        }
    }
    Ok(viewers)
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
    Ok(unpack(&packed.unwrap_or_default()))
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
                lock: None,
                collect: None,
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

    #[test]
    fn saving_keeps_the_top_key_the_page_shown_and_selections() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::schema::configure(&conn).unwrap();
        crate::schema::migrate(&mut conn).unwrap();
        let (a, b) = (search_page("a"), search_page("b"));
        let session = Session {
            name: LAST_SESSION.into(),
            pages: vec![a.clone(), b.clone()],
        };
        assert_eq!(top_key(&conn, LAST_SESSION).unwrap(), None);
        save(&conn, &session, 100).unwrap();
        let top = top_key(&conn, LAST_SESSION).unwrap().expect("a top key");
        assert_eq!(shown(&conn, LAST_SESSION).unwrap(), None);
        set_shown(&conn, LAST_SESSION, Some(&b.key)).unwrap();
        set_page_files(&conn, &a.key, &[HashId(3), HashId(1)]).unwrap();
        set_page_selected(&conn, &a.key, &[HashId(1)]).unwrap();
        // (saving again, and the files again, changes none of them)
        save(&conn, &session, 200).unwrap();
        set_page_files(&conn, &a.key, &[HashId(3), HashId(1), HashId(2)]).unwrap();
        assert_eq!(top_key(&conn, LAST_SESSION).unwrap(), Some(top));
        assert_eq!(shown(&conn, LAST_SESSION).unwrap(), Some(b.key));
        assert_eq!(page_selected(&conn, &a.key).unwrap(), [HashId(1)]);
        assert_eq!(page_files(&conn, &a.key).unwrap().len(), 3);
        // (a selection alone, before any files)
        set_page_selected(&conn, &b.key, &[HashId(5)]).unwrap();
        assert_eq!(page_selected(&conn, &b.key).unwrap(), [HashId(5)]);
        assert!(page_files(&conn, &b.key).unwrap().is_empty());
    }

    #[test]
    fn commands_queue_for_the_gui_or_change_the_session() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::schema::configure(&conn).unwrap();
        crate::schema::migrate(&mut conn).unwrap();
        let (a, b) = (search_page("a"), search_page("b"));
        save(
            &conn,
            &Session {
                name: LAST_SESSION.into(),
                pages: vec![a.clone(), b.clone()],
            },
            100,
        )
        .unwrap();
        let add = PageCommand::AddFiles(vec![HashId(2), HashId(1), HashId(2)]);
        push_command(&conn, &a.key, &add).unwrap();
        push_command(&conn, &b.key, &PageCommand::Focus).unwrap();
        push_command(&conn, &a.key, &PageCommand::Refresh).unwrap();
        assert_eq!(
            take_commands(&conn).unwrap(),
            [
                (a.key, add.clone()),
                (b.key, PageCommand::Focus),
                (a.key, PageCommand::Refresh)
            ]
        );
        assert!(take_commands(&conn).unwrap().is_empty());

        // with no GUI: files join the end once each, and the focused page
        // is the one shown
        set_page_files(&conn, &a.key, &[HashId(1), HashId(3)]).unwrap();
        apply_command(&conn, &a.key, &add).unwrap();
        assert_eq!(
            page_files(&conn, &a.key).unwrap(),
            [HashId(1), HashId(3), HashId(2)]
        );
        apply_command(&conn, &b.key, &PageCommand::Focus).unwrap();
        assert_eq!(shown(&conn, LAST_SESSION).unwrap(), Some(b.key));
        apply_command(&conn, &a.key, &PageCommand::Refresh).unwrap();
        assert_eq!(page_files(&conn, &a.key).unwrap().len(), 3);
    }

    #[test]
    fn media_viewers_are_kept_in_order() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::schema::configure(&conn).unwrap();
        crate::schema::migrate(&mut conn).unwrap();
        let viewers = [
            MediaViewer {
                canvas_key: [2; 32],
                canvas_type: 0,
                file: Some(HashId(7)),
            },
            MediaViewer {
                canvas_key: [1; 32],
                canvas_type: 0,
                file: None,
            },
        ];
        set_media_viewers(&conn, &viewers).unwrap();
        assert_eq!(media_viewers(&conn).unwrap(), viewers);
        set_media_viewers(&conn, &viewers[1..]).unwrap();
        assert_eq!(media_viewers(&conn).unwrap(), viewers[1..]);
    }
}
