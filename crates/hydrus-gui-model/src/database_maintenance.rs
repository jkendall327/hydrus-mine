//! The Database menu's maintenance entries (`_InitialiseMenuInfoDatabase`'s
//! "db maintenance", "check and repair" and "regenerate" submenus): what
//! each asks, in the reference's words and buttons, which tag service it
//! then asks for, and the native job it runs with the popups it sends.
//! Recorded by `oracle/record_database_maintenance.py`.
//!
//! The native store has no local hashes, local tags or service info caches,
//! no separate mappings cache to repopulate from and no hashed serialisables;
//! those entries stay disabled (DIFFERENCES.md).

use hydrus_core::ServiceKey;
use hydrus_store::Store;
use hydrus_store::db_maintenance::{self as db, Definition, SubtagIndex, TagScope};
use hydrus_store::popups::{self, Job as Popup};

/// A maintenance entry hydrus-rs runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    TagStorage,
    TagStoragePending,
    TagDisplay,
    TagDisplayPending,
    TagDisplayRepopulate,
    SiblingsLookup,
    ParentsLookup,
    TagText,
    TagTextSubtags,
    TagTextSearchable,
    FixInvalidTags,
    FixInconsistentMappings,
    ResyncTagCounts,
    Analyze,
    OrphanFileRecords,
    OrphanUrlMappings,
    OrphanTables,
    TablesUsingDefinitions,
}

/// How a job's first question is asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asking {
    /// `GetYesNo`.
    YesNo { yes: &'static str, no: &'static str },
    /// `GetYesYesNo`: analyze's soft and full.
    YesYesNo {
        yes: [&'static str; 2],
        no: &'static str,
    },
    /// `SelectFromListButtons`, with this title.
    Buttons {
        title: &'static str,
        choices: Vec<(String, String)>,
    },
}

/// The "Which service?" chooser's title (`GetTagServiceKeyForMaintenance`).
pub const WHICH_SERVICE: &str = "Which service?";

/// What the user chose, besides yes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Answer {
    /// The tag service (none: all services).
    pub service: Option<ServiceKey>,
    /// Analyze's "full" (else "soft").
    pub full: bool,
    /// Tables using tag ids (else hash ids).
    pub tag_definitions: bool,
}

/// What the job leaves the caller to do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outcome {
    /// Text to put on the clipboard.
    pub clipboard: Option<String>,
}

impl Job {
    /// The entries, in their menus' order.
    pub const ALL: [Job; 18] = [
        Job::Analyze,
        Job::OrphanFileRecords,
        Job::OrphanUrlMappings,
        Job::OrphanTables,
        Job::TablesUsingDefinitions,
        Job::FixInvalidTags,
        Job::FixInconsistentMappings,
        Job::ResyncTagCounts,
        Job::TagStorage,
        Job::TagStoragePending,
        Job::TagDisplay,
        Job::TagDisplayPending,
        Job::TagDisplayRepopulate,
        Job::SiblingsLookup,
        Job::ParentsLookup,
        Job::TagText,
        Job::TagTextSubtags,
        Job::TagTextSearchable,
    ];

    /// The job of a Database menu entry's label (without its ellipsis).
    pub fn from_label(label: &str) -> Option<Job> {
        Self::ALL.into_iter().find(|job| job.label() == label)
    }

    /// Whether, after yes, it asks which tag service.
    pub const fn chooses_tag_service(self) -> bool {
        matches!(
            self,
            Self::TagStorage
                | Self::TagStoragePending
                | Self::TagDisplay
                | Self::TagDisplayPending
                | Self::TagDisplayRepopulate
                | Self::TagText
                | Self::TagTextSubtags
                | Self::TagTextSearchable
                | Self::FixInconsistentMappings
                | Self::ResyncTagCounts
        )
    }

    /// How its first question is asked.
    pub fn asking(self) -> Asking {
        match self {
            Self::Analyze => Asking::YesYesNo {
                yes: ["soft", "full"],
                no: "forget it",
            },
            Self::TablesUsingDefinitions => Asking::Buttons {
                title: "Select which content type to fetch for",
                choices: vec![("hash".into(), "hash".into()), ("tag".into(), "tag".into())],
            },
            _ => Asking::YesNo {
                yes: self.yes_label(),
                no: "forget it",
            },
        }
    }

