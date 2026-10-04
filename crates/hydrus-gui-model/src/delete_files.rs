//! The advanced local deletion dialog: actionable domains, physical-delete lock,
//! preserved existing reasons, and remembered choices. Cancel never writes.
use hydrus_core::{HashId, ServiceType};
use hydrus_store::Store;
use hydrus_store::content::DomainRoles;
use hydrus_store::settings::{DeletionAction, DeletionPreferences};

/// A permitted action, frozen to the files actionable when the dialog opened.
#[derive(Debug, Clone)]
pub struct Choice {
    pub label: String,
    pub action: DeletionAction,
    pub files: Vec<HashId>,
}

/// A deletion reason or the instruction to preserve each file's existing reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    Text(String),
    Custom,
    Preserve,
}

/// A reason radio row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasonChoice {
    pub label: String,
    pub reason: Reason,
}

/// The editable dialog state. Store writes happen only on explicit acceptance.
#[derive(Debug, Clone)]
pub struct Draft {
    pub choices: Vec<Choice>,
    pub reasons: Vec<ReasonChoice>,
    pub action: usize,
    pub reason: usize,
    pub custom: String,
    default_reason: String,
    shared_reason: Option<String>,
    includes_domains: bool,
    local_domain_count: usize,
}

impl Draft {
    /// Read actionable local domains and unlocked physical deletions, in reference order.
    pub fn load(
        store: &Store,
        files: &[HashId],
        suggested: Option<&DeletionAction>,
        default_reason: &str,
    ) -> hydrus_store::Result<Self> {
        let snapshot = store.snapshot();
        let roles = DomainRoles::new(&snapshot.services)?;
        let prefs: DeletionPreferences = store.read(hydrus_store::settings::get)?;
        let batch =
            store.read(|c| hydrus_store::media::load(c, &snapshot.services, None, files))?;
        let locked = store
            .read(|c| hydrus_store::delete_lock::locked(c, roles.local_file_storage, files))?;
        let in_domain = |domain| {
            batch
                .results
                .iter()
                .filter(|m| m.is_current_in(domain))
                .map(|m| m.hash_id)
                .collect::<Vec<_>>()
        };
        let mut domains = snapshot
            .services
            .of_type(ServiceType::LocalFileDomain)
            .collect::<Vec<_>>();
        domains.sort_by_cached_key(|s| s.name.to_lowercase());
        let domains = domains
            .into_iter()
            .filter_map(|s| {
                let files = in_domain(s.id);
                (!files.is_empty()).then_some((s, files))
            })
            .collect::<Vec<_>>();
        let mut choices = Vec::new();
        for (service, files) in &domains {
            let count = if files.len() == 1 {
                "one file".into()
            } else {
                format!(
                    "{} files",
                    hydrus_core::numbers::human_int(files.len() as u64)
                )
            };
            choices.push(Choice {
                label: if domains.len() == 1 {
                    format!("Send {count} from {} to trash?", service.name)
                } else {
                    format!("Remove {count} from {}?", service.name)
                },
                action: DeletionAction::Domain(service.key.clone()),
                files: files.clone(),
            });
        }
        if domains.len() > 1 {
            choices.push(Choice {
                label: "Delete from all local services? (force send to trash)".into(),
                action: DeletionAction::Domain(
                    snapshot
                        .services
                        .get(roles.combined_local_media)?
                        .key
                        .clone(),
                ),
                files: batch
                    .results
                    .iter()
                    .filter(|m| domains.iter().any(|(s, _)| m.is_current_in(s.id)))
                    .map(|m| m.hash_id)
                    .collect(),
            });
        }
        let physical = |domain| {
            in_domain(domain)
                .into_iter()
                .filter(|f| !locked.contains(f))
                .collect::<Vec<_>>()
        };
        let trash = physical(roles.trash);
        let storage = physical(roles.local_file_storage);
        for (files, trashed) in [(&trash, true), (&storage, false)] {
            if files.is_empty() || (!trashed && *files == trash) {
                continue;
            }
            let count = if files.len() == 1 {
                "one".to_owned()
            } else {
                hydrus_core::numbers::human_int(files.len() as u64)
            };
            let suffix = if files.len() == 1 { "file" } else { "files" };
            choices.push(Choice {
                label: format!(
                    "Permanently delete {count} {}{suffix}?",
                    if trashed { "trashed " } else { "" }
                ),
                action: DeletionAction::Physical,
                files: files.clone(),
            });
        }
        if !storage.is_empty() {
            let count = if storage.len() == 1 {
                "this file".to_owned()
            } else {
                format!(
                    "these {} files",
                    hydrus_core::numbers::human_int(storage.len() as u64)
                )
            };
            choices.push(Choice {
                label: format!("Permanently delete {count} and do not save a deletion record?"),
                action: DeletionAction::ClearRecord,
                files: storage,
            });
        }
        let all_existing =
            !batch.results.is_empty() && batch.results.iter().all(|m| m.deletion_reason.is_some());
        let shared_reason = all_existing
            .then(|| batch.results[0].deletion_reason.clone())
            .flatten()
            .filter(|r| {
                batch
                    .results
                    .iter()
                    .all(|m| m.deletion_reason.as_ref() == Some(r))
            });
        let last_reason = prefs.last_reason.as_ref().filter(|_| prefs.remember_reason);
        let mut reasons = Vec::new();
        let mut forced = None;
        let mut reason = 0;
        let suggested_reasons = std::iter::once(default_reason)
            .filter(|r| !prefs.reasons.iter().any(|s| s == r))
            .chain(prefs.reasons.iter().map(String::as_str));
        for text in suggested_reasons {
            if shared_reason.as_deref() == Some(text) {
                if forced.is_some() {
                    continue;
                }
                forced = Some(reasons.len());
            }
            if last_reason.map(String::as_str) == Some(text)
                || (last_reason.is_none() && text == default_reason)
            {
                reason = reasons.len();
            }
            reasons.push(ReasonChoice {
                label: if shared_reason.as_deref() == Some(text) {
                    format!("keep existing reason: {text}")
                } else {
                    text.into()
                },
                reason: Reason::Text(text.into()),
            });
        }
        let custom_index = reasons.len();
        reasons.push(ReasonChoice {
            label: "custom".into(),
            reason: Reason::Custom,
        });
        if let Some(shared) = &shared_reason
            && forced.is_none()
        {
            forced = Some(reasons.len());
            reasons.push(ReasonChoice {
                label: format!("keep existing reason: {shared}"),
                reason: Reason::Text(shared.clone()),
            });
        } else if all_existing && shared_reason.is_none() {
            forced = Some(reasons.len());
            reasons.push(ReasonChoice { label: "(all files have existing file deletion reasons and they differ): do not alter them.".into(), reason: Reason::Preserve });
        }
        let mut custom = String::new();
        if let Some(last) = last_reason
            && !prefs.reasons.contains(last)
        {
            custom.clone_from(last);
            reason = custom_index;
        }
        reason = forced.unwrap_or(reason);
        let action = prefs
            .last_action
            .as_ref()
            .filter(|_| prefs.remember_action)
            .and_then(|a| choices.iter().position(|c| &c.action == a))
            .or_else(|| suggested.and_then(|a| choices.iter().position(|c| &c.action == a)))
            .unwrap_or(0);
        Ok(Self {
            local_domain_count: domains.len(),
            includes_domains: !domains.is_empty(),
            choices,
            reasons,
            action,
            reason,
            custom,
            default_reason: default_reason.into(),
            shared_reason,
        })
    }

