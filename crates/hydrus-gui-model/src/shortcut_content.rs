//! Shortcuts that apply a tag or rating (the reference's content
//! application commands): how one reads in the shortcut lists (`ToString`)
//! and what it does to the files it is used on
//! (`ApplyContentApplicationCommandToMedia`).

use hydrus_core::shortcuts::ContentCommand;
use hydrus_core::{HashId, ServiceKey, Tag};
use hydrus_store::services::{ServiceKind, ServiceRegistry};
use hydrus_store::{Result, Store, StoreError};
use rusqlite::params;

fn service_name(services: &ServiceRegistry, key: &ServiceKey) -> String {
    services
        .by_key(key)
        .map_or_else(|_| "unknown service".to_owned(), |s| s.name.clone())
}

/// The command as the reference words it: "flip on/off tag mappings
/// "blue eyes" for my tags", "set ratings 3/5 for stars", "increment
/// ratings for counter".
pub fn text(services: &ServiceRegistry, command: &ContentCommand) -> String {
    match command {
        ContentCommand::Tag { service, tag, flip } => format!(
            "{} tag mappings \"{tag}\" for {}",
            if *flip { "flip on/off" } else { "set" },
            service_name(services, service)
        ),
        ContentCommand::Rating {
            service,
            stars,
            flip,
        } => {
            let value = match (services.by_key(service).map(|s| &s.kind), stars) {
                (_, None) => "not set".to_owned(),
                (Ok(ServiceKind::RatingLike(_)), Some(s)) => {
                    if *s >= 1 { "like" } else { "dislike" }.to_owned()
                }
                (Ok(ServiceKind::RatingNumerical(c)), Some(s)) => {
                    hydrus_core::numbers::value_range(u64::from(*s), u64::from(c.num_stars))
                }
                _ => "unknown".to_owned(),
            };
            format!(
                "{} ratings {value} for {}",
                if *flip { "flip on/off" } else { "set" },
                service_name(services, service)
            )
        }
        ContentCommand::Step { service, up } => format!(
            "{} ratings for {}",
            if *up { "increment" } else { "decrement" },
            service_name(services, service)
        ),
    }
}