    /// The menu entry's label (without its ellipsis).
    pub const fn label(self) -> &'static str {
        match self {
            Self::TagStorage => {
                "tag storage mappings cache (all, with deferred siblings & parents calculation)"
            }
            Self::TagStoragePending => {
                "tag storage mappings cache (just pending tags, instant calculation)"
            }
            Self::TagDisplay => {
                "tag display mappings cache (all, deferred siblings & parents calculation)"
            }
            Self::TagDisplayPending => {
                "tag display mappings cache (just pending tags, instant calculation)"
            }
            Self::TagDisplayRepopulate => "tag display mappings cache (missing file repopulation)",
            Self::SiblingsLookup => "tag siblings lookup cache",
            Self::ParentsLookup => "tag parents lookup cache",
            Self::TagText => "tag text search cache",
            Self::TagTextSubtags => "tag text search cache (subtags repopulation)",
            Self::TagTextSearchable => "tag text search cache (searchable subtag maps)",
            Self::FixInvalidTags => "fix invalid tags",
            Self::FixInconsistentMappings => "fix logically inconsistent mappings",
            Self::ResyncTagCounts => "resync tag mappings cache files",
            Self::Analyze => "analyze",
            Self::OrphanFileRecords => "clear/fix orphan file records",
            Self::OrphanUrlMappings => "clear orphan URL mappings",
            Self::OrphanTables => "clear orphan tables",
            Self::TablesUsingDefinitions => "get tables using definitions",
        }
    }

    /// What it asks first.
    pub const fn question(self) -> &'static str {
        match self {
            Self::TagStorage => {
                "WARNING: Do not run this for no reason! On a large database, this could take hours to finish!\n\nThis will delete and then recreate the entire tag 'storage' mappings cache, which is used for tag calculation based on actual values and autocomplete counts in editing contexts like _manage tags_. This is useful if miscounting has somehow occurred.\n\nIf you have a lot of tags and files, it can take a long time, during which the gui may hang. It necessarily involves a regeneration of the tag display mappings cache, which relies on the storage cache, and the tag text search cache. All siblings and parents will have to be resynced.\n\nIf you do not have a specific reason to run this, it is pointless."
            }
            Self::TagStoragePending => {
                "This will delete and then recreate the pending tags on the whole tag mappings cache, which is used for multiple kinds of tag searching, loading, and autocomplete counts. This is useful if you have 'ghost' pending tags or counts hanging around.\n\nIf you have a millions of tags, pending or current, it can take a long time, during which the gui may hang.\n\nIf you do not have a specific reason to run this, it is pointless."
            }
            Self::TagDisplay => {
                "This will delete and then recreate the tag 'display' mappings cache, which is used for user-presented tag searching, loading, and autocomplete counts. This is useful if miscounting (particularly related to siblings/parents) has somehow occurred.\n\nIf you have a lot of tags and files, it can take a long time, during which the gui may hang. All siblings and parents will have to be resynced.\n\nIf you do not have a specific reason to run this, it is pointless."
            }
            Self::TagDisplayPending => {
                "This will delete and then recreate the pending tags on the tag 'display' mappings cache, which is used for user-presented tag searching, loading, and autocomplete counts. This is useful if you have 'ghost' pending tags or counts hanging around.\n\nIf you have a millions of tags, pending or current, it can take a long time, during which the gui may hang.\n\nIf you do not have a specific reason to run this, it is pointless."
            }
            Self::TagDisplayRepopulate => {
                "This will go through your mappings cache and fill in any missing files. It is radically faster than a full regen, and adds siblings and parents instantly, but it only solves the problem of missing file rows.\n\nIf you have a millions of tags, pending or current, it can take a long time, during which the gui may hang.\n\nIf you do not have a specific reason to run this, it is pointless."
            }
            Self::SiblingsLookup => {
                "This will delete and then recreate the tag siblings lookup cache, which is used for all basic tag sibling operations. This is useful if it has become damaged or otherwise desynchronised.\n\nIt should only take a second or two. It necessarily involves a regeneration of the tag parents lookup cache.\n\nIf you do not have a specific reason to run this, it is pointless."
            }
            Self::ParentsLookup => {
                "This will delete and then recreate the tag parents lookup cache, which is used for all basic tag parents operations. This is useful if it has become damaged or otherwise desynchronised.\n\nIt should only take a second or two.\n\nIf you do not have a specific reason to run this, it is pointless."
            }
            Self::TagText => {
                "This will delete and then recreate the fast search cache for one or all tag services.\n\nIf you have a lot of tags and files, it can take a little while, during which the gui may hang.\n\nIf you do not have a specific reason to run this, it is pointless. It fixes missing autocomplete or tag search results."
            }
            Self::TagTextSubtags => {
                "This will repopulate the fast search cache's subtag search, filling in missing entries, for one or all tag services.\n\nIf you have a lot of tags and files, it can take a little while, during which the gui may hang.\n\nIf you do not have a specific reason to run this, it is pointless. It fixes missing autocomplete or tag search results."
            }
            Self::TagTextSearchable => {
                "This will regenerate the fast search cache's 'unusual character logic' lookup map, for one or all tag services.\n\nIf you have a lot of tags, it can take a little while, during which the gui may hang.\n\nIf you do not have a specific reason to run this, it is pointless. It fixes missing autocomplete search results."
            }
            Self::FixInvalidTags => {
                "This will scan all your tags and repair any that are invalid. This might mean taking out unrenderable characters or cleaning up improper whitespace. If there is a tag collision once cleaned, it may add a (1)-style number on the end.\n\nIf you have a lot of tags, it can take a long time, during which the gui may hang. If it finds bad tags, you should restart the program once it is complete.\n\nIf you have not had tag rendering problems, there is no reason to run this."
            }
            Self::FixInconsistentMappings => {
                "This will check for tags that are occupying mutually exclusive states--either current & pending or deleted & petitioned.\n\nPlease run this if you attempt to upload some tags and get a related error. You may need some follow-up regeneration work to correct autocomplete or 'num pending' counts."
            }
            Self::ResyncTagCounts => {
                "This will scan your mappings cache for surplus or missing files and correct them. This is useful if you see ghost files or if searches miss files that have the tag.\n\nIf you have a lot of tags and files, it can take a long time, during which the gui may hang. It should be much faster than the full regen options though!\n\nIf you do not have a specific reason to run this, it is pointless."
            }
            Self::Analyze => {
                "This will gather statistical information on the database's indices, helping the query planner perform efficiently. It typically happens automatically every few days, but you can force it here. If you have a large database, it will take a few minutes, during which your gui may hang. A popup message will show its status.\n\nA 'soft' analyze will only reanalyze those indices that are due for a check in the normal db maintenance cycle. If nothing is due, it will return immediately.\n\nA 'full' analyze will force a run over every index in the database. This can take substantially longer. If you do not have a specific reason to select this, it is probably pointless."
            }
            Self::OrphanFileRecords => {
                "DO NOT RUN THIS UNLESS YOU KNOW YOU NEED TO\n\nThis will instruct the database to review its file records' integrity. If anything appears to be in a specific domain like \"my files\" but not an associated umbrella domain \"combined local file domains\", and the actual file also exists on disk, it will try to recover the record and fix the umbrella domain. If the file does not actually exist on disk, or the record is in the umbrella domain and not in the specific, or if recovery data cannot be found, the orphaned record will be deleted.\n\nYou typically do not ever see these files and they are basically harmless, but they can offset some file counts confusingly and may break other maintenance routines. You probably only need to run this if you can't process the apparent last handful of duplicate filter pairs or hydrus dev otherwise told you to try it.\n\nIt will create a popup message while it works and inform you of the number of orphan records found. It may lock up the client for a bit."
            }
            Self::OrphanUrlMappings => {
                "DO NOT RUN THIS UNLESS YOU KNOW YOU NEED TO\n\nThis will instruct the database to review its URL records' integrity. If a mapping exists without a master URL, the mapping will be removed. This is useful if your client.master.db has been damaged or truncated because of a partial backup rollback.\n\nIt will create a popup message while it works, but it will not have any feedback until it is complete. It may lock up the client for a bit."
            }
            Self::OrphanTables => {
                "DO NOT RUN THIS UNLESS YOU KNOW YOU NEED TO. MAKE A BACKUP BEFORE YOU RUN IT\n\nThis will instruct the database to review its service tables and delete any orphans. This will typically do nothing, but hydrus dev may tell you to run this, just to check. Be sure you have a recent backup before you run this--if it deletes something important by accident, you will want to roll back!\n\nIt will create popups if it finds anything to delete."
            }
            Self::TablesUsingDefinitions => {
                "SUPER ADVANCED!\n\nThis will gather all the tables and columns that use the particular content type and put them in your clipboard in the format \"(schema_name.)table_name,column_name\". If you want to do mass SELECT or DELETE operations for each of a particular definition, use a multi-editor tool in a powerful text editor to edit all the lines at once (with Ctrl+D, usually).\n\nSome tables are referred to by \"external_x\" schema name, so when you have your commands written out, you will need to either remove the schema names; or use the connect.bat in the db dir, which sets up the correct names for you; or manually initialise your session like so:\n\n.open client.db\nATTACH \"client.caches.db\" as external_caches;\nATTACH \"client.master.db\" as external_master;\nATTACH \"client.mappings.db\" as external_mappings;"
            }
        }
    }

    /// Its yes label, for a plain yes/no question.
    pub const fn yes_label(self) -> &'static str {
        match self {
            Self::TagStorage
            | Self::TagStoragePending
            | Self::TagDisplay
            | Self::TagDisplayPending
            | Self::TagDisplayRepopulate
            | Self::TagText
            | Self::TagTextSubtags
            | Self::TagTextSearchable
            | Self::FixInconsistentMappings
            | Self::ResyncTagCounts => "do it--now choose which service",
            Self::SiblingsLookup
            | Self::ParentsLookup
            | Self::FixInvalidTags
            | Self::Analyze
            | Self::OrphanFileRecords
            | Self::OrphanUrlMappings
            | Self::OrphanTables
            | Self::TablesUsingDefinitions => "do it",
        }
    }
    /// The popup a job shows while it works, finished "done!" after.
    const fn title(self) -> Option<&'static str> {
        Some(match self {
            Self::TagStorage => "regenerating tag mappings cache",
            Self::TagStoragePending => "regenerating tag pending mappings cache",
            Self::TagDisplay => "regenerating tag display mappings cache",
            Self::TagDisplayPending => "regenerating tag display pending mappings cache",
            Self::TagDisplayRepopulate => "repopulating tag display mappings cache",
            Self::TagText => "regenerating tag fast search cache",
            Self::TagTextSubtags => "repopulate tag fast search cache subtags",
            Self::TagTextSearchable => "regenerate tag fast search cache searchable subtag map",
            Self::FixInvalidTags => "repairing invalid tags",
            Self::FixInconsistentMappings => "fixing logically inconsistent mappings",
            Self::ResyncTagCounts => "resyncing tag mappings cache files",
            Self::Analyze => "database maintenance - analyzing",
            Self::OrphanFileRecords => "clear/fix orphan file records",
            Self::OrphanUrlMappings => "clear orphan url mappings",
            Self::SiblingsLookup
            | Self::ParentsLookup
            | Self::OrphanTables
            | Self::TablesUsingDefinitions => return None,
        })
    }
}

