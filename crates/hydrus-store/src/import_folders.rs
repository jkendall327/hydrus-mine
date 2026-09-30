//! Import folders, stored as import queues of kind
//! [`QueueKind::ImportFolder`]: the queue's name, own import options and
//! files-paused flag are the folder's, its file seeds are the files the
//! folder has seen, and its extra is the folder's [`ImportFolderSettings`].

use rusqlite::Connection;

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_parse::folders::ImportFolderSettings;

use crate::error::{Result, StoreError};
use crate::queues::{self, Queue, QueueKind};

/// An import folder.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportFolder {
    pub queue: Queue,
    pub settings: ImportFolderSettings,
}

impl ImportFolder {
    pub fn id(&self) -> i64 {
        self.queue.id
    }

    pub fn name(&self) -> &str {
        &self.queue.name
    }

    pub fn paused(&self) -> bool {
        self.queue.files_paused
    }
}

fn settings_of(queue: &Queue) -> Result<ImportFolderSettings> {
    serde_json::from_value(queue.extra.clone())
        .map_err(|e| StoreError::Corrupt(format!("import folder {:?} settings: {e}", queue.name)))
}

/// Add an import folder; `Ok(None)` if the name is taken.
pub fn create_import_folder(
    conn: &Connection,
    name: &str,
    settings: &ImportFolderSettings,
    options: &ImportOptionsSlice,
    paused: bool,
    now: i64,
) -> Result<Option<i64>> {
    if queues::find_queue(conn, QueueKind::ImportFolder, Some(name), None)?.is_some() {
        return Ok(None);
    }
    let id = queues::create_queue(conn, QueueKind::ImportFolder, name, None, options, now)?;
    set_settings(conn, id, settings)?;
    queues::set_paused(conn, id, Some(paused), None)?;
    Ok(Some(id))
}

pub fn set_settings(conn: &Connection, id: i64, settings: &ImportFolderSettings) -> Result<()> {
    let value = serde_json::to_value(settings).expect("plain data serialises");
    queues::set_queue_extra(conn, id, &value)
}

/// Every import folder, in the order they were made.
pub fn import_folders(conn: &Connection) -> Result<Vec<ImportFolder>> {
    queues::queues(conn, Some(QueueKind::ImportFolder))?
        .into_iter()
        .map(|queue| {
            Ok(ImportFolder {
                settings: settings_of(&queue)?,
                queue,
            })
        })
        .collect()
}

pub fn import_folder(conn: &Connection, id: i64) -> Result<Option<ImportFolder>> {
    let Some(queue) = queues::queue(conn, id)? else {
        return Ok(None);
    };
    if queue.kind != QueueKind::ImportFolder {
        return Ok(None);
    }
    Ok(Some(ImportFolder {
        settings: settings_of(&queue)?,
        queue,
    }))
}

pub fn find_import_folder(conn: &Connection, name: &str) -> Result<Option<ImportFolder>> {
    match queues::find_queue(conn, QueueKind::ImportFolder, Some(name), None)? {
        None => Ok(None),
        Some(queue) => Ok(Some(ImportFolder {
            settings: settings_of(&queue)?,
            queue,
        })),
    }
}