/// Apply `command` to `files`, as the reference does: a tag is added where
/// any file lacks it, else (flipping) removed; a rating is set where any
/// file differs, else (flipping) cleared; a step moves each file's rating
/// one star or count, a numerical rating starting at none going to its
/// lowest or highest. Only local tag services take tags. Returns whether
/// anything changed.
///
/// # Errors
/// If the service is missing or of the wrong kind, or the write fails.
pub fn apply(store: &Store, files: &[HashId], command: &ContentCommand) -> Result<bool> {
    if files.is_empty() {
        return Ok(false);
    }
    let snapshot = store.snapshot();
    let files = files.to_vec();
    match command.clone() {
        ContentCommand::Tag { service, tag, flip } => {
            let service = snapshot.services.by_key(&service)?.clone();
            if !matches!(service.kind, ServiceKind::LocalTags) {
                return Err(StoreError::Invalid(
                    "only local tag services can be tagged by shortcut".into(),
                ));
            }
            let tag = Tag::new(&tag)
                .ok_or_else(|| StoreError::Invalid(format!("\"{tag}\" is not a valid tag")))?;
            let table = hydrus_store::schema::MappingTables::new(service.id).current;
            let have = store.read(|conn| {
                let Some(tag_id) = hydrus_store::master::tag_id(conn, &tag)? else {
                    return Ok(0);
                };
                let mut stmt = conn.prepare(&format!(
                    "SELECT 1 FROM {table} WHERE tag_id = ?1 AND hash_id = ?2"
                ))?;
                let mut n = 0;
                for f in &files {
                    if stmt.exists(params![tag_id, f])? {
                        n += 1;
                    }
                }
                Ok(n)
            })?;
            let action = if have < files.len() {
                hydrus_store::content::MappingAction::Add
            } else if flip {
                hydrus_store::content::MappingAction::Delete
            } else {
                return Ok(false);
            };
            store.write_content(move |w| {
                let tag_id = hydrus_store::master::intern_tag(w.conn(), &tag)?;
                w.update_mappings(service.id, &action, tag_id, &files)
                    .map(|n| n > 0)
            })
        }
        ContentCommand::Rating {
            service,
            stars,
            flip,
        } => {
            let service = snapshot.services.by_key(&service)?.clone();
            let value = match (&service.kind, stars) {
                (_, None) => None,
                (ServiceKind::RatingLike(_), Some(s)) => Some(if s >= 1 { 1.0 } else { 0.0 }),
                (ServiceKind::RatingNumerical(c), Some(s)) => Some(c.rating(s)),
                _ => {
                    return Err(StoreError::Invalid(format!(
                        "{} is not a like or numerical rating service",
                        service.name
                    )));
                }
            };
            let current = ratings(store, service.id, &files)?;
            #[allow(clippy::float_cmp)] // (stored as the same fractions)
            let can_set = current.iter().any(|r| *r != value || !flip);
            let set = if can_set {
                value
            } else if current.contains(&value) {
                None
            } else {
                return Ok(false);
            };
            store.write_content(move |w| w.set_rating(service.id, &files, set).map(|()| true))
        }
        ContentCommand::Step { service, up } => {
            let service = snapshot.services.by_key(&service)?.clone();
            match &service.kind {
                ServiceKind::RatingIncDec(_) => {
                    let current: Vec<i64> = store.read(|conn| {
                        let mut stmt = conn.prepare_cached(
                            "SELECT rating FROM ratings_incdec WHERE service_id = ?1 AND hash_id = ?2",
                        )?;
                        files
                            .iter()
                            .map(|f| {
                                Ok(stmt
                                    .query_row(params![service.id, f], |r| r.get(0))
                                    .unwrap_or(0))
                            })
                            .collect()
                    })?;
                    let changes: Vec<(HashId, i64)> = files
                        .iter()
                        .zip(current)
                        .map(|(f, v)| (*f, (v + if up { 1 } else { -1 }).max(0)))
                        .collect();
                    store.write_content(move |w| {
                        for (f, v) in changes {
                            w.set_incdec(service.id, &[f], v)?;
                        }
                        Ok(true)
                    })
                }
                ServiceKind::RatingNumerical(c) => {
                    let c = c.clone();
                    let current = ratings(store, service.id, &files)?;
                    let one = c.rating(c.min_stars() + 1) - c.rating(c.min_stars());
                    let changes: Vec<(HashId, f64)> = files
                        .iter()
                        .zip(current)
                        .filter_map(|(f, r)| {
                            let new = match r {
                                None => {
                                    if up {
                                        0.0
                                    } else {
                                        1.0
                                    }
                                }
                                Some(r) => (r + if up { one } else { -one }).clamp(0.0, 1.0),
                            };
                            #[allow(clippy::float_cmp)] // (stored as the same fractions)
                            let changed = r != Some(new);
                            changed.then_some((*f, new))
                        })
                        .collect();
                    if changes.is_empty() {
                        return Ok(false);
                    }
                    store.write_content(move |w| {
                        for (f, v) in changes {
                            w.set_rating(service.id, &[f], Some(v))?;
                        }
                        Ok(true)
                    })
                }
                _ => Err(StoreError::Invalid(format!(
                    "{} can't be stepped",
                    service.name
                ))),
            }
        }
    }
}

fn ratings(
    store: &Store,
    service: hydrus_core::ServiceId,
    files: &[HashId],
) -> Result<Vec<Option<f64>>> {
    store.read(|conn| {
        let mut stmt = conn
            .prepare_cached("SELECT rating FROM ratings WHERE service_id = ?1 AND hash_id = ?2")?;
        files
            .iter()
            .map(|f| Ok(stmt.query_row(params![service, f], |r| r.get(0)).ok()))
            .collect()
    })
}

/// What kind of value a service's content command takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueKind {
    Tag,
    Like,
    Stars { min: u32, max: u32 },
    Count,
}

/// A service a content command can apply to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentService {
    pub key: ServiceKey,
    pub name: String,
    pub value: ValueKind,
}