    /// The reference bypasses even an advanced dialog for one actionable domain
    /// when trash confirmation is disabled, retaining the selected remembered action.
    pub fn already_resolved(&self, confirm_trash: bool) -> bool {
        self.local_domain_count == 1 && !confirm_trash
    }

    /// Reason controls are disabled for a clean deletion.
    pub fn reason_enabled(&self) -> bool {
        self.choices
            .get(self.action)
            .is_some_and(|c| c.action != DeletionAction::ClearRecord)
    }

    /// Only the custom radio row enables the free text field.
    pub fn custom_enabled(&self) -> bool {
        self.reason_enabled()
            && self
                .reasons
                .get(self.reason)
                .is_some_and(|r| r.reason == Reason::Custom)
    }

    /// Apply the frozen choice atomically, preserving each existing reason for Preserve.
    /// Remembered fields merge into the current settings so concurrent option edits survive.
    pub fn apply(&self, store: &Store) -> hydrus_store::Result<()> {
        let Some(choice) = self.choices.get(self.action).cloned() else {
            return Ok(());
        };
        let reason = if self.reason_enabled() {
            self.reasons.get(self.reason).and_then(|r| match &r.reason {
                Reason::Text(t) => Some(t.clone()),
                Reason::Custom => Some(self.custom.clone()),
                Reason::Preserve => None,
            })
        } else {
            None
        };
        let default_reason = self.default_reason.clone();
        let shared_reason = self.shared_reason.clone();
        let includes_domains = self.includes_domains;
        store.write_content(move |w| {
            let roles = w.roles().clone();
            let domain = match &choice.action {
                DeletionAction::Domain(key) => store_service(w.conn(), key)?,
                DeletionAction::Physical | DeletionAction::ClearRecord => roles.local_file_storage,
            };
            let mut files = choice.files;
            if matches!(
                choice.action,
                DeletionAction::Physical | DeletionAction::ClearRecord
            ) {
                let locked =
                    hydrus_store::delete_lock::locked(w.conn(), roles.local_file_storage, &files)?;
                files.retain(|file| !locked.contains(file));
            }
            w.delete_files(domain, &files, reason.as_deref())?;
            if choice.action == DeletionAction::ClearRecord {
                w.clear_local_delete_records(Some(&files))?;
            }
            let mut prefs: DeletionPreferences = hydrus_store::settings::get(w.conn())?;
            // The reference identifies a previous user service by a 64-character
            // hex key; its short builtin keys behave like special actions here.
            let keep_previous = !includes_domains && (prefs.last_action.is_none() || matches!(&prefs.last_action, Some(DeletionAction::Domain(key)) if key.as_bytes().len() == 32));
            if prefs.remember_action && !keep_previous
            {
                prefs.last_action = Some(choice.action.clone());
            }
            if prefs.remember_reason
                && let Some(reason) = reason.filter(|r| shared_reason.as_ref() != Some(r))
            {
                prefs.last_reason = (reason != default_reason).then_some(reason);
            }
            hydrus_store::settings::set(w.conn(), &prefs)
        })
    }
}

