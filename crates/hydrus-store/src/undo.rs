//! Undo and redo of content changes made in the client (the reference's
//! `UndoManager`): archive/inbox and tag mapping changes, each change kept
//! with its inverse. Undoing writes the inverse, redoing the change again;
//! neither is itself recorded. A new change forgets what could be redone.

use hydrus_core::{HashId, ServiceId, TagId};

use crate::content::{ContentWriter, MappingAction};
use crate::error::Result;
use crate::services::ServiceRegistry;

/// A mapping change that can be undone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingChange {
    Add,
    Delete,
    Pend,
    RescindPend,
    Petition(String),
    RescindPetition,
}

impl MappingChange {
    /// The undoable form of a mapping action (none: not undoable).
    pub fn of(action: &MappingAction) -> Option<Self> {
        Some(match action {
            MappingAction::Add => Self::Add,
            MappingAction::Delete => Self::Delete,
            MappingAction::Pend => Self::Pend,
            MappingAction::RescindPend => Self::RescindPend,
            MappingAction::Petition { reason } => Self::Petition(reason.clone()),
            // (the reference doesn't undo rescinded petitions)
            MappingAction::RescindPetition => return None,
        })
    }

    fn action(&self) -> MappingAction {
        match self {
            Self::Add => MappingAction::Add,
            Self::Delete => MappingAction::Delete,
            Self::Pend => MappingAction::Pend,
            Self::RescindPend => MappingAction::RescindPend,
            Self::Petition(reason) => MappingAction::Petition {
                reason: reason.clone(),
            },
            Self::RescindPetition => MappingAction::RescindPetition,
        }
    }

    fn inverse(&self) -> Self {
        match self {
            Self::Add => Self::Delete,
            Self::Delete => Self::Add,
            Self::Pend => Self::RescindPend,
            Self::RescindPend => Self::Pend,
            Self::Petition(_) | Self::RescindPetition => Self::RescindPetition,
        }
    }

    /// `content_update_string_lookup`.
    fn word(&self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Delete => "delete",
            Self::Pend => "pending",
            Self::RescindPend => "rescind pending",
            Self::Petition(_) => "petition",
            Self::RescindPetition => "rescind petition",
        }
    }
}

/// One content change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Archive(Vec<HashId>),
    Inbox(Vec<HashId>),
    Mappings {
        service: ServiceId,
        change: MappingChange,
        tag: TagId,
        files: Vec<HashId>,
    },
}

/// The changes made by one write (a content update package).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Package(pub Vec<Change>);

impl Package {
    fn inverse(&self) -> Package {
        Package(
            self.0
                .iter()
                .map(|change| match change {
                    Change::Archive(files) => Change::Inbox(files.clone()),
                    Change::Inbox(files) => Change::Archive(files.clone()),
                    Change::Mappings {
                        service,
                        change,
                        tag,
                        files,
                    } => Change::Mappings {
                        service: *service,
                        change: change.inverse(),
                        tag: *tag,
                        files: files.clone(),
                    },
                })
                .collect(),
        )
    }

    /// Write it.
    pub fn apply(&self, w: &mut ContentWriter<'_>) -> Result<()> {
        for change in &self.0 {
            match change {
                Change::Archive(files) => w.archive(files)?,
                Change::Inbox(files) => w.inbox(files)?,
                Change::Mappings {
                    service,
                    change,
                    tag,
                    files,
                } => {
                    w.update_mappings(*service, &change.action(), *tag, files)?;
                }
            }
        }
        Ok(())
    }

    /// How the Undo menu names it (`ContentUpdatePackage.ToString`): the
    /// services ("my tags->"), the actions, " tags for" for mappings and the
    /// number of files. Archive and inbox name no service.
    pub fn describe(&self, services: &ServiceRegistry) -> String {
        let mut actions: Vec<&str> = Vec::new();
        let mut locations: Vec<String> = Vec::new();
        let mut mappings = false;
        let mut archive_or_inbox = false;
        let mut num_files = 0;
        for change in &self.0 {
            let (word, files) = match change {
                Change::Archive(files) => {
                    archive_or_inbox = true;
                    ("archive", files)
                }
                Change::Inbox(files) => {
                    archive_or_inbox = true;
                    ("inbox", files)
                }
                Change::Mappings {
                    service,
                    change,
                    files,
                    ..
                } => {
                    mappings = true;
                    if let Ok(service) = services.get(*service) {
                        locations.push(service.name.clone());
                    }
                    (change.word(), files)
                }
            };
            actions.push(word);
            num_files += files.len();
        }
        actions.sort_unstable();
        actions.dedup();
        locations.sort();
        locations.dedup();
        let mut text = String::new();
        if !locations.is_empty() && !archive_or_inbox {
            text.push_str(&locations.join(", "));
            text.push_str("->");
        }
        text.push_str(&actions.join(", "));
        if mappings {
            text.push_str(" tags for");
        }
        text.push_str(&format!(
            " {} files",
            hydrus_core::numbers::human_int(num_files as u64)
        ));
        text
    }
}

/// The changes made so far, and how far back undoing has gone.
#[derive(Debug, Default)]
pub struct UndoLog {
    commands: Vec<(Package, Package)>,
    index: usize,
}

impl UndoLog {
    /// Record a change just made, forgetting anything that could be redone.
    pub fn record(&mut self, package: Package) {
        if package.0.is_empty() {
            return;
        }
        self.commands.truncate(self.index);
        let inverse = package.inverse();
        self.commands.push((package, inverse));
        self.index += 1;
    }

    /// The menu entries' texts ("undo …", "redo …"), where there is one.
    pub fn strings(&self, services: &ServiceRegistry) -> (Option<String>, Option<String>) {
        let undo = self
            .index
            .checked_sub(1)
            .and_then(|i| self.commands.get(i))
            .map(|(package, _)| format!("undo {}", package.describe(services)));
        let redo = self
            .commands
            .get(self.index)
            .map(|(package, _)| format!("redo {}", package.describe(services)));
        (undo, redo)
    }

    /// Step back: the inverse to write, if any.
    pub fn undo(&mut self) -> Option<Package> {
        self.index = self.index.checked_sub(1)?;
        Some(self.commands[self.index].1.clone())
    }

    /// Step forward: the change to write again, if any.
    pub fn redo(&mut self) -> Option<Package> {
        let (package, _) = self.commands.get(self.index)?;
        let package = package.clone();
        self.index += 1;
        Some(package)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_and_redo_step_through_the_log_and_new_changes_cut_it() {
        let files = vec![HashId::from(1), HashId::from(2)];
        let mut log = UndoLog::default();
        log.record(Package(vec![Change::Archive(files.clone())]));
        log.record(Package(vec![Change::Inbox(files.clone())]));
        assert_eq!(
            log.undo(),
            Some(Package(vec![Change::Archive(files.clone())]))
        );
        assert_eq!(
            log.undo(),
            Some(Package(vec![Change::Inbox(files.clone())]))
        );
        assert_eq!(log.undo(), None);
        assert_eq!(
            log.redo(),
            Some(Package(vec![Change::Archive(files.clone())]))
        );
        log.record(Package(vec![Change::Archive(files.clone())]));
        assert_eq!(log.redo(), None);
        assert_eq!(log.commands.len(), 2);
    }
}