/// The services shortcuts can tag or rate with: local tag services, then
/// the rating services.
pub fn services(registry: &ServiceRegistry) -> Vec<ContentService> {
    registry
        .all()
        .filter_map(|s| {
            let value = match &s.kind {
                ServiceKind::LocalTags => ValueKind::Tag,
                ServiceKind::RatingLike(_) => ValueKind::Like,
                ServiceKind::RatingNumerical(c) => ValueKind::Stars {
                    min: c.min_stars(),
                    max: c.num_stars,
                },
                ServiceKind::RatingIncDec(_) => ValueKind::Count,
                _ => return None,
            };
            Some(ContentService {
                key: s.key.clone(),
                name: s.name.clone(),
                value,
            })
        })
        .collect()
}

/// The actions a service's commands offer, as the editor lists them.
pub fn actions(value: &ValueKind) -> &'static [&'static str] {
    match value {
        ValueKind::Tag => &["flip on/off", "set"],
        ValueKind::Like | ValueKind::Stars { .. } => {
            &["flip on/off", "set", "increment", "decrement"]
        }
        ValueKind::Count => &["increment", "decrement"],
    }
}

/// The command the editor's choices make: `action` from [`actions`], and
/// `value` typed (a tag; "like" or "dislike"; a number of stars, or blank
/// for none).
///
/// # Errors
/// If the value doesn't suit the service.
pub fn command(
    service: &ContentService,
    action: &str,
    value: &str,
) -> Result<ContentCommand, String> {
    let key = service.key.clone();
    match (action, &service.value) {
        ("increment" | "decrement", ValueKind::Stars { .. } | ValueKind::Count) => {
            Ok(ContentCommand::Step {
                service: key,
                up: action == "increment",
            })
        }
        ("increment" | "decrement", ValueKind::Like) => {
            Err("like/dislike ratings can't be stepped".into())
        }
        (_, ValueKind::Tag) => {
            let tag = Tag::new(value).ok_or_else(|| format!("\"{value}\" is not a valid tag"))?;
            Ok(ContentCommand::Tag {
                service: key,
                tag: tag.into_string(),
                flip: action == "flip on/off",
            })
        }
        (_, ValueKind::Like) => {
            let stars = match value.trim() {
                "like" => Some(1),
                "dislike" => Some(0),
                "" => None,
                other => return Err(format!("\"{other}\" is not like, dislike or blank")),
            };
            Ok(ContentCommand::Rating {
                service: key,
                stars,
                flip: action == "flip on/off",
            })
        }
        (_, ValueKind::Stars { min, max }) => {
            let stars = match value.trim() {
                "" => None,
                text => {
                    let n: u32 = text
                        .parse()
                        .map_err(|_| format!("\"{text}\" is not a number of stars"))?;
                    if n < *min || n > *max {
                        return Err(format!("the stars must be {min} to {max}"));
                    }
                    Some(n)
                }
            };
            Ok(ContentCommand::Rating {
                service: key,
                stars,
                flip: action == "flip on/off",
            })
        }
        (_, ValueKind::Count) => Err("inc/dec ratings can only be stepped".into()),
    }
}

/// The editor's choices for an existing command: its service's index in
/// `services`, its action and its typed value.
pub fn choices(
    services: &[ContentService],
    command: &ContentCommand,
) -> Option<(usize, &'static str, String)> {
    let (key, action, value) = match command {
        ContentCommand::Tag { service, tag, flip } => (
            service,
            if *flip { "flip on/off" } else { "set" },
            tag.clone(),
        ),
        ContentCommand::Rating {
            service,
            stars,
            flip,
        } => {
            let like = services
                .iter()
                .any(|s| &s.key == service && s.value == ValueKind::Like);
            let value = match (like, stars) {
                (_, None) => String::new(),
                (true, Some(s)) => if *s >= 1 { "like" } else { "dislike" }.to_owned(),
                (false, Some(s)) => s.to_string(),
            };
            (service, if *flip { "flip on/off" } else { "set" }, value)
        }
        ContentCommand::Step { service, up } => (
            service,
            if *up { "increment" } else { "decrement" },
            String::new(),
        ),
    };
    let index = services.iter().position(|s| &s.key == key)?;
    Some((index, action, value))
}