fn store_service(
    conn: &rusqlite::Connection,
    key: &hydrus_core::ServiceKey,
) -> hydrus_store::Result<hydrus_core::ServiceId> {
    Ok(hydrus_store::services::ServiceRegistry::load(conn)?
        .by_key(key)?
        .id)
}

/// The reference's ordered reason queue, with stable row identities so duplicate
/// text and movement preserve selection. Add/Edit accept empty strings verbatim.
#[derive(Debug, Clone)]
pub struct ReasonQueue {
    rows: Vec<(u64, String)>,
    next: u64,
    pub selection: crate::list_selection::ListSelection<u64>,
}
impl ReasonQueue {
    pub fn new(values: &[String]) -> Self {
        Self {
            rows: values
                .iter()
                .enumerate()
                .map(|(i, v)| (i as u64, v.clone()))
                .collect(),
            next: values.len() as u64,
            selection: crate::list_selection::ListSelection::default(),
        }
    }
    pub fn rows(&self) -> &[(u64, String)] {
        &self.rows
    }
    pub fn values(&self) -> Vec<String> {
        self.rows.iter().map(|(_, v)| v.clone()).collect()
    }
    pub fn click(&mut self, index: usize, ctrl: bool, shift: bool) {
        self.selection.click(
            &self.rows.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
            index,
            ctrl,
            shift,
        );
    }
    pub fn add(&mut self, value: String) {
        self.rows.push((self.next, value));
        self.next += 1;
    }
    pub fn editing(&self) -> Option<(u64, &str)> {
        self.rows
            .iter()
            .find(|(key, _)| self.selection.is_selected(*key))
            .map(|(key, text)| (*key, text.as_str()))
    }
    pub fn replace(&mut self, key: u64, value: String) {
        if let Some((_, text)) = self.rows.iter_mut().find(|(k, _)| *k == key) {
            *text = value;
        }
    }
    pub fn move_selected(&mut self, down: bool) {
        let mut selected = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, (key, _))| self.selection.is_selected(*key))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if down {
            selected.reverse();
        }
        for index in selected {
            let other = if down {
                index.checked_add(1)
            } else {
                index.checked_sub(1)
            };
            if let Some(other) = other.filter(|i| *i < self.rows.len()) {
                self.rows.swap(index, other);
            }
        }
    }
    pub fn removal_question(&self) -> Option<String> {
        (!self.selection.is_empty()).then(|| {
            format!(
                "Remove {} selected?",
                hydrus_core::numbers::human_int(
                    self.rows
                        .iter()
                        .filter(|(key, _)| self.selection.is_selected(*key))
                        .count() as u64
                )
            )
        })
    }
    pub fn remove_selected(&mut self) {
        self.rows
            .retain(|(key, _)| !self.selection.is_selected(*key));
        self.selection = crate::list_selection::ListSelection::default();
    }
}
