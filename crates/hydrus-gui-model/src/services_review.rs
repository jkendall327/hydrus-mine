//! Service review facts read from one consistent native database snapshot.

use hydrus_core::numbers::human_bytes;

use hydrus_core::{ServiceId, ServiceKey};
use hydrus_store::services::{ServiceKind, ServiceRegistry};
use hydrus_store::{Store, error::Result};
use rusqlite::Connection;

/// Immediate local maintenance choices, committed only after an owned confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ClearTrash,
    UndeleteTrash,
    ClearDeletedRecords,
    ClearRatings(hydrus_store::content::RatingClearScope),
}
impl Action {
    /// Exact reference confirmation for the selected service operation.
    pub fn question(self) -> String {
        match self {
            Self::ClearTrash => "This will completely clear your trash of all its files, deleting them permanently from the client. This operation cannot be undone.\n\nIf you have many files in your trash, it will take some time to complete and for all the files to eventually be deleted.".into(),
            Self::UndeleteTrash => "This will instruct your database to restore all files currently in the trash to all the local file domains they have been in.".into(),
            Self::ClearDeletedRecords => "This will instruct your database to forget its _entire_ record of locally deleted files, meaning that if it ever encounters any of those files again, it will assume they are new and reimport them. This operation cannot be undone.".into(),
            Self::ClearRatings(scope) => {
                let population = match scope {
                    hydrus_store::content::RatingClearScope::Deleted => "deleted files",
                    hydrus_store::content::RatingClearScope::NonLocal => "non-local files",
                    hydrus_store::content::RatingClearScope::All => "ALL FILES",
                };
                format!("Delete any ratings on this service for {population}? THIS CANNOT BE UNDONE\n\nPlease note a client restart is needed to see the ratings disappear in media views.")
            }
        }
    }
    /// Advanced deleted-record clearing needs this second explicit confirmation.
    pub fn second_question(self) -> Option<&'static str> {
        (self == Self::ClearDeletedRecords).then_some("Hey, I am just going to ask again--are you _absolutely_ sure? This is an advanced action that may mess up your downloads/imports in future.")
    }
    /// Decode the native menu's fixed action values at its boundary.
    pub fn from_index(index: i32) -> Option<Self> {
        use hydrus_store::content::RatingClearScope;
        match index {
            0 => Some(Self::ClearTrash),
            1 => Some(Self::UndeleteTrash),
            2 => Some(Self::ClearRatings(RatingClearScope::Deleted)),
            3 => Some(Self::ClearRatings(RatingClearScope::NonLocal)),
            4 => Some(Self::ClearRatings(RatingClearScope::All)),
            5 => Some(Self::ClearDeletedRecords),
            _ => None,
        }
    }
}

/// Recheck the selected service kind inside the content transaction before maintenance.
pub fn apply(store: &Store, key: ServiceKey, action: Action) -> Result<()> {
    store.write_content(move |writer| {
        let service = writer.snapshot().services.by_key(&key)?;
        let id = service.id;
        match (&service.kind, action) {
            (ServiceKind::Trash, Action::ClearTrash) => writer.clear_trash(),
            (ServiceKind::Trash, Action::UndeleteTrash) => writer.undelete_trash(),
            (ServiceKind::LocalFileStorage, Action::ClearDeletedRecords) => {
                writer.clear_local_delete_records(None)
            }
            (
                ServiceKind::RatingLike(_)
                | ServiceKind::RatingNumerical(_)
                | ServiceKind::RatingIncDec(_),
                Action::ClearRatings(scope),
            ) => {
                writer.clear_ratings(id, scope)?;
                Ok(())
            }
            _ => Err(hydrus_store::error::StoreError::Invalid(
                "maintenance does not match this local service".into(),
            )),
        }
    })
}

fn human_int(n: i64) -> String {
    hydrus_core::numbers::human_int(u64::try_from(n).unwrap_or(0))
}

/// One service in the review window, including its current native statistics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: ServiceId,
    pub key: ServiceKey,
    pub name: String,
    pub service_type: String,
    pub statistics: String,
    pub unavailable: String,
    pub actions: Vec<Action>,
}

fn count(conn: &Connection, table: &str, id: ServiceId) -> Result<i64> {
    Ok(conn.query_row(
        &format!("SELECT count(*) FROM {table} WHERE service_id = ?"),
        [id],
        |r| r.get(0),
    )?)
}