/// The "Which service?" choices: all services, then each tag service (name,
/// key, tooltip), as `GetTagServiceKeyForMaintenance` offers them.
pub fn service_choices(store: &Store) -> Vec<(String, Option<ServiceKey>, String)> {
    let snapshot = store.snapshot();
    let mut choices = vec![(
        "all services".to_owned(),
        None,
        "Do it for everything. Can take a long time!".to_owned(),
    )];
    let mut services: Vec<_> = snapshot.services.tag_services().collect();
    // (by name, as the reference's services manager sorts them)
    services.sort_by_key(|s| s.name.to_lowercase());
    for service in services {
        choices.push((
            service.name.clone(),
            Some(service.key.clone()),
            service.name.clone(),
        ));
    }
    choices
}

fn add(store: &Store, popup: Popup, now: i64) -> hydrus_store::Result<[u8; 32]> {
    let key = popup.key;
    store.write(move |ctx| popups::add(ctx.conn(), &popup, now))?;
    Ok(key)
}

fn text(store: &Store, message: impl Into<String>, now: i64) -> hydrus_store::Result<()> {
    #[allow(clippy::cast_precision_loss)] // (seconds)
    add(store, Popup::text(message, now as f64), now).map(|_| ())
}

/// Run an accepted job at `now` (seconds), sending its popups.
///
/// # Errors
/// A store error; its titled popup is then left as it was.
pub fn run(store: &Store, job: Job, answer: &Answer, now: i64) -> hydrus_store::Result<Outcome> {
    let scope = match &answer.service {
        None => TagScope::All,
        Some(key) => TagScope::One(store.snapshot().services.by_key(key)?.id),
    };
    let titled = match job.title() {
        Some(title) => {
            #[allow(clippy::cast_precision_loss)] // (seconds)
            let mut popup = Popup::new(false, true, now as f64);
            popup.status_title = Some(title.to_owned());
            Some(add(store, popup, now)?)
        }
        None => None,
    };
    let mut messages: Vec<String> = Vec::new();
    let mut outcome = Outcome::default();
    // (the invalid-tag scan reports in its own popup's text)
    let mut final_text = "done!".to_owned();
    let mut dismiss = Some(5);
    match job {
        Job::TagStorage => {
            store.write(move |ctx| {
                db::rebuild_tag_counts(ctx.conn(), scope)?;
                db::rebuild_subtag_index(ctx.conn(), SubtagIndex::All).map(|_| ())
            })?;
            messages.push(
                "Now the mappings cache regen is done, you might want to restart the program."
                    .into(),
            );
        }
        Job::TagStoragePending
        | Job::TagDisplay
        | Job::TagDisplayPending
        | Job::TagDisplayRepopulate => {
            store.write(move |ctx| db::rebuild_tag_counts(ctx.conn(), scope))?;
        }
        Job::SiblingsLookup | Job::ParentsLookup => {
            store
                .write_and_refresh(move |ctx| db::rebuild_tag_counts(ctx.conn(), TagScope::All))?;
        }
        Job::TagText | Job::TagTextSubtags | Job::TagTextSearchable => {
            let which = match job {
                Job::TagText => SubtagIndex::All,
                Job::TagTextSubtags => SubtagIndex::MissingOnly,
                _ => SubtagIndex::SearchableMaps,
            };
            store.write(move |ctx| db::rebuild_subtag_index(ctx.conn(), which).map(|_| ()))?;
        }
        Job::FixInvalidTags => {
            let renamed = store.write(|ctx| db::repair_invalid_tags(ctx.conn()))?;
            final_text = if renamed.is_empty() {
                "Invalid tag scanning: No bad tags found!".to_owned()
            } else {
                format!(
                    "Invalid tag scanning: {} bad tags found and fixed! They have been written to the log.",
                    hydrus_core::numbers::human_int(renamed.len() as u64)
                )
            };
            dismiss = None;
        }
        Job::FixInconsistentMappings => {
            let fixed = store.write(move |ctx| db::fix_inconsistent_mappings(ctx.conn(), scope))?;
            messages.push(if fixed == 0 {
                "No inconsistent mappings found!".to_owned()
            } else {
                format!(
                    "Found {} bad mappings! They _should_ be deleted, and your pending counts should be updated.",
                    hydrus_core::numbers::human_int(fixed as u64)
                )
            });
        }
        Job::ResyncTagCounts => {
            let desynced = store.write(move |ctx| db::resync_tag_counts(ctx.conn(), scope))?;
            if desynced.is_empty() {
                messages.push("All checks ok--no desynced mapping caches!".into());
            }
            for (name, wrong) in desynced {
                messages.push(format!(
                    "{} desynced tag counts in {name}!",
                    hydrus_core::numbers::human_int(wrong as u64)
                ));
            }
        }
        Job::Analyze => {
            let full = answer.full;
            store.write(move |ctx| db::analyze(ctx.conn(), full))?;
            messages.push("Done!".into());
        }
        Job::OrphanFileRecords => messages = db::clear_orphan_file_records(store)?,
        Job::OrphanUrlMappings => {
            let deleted = store.write(|ctx| db::clear_orphan_url_mappings(ctx.conn()))?;
            messages.push(if deleted == 0 {
                "No orphan url mappings found!".to_owned()
            } else {
                format!(
                    "{} orphan url mappings deleted!",
                    hydrus_core::numbers::human_int(deleted as u64)
                )
            });
            dismiss = Some(0);
        }
        Job::OrphanTables => {
            let dropped = store.write(|ctx| db::clear_orphan_tables(ctx.conn()))?;
            if dropped.is_empty() {
                messages.push("No orphan tables!".into());
            }
            messages.extend(
                dropped
                    .iter()
                    .map(|t| format!("Cleared orphan table \"{t}\"")),
            );
        }
        Job::TablesUsingDefinitions => {
            let definition = if answer.tag_definitions {
                Definition::Tag
            } else {
                Definition::Hash
            };
            let pairs = store.read(|conn| db::tables_using(conn, definition))?;
            outcome.clipboard = Some(
                pairs
                    .iter()
                    .map(|(table, column)| format!("{table},{column}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            messages.push(format!(
                "{} table and column pairs sent to clipboard.",
                hydrus_core::numbers::human_int(pairs.len() as u64)
            ));
        }
    }
    if let Some(key) = titled {
        store.write(move |ctx| {
            popups::update(ctx.conn(), &key, now, |popup| {
                popup.status_text_1 = Some(final_text);
                match dismiss {
                    Some(0) => popup.finish_and_dismiss(None, now),
                    Some(seconds) => popup.finish_and_dismiss(Some(seconds), now),
                    None => popup.finish(),
                }
            })
            .map(|_| ())
        })?;
    }
    for message in messages {
        text(store, message, now)?;
    }
    Ok(outcome)
}