fn statistics(
    conn: &Connection,
    id: ServiceId,
    kind: &ServiceKind,
    graph: &hydrus_store::display::DisplayGraph,
) -> Result<String> {
    Ok(match kind {
        ServiceKind::LocalFiles
        | ServiceKind::LocalUpdates
        | ServiceKind::Trash
        | ServiceKind::LocalFileStorage
        | ServiceKind::CombinedLocalMedia
        | ServiceKind::FileRepository(_) => {
            let (files, size): (i64, i64) = conn.query_row(
                "SELECT count(*), coalesce(sum(coalesce(f.size, 0)), 0) FROM file_domain_current d LEFT JOIN files f USING(hash_id) WHERE d.service_id = ?",
                [id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            let mut text = format!(
                "{} files, totalling {}",
                human_int(files),
                human_bytes(u64::try_from(size).unwrap_or(0))
            );
            if !matches!(kind, ServiceKind::LocalUpdates | ServiceKind::Trash) {
                text.push_str(&format!(
                    " - {} deleted files",
                    human_int(count(conn, "file_domain_deleted", id)?)
                ));
            }
            text
        }
        ServiceKind::LocalTags | ServiceKind::TagRepository(_) => {
            let table = hydrus_store::schema::MappingTables::new(id);
            let (mappings, files): (i64, i64) = conn.query_row(
                &format!(
                    "SELECT count(*), count(DISTINCT hash_id) FROM {}",
                    table.current
                ),
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let mut tags = graph.all_tags();
            tags.extend(
                conn.prepare(&format!(
                    "SELECT tag_id FROM {} UNION SELECT tag_id FROM {}",
                    table.current, table.pending
                ))?
                .query_map([], |r| r.get::<_, hydrus_core::TagId>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?,
            );
            format!(
                "{} total mappings involving {} different tags on {} different files",
                human_int(mappings),
                hydrus_core::numbers::human_int(u64::try_from(tags.len()).unwrap_or(u64::MAX)),
                human_int(files)
            )
        }
        ServiceKind::RatingLike(_) | ServiceKind::RatingNumerical(_) => {
            format!("{} files are rated", human_int(count(conn, "ratings", id)?))
        }
        ServiceKind::RatingIncDec(_) => format!(
            "{} files are rated",
            human_int(count(conn, "ratings_incdec", id)?)
        ),
        ServiceKind::LocalNotes => {
            let files: i64 =
                conn.query_row("SELECT count(DISTINCT hash_id) FROM file_notes", [], |r| {
                    r.get(0)
                })?;
            format!("{} files have notes", human_int(files))
        }
        _ => String::new(),
    })
}

/// Read names, types and counts together, so a concurrent service change cannot mix them.
pub fn rows(store: &Store) -> Result<Vec<Row>> {
    store.read(|conn| {
        let registry = ServiceRegistry::load(conn)?;
        let graphs = hydrus_store::display::DisplayGraphs::load(conn,&registry)?;
        let mut rows = Vec::new();
        for service in registry.all() {
            let unavailable = match &service.kind {
                ServiceKind::TagRepository(_) | ServiceKind::FileRepository(_) | ServiceKind::Ipfs(_) => "Repository synchronisation, IPFS and account administration are not available yet.",
                ServiceKind::ClientApi(_) => "API request registration uses hydrus api-keys listen; remote account controls are not available yet.",
                _ => "",
            };
            let actions = match service.kind {
                ServiceKind::LocalFileStorage => vec![Action::ClearDeletedRecords],
                ServiceKind::Trash if count(conn, "file_domain_current", service.id)? > 0 => vec![Action::ClearTrash, Action::UndeleteTrash],
                ServiceKind::RatingLike(_) | ServiceKind::RatingNumerical(_) | ServiceKind::RatingIncDec(_) => (2..=4).filter_map(Action::from_index).collect(),
                _ => Vec::new(),
            };
            rows.push(Row { actions, id: service.id, key: service.key.clone(), name: service.name.clone(), service_type: service.service_type().name().into(), statistics: statistics(conn, service.id, &service.kind, &graphs.get(service.id))?, unavailable: unavailable.into() });
        }
        rows.sort_by(|a,b| a.service_type.cmp(&b.service_type).then_with(|| a.name.cmp(&b.name)));
        Ok(rows)
    })
}
