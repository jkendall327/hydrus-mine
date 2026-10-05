//! Migration choices and reviewable confirmations, independent of the window.
use hydrus_core::HashKind;
use hydrus_core::search::context::LocationContext;
use hydrus_core::{HashId, ServiceKey, ServiceType, TagFilter};
pub use hydrus_store::tag_migration::{
    Action, Content, Options, PairCounts, Progress, Request, Scope, Status,
};
use hydrus_store::{Result, Store};
use std::path::Path;
use std::sync::Arc;

/// A selectable real tag service, retained by stable key.
#[derive(Debug, Clone)]
pub struct Service {
    pub key: ServiceKey,
    pub name: String,
    pub local: bool,
}
/// Window settings; execution takes an immutable copy of its request.
#[derive(Debug)]
pub struct Migration {
    store: Arc<Store>,
    default_service: usize,
    pub services: Vec<Service>,
    pub source: usize,
    pub destination: usize,
    pub content: Content,
    pub status: Status,
    pub action: Action,
    pub location: LocationContext,
    pub files: Vec<HashId>,
    pub selected_files: bool,
    pub left_filter: TagFilter,
    pub right_filter: TagFilter,
    pub reason: String,
    pub archives: Options,
    pub count_service: usize,
    pub count_left: bool,
    pub count_right: bool,
    pub count_either: bool,
    pub destination_hash_locked: bool,
}
impl Migration {
    /// Open for a service, optionally limited to selected files by default.
    pub fn new(store: &Arc<Store>, key: &ServiceKey, files: Vec<HashId>) -> Result<Self> {
        let registry = store.snapshot();
        let services = registry
            .services
            .tag_services()
            .filter(|s| {
                matches!(
                    s.service_type(),
                    ServiceType::LocalTag | ServiceType::TagRepository
                )
            })
            .map(|s| Service {
                key: s.key.clone(),
                name: s.name.clone(),
                local: s.service_type() == ServiceType::LocalTag,
            })
            .collect::<Vec<_>>();
        if services.is_empty() {
            return Err(hydrus_store::StoreError::Invalid(
                "there are no real tag services".into(),
            ));
        }
        let source = services.iter().position(|s| &s.key == key).unwrap_or(0);
        let mut model = Self {
            store: store.clone(),
            default_service: source,
            services,
            source,
            destination: source,
            content: Content::Mappings,
            status: Status::Current,
            action: Action::Delete,
            location: store
                .read(hydrus_store::settings::get::<hydrus_store::settings::SearchDefaults>)?
                .local_location,
            selected_files: !files.is_empty(),
            files,
            left_filter: TagFilter::default(),
            right_filter: TagFilter::default(),
            reason: "Mass Migration Job".into(),
            archives: Options::default(),
            count_service: source,
            count_left: false,
            count_right: false,
            count_either: false,
            destination_hash_locked: false,
        };
        model.normalize();
        Ok(model)
    }
    /// Status choices permitted by this source kind.
    pub fn statuses(&self) -> Vec<Status> {
        if self.source == self.services.len() {
            vec![Status::Current]
        } else if self.services[self.source].local {
            vec![Status::Current, Status::Deleted]
        } else {
            vec![
                Status::Current,
                Status::CurrentAndPending,
                Status::Pending,
                Status::Deleted,
            ]
        }
    }
    /// Destination actions with redundant same-service choices suppressed.
    pub fn actions(&self) -> Vec<Action> {
        if self.destination == self.services.len() {
            return vec![Action::Add];
        }
        hydrus_store::tag_migration::actions(
            self.services[self.destination].local,
            self.source == self.destination && self.source < self.services.len(),
            self.content,
            self.status,
        )
    }
    /// Keep dependent choices valid after any source/content/destination change.
    pub fn normalize(&mut self) {
        self.source = self.source.min(self.services.len());
        self.destination = self.destination.min(self.services.len());
        if !self.statuses().contains(&self.status) {
            self.status = Status::Current;
        }
        let actions = self.actions();
        if !actions.contains(&self.action) {
            self.action = actions[0];
        }
    }
    /// Freeze all settings used by the background worker.
    pub fn request(&self) -> Request {
        Request {
            source: self.services[self.source.min(self.services.len() - 1)]
                .key
                .clone(),
            destination: self.services[self.destination.min(self.services.len() - 1)]
                .key
                .clone(),
            content: self.content,
            status: self.status,
            action: self.action,
            scope: if self.selected_files {
                Scope::Files(self.files.clone())
            } else {
                Scope::Location(self.location.clone())
            },
            left_filter: self.left_filter.clone(),
            right_filter: self.right_filter.clone(),
            reason: self.reason.clone(),
        }
    }
    /// Freeze selected archive paths and optional pair-count gates.
    pub fn job_options(&self) -> Result<Options> {
        let mut options = self.archives.clone();
        if self.source != self.services.len() {
            options.source = None;
        } else if options.source.is_none() {
            return Err(hydrus_store::StoreError::Invalid(format!(
                "Please set a path for the source {}.",
                if self.content == Content::Mappings {
                    "Hydrus Tag Archive"
                } else {
                    "Hydrus Tag Pair Archive"
                }
            )));
        }
        if self.destination != self.services.len() {
            options.destination = None;
        } else if options.destination.is_none() {
            return Err(hydrus_store::StoreError::Invalid(format!(
                "Please set a path for the destination {}.",
                if self.content == Content::Mappings {
                    "Hydrus Tag Archive"
                } else {
                    "Hydrus Tag Pair Archive"
                }
            )));
        }
        options.counts = if self.content != Content::Mappings
            && (self.count_left || self.count_right || self.count_either)
        {
            Some(PairCounts {
                service: self.services[self.count_service.min(self.services.len() - 1)]
                    .key
                    .clone(),
                left: self.count_left && !self.count_either,
                right: self.count_right && !self.count_either,
                either: self.count_either,
            })
        } else {
            None
        };
        Ok(options)
    }
    /// Inspect an accepted picker path before changing the current draft.
    pub fn set_archive_path(&mut self, source: bool, path: &Path) -> Result<()> {
        use hydrus_store::tag_migration::archive::{self, Metadata};
        if source {
            archive::inspect(path, self.content)?;
            self.archives.source = Some(path.to_path_buf());
        } else {
            let metadata =
                archive::inspect_destination(path, self.content, self.archives.hash_kind)?;
            if let Metadata::Mappings(kind) = metadata {
                self.archives.hash_kind = kind;
            }
            self.destination_hash_locked =
                path.exists() && archive::inspect(path, self.content).is_ok();
            self.archives.destination = Some(path.to_path_buf());
        }
        Ok(())
    }
    /// Names for the source/destination controls, including the matching archive.
    pub fn endpoint_labels(&self) -> Vec<String> {
        self.services
            .iter()
            .map(|s| s.name.clone())
            .chain(std::iter::once(if self.content == Content::Mappings {
                "Hydrus Tag Archive".into()
            } else {
                "Hydrus Tag Pair Archive".into()
            }))
            .collect()
    }
    fn endpoint_description(&self, source: bool) -> String {
        let index = if source {
            self.source
        } else {
            self.destination
        };
        if let Some(service) = self.services.get(index) {
            return service.name.clone();
        }
        self.archive_path_label(source)
    }
    /// Path label displayed by the archive chooser button.
    pub fn archive_path_label(&self, source: bool) -> String {
        let path = if source {
            &self.archives.source
        } else {
            &self.archives.destination
        };
        path.as_ref().map_or_else(
            || "no path set".into(),
            |p| {
                p.file_name().map_or_else(
                    || p.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                )
            },
        )
    }
    /// Reset archive drafts when the primary content type changes.
    pub fn reset_archives(&mut self) {
        self.archives = Options::default();
        self.destination_hash_locked = false;
        self.source = self.default_service;
        self.destination = self.default_service;
    }
    /// Human-readable current/deleted domain scope, also shown on its button.
    pub fn location_description(&self) -> String {
        if self.location.is_all_known_files() {
            return "all known files".into();
        }
        let registry = self.store.snapshot();
        let name = |k| {
            registry
                .services
                .by_key(k)
                .map_or_else(|_| "removed service".to_string(), |s| s.name.clone())
        };
        self.location
            .current()
            .iter()
            .map(&name)
            .chain(
                self.location
                    .deleted()
                    .iter()
                    .map(|k| format!("deleted from {}", name(k))),
            )
            .collect::<Vec<_>>()
            .join(", ")
    }
    /// First reference confirmation, with the precise content, scopes and filters.
    pub fn confirmation(&self) -> String {
        let action = match self.action {
            Action::Add => "adding them to",
            Action::Delete => "deleting them from",
            Action::ClearDeletion => "clearing their deletion record from",
            Action::Pend => "pending them to",
            Action::Petition => "petitioning them for removal from",
        };
        let filters = if self.content == Content::Mappings {
            let scope = if self.selected_files {
                format!("{} files", self.files.len())
            } else if self.location.is_all_known_files() {
                "all known files".into()
            } else {
                format!("files in \"{}\"", self.location_description())
            };
            format!(
                "for {scope} and for tags \"{}\"",
                self.left_filter.to_filter_string()
            )
        } else {
            let left = self.left_filter.to_filter_string();
            let right = self.right_filter.to_filter_string();
            if left == right {
                format!("for \"{left}\" on both sides")
            } else {
                format!("for \"{left}\" on the left and for \"{right}\" on the right")
            }
        };
        let filters = if self.content != Content::Mappings
            && (self.count_left || self.count_right || self.count_either)
        {
            let service = &self.services[self.count_service.min(self.services.len() - 1)].name;
            let (left, right) = if self.content == Content::Siblings {
                ("worse", "ideal")
            } else {
                ("child", "parent")
            };
            let mut pieces = Vec::new();
            if self.count_either {
                pieces.push(format!(
                    "where the {left} or {right} tag of each pair has count on \"{service}\""
                ));
            } else {
                if self.count_left {
                    pieces.push(format!(
                        "where the {left} tag of each pair has count on \"{service}\""
                    ));
                }
                if self.count_right {
                    pieces.push(format!(
                        "where the {right} tag of each pair{} has count on \"{service}\"",
                        if self.content == Content::Siblings {
                            "'s chain"
                        } else {
                            ""
                        }
                    ));
                }
            }
            format!("{filters} and {}", pieces.join(" and "))
        } else {
            filters
        };
        format!(
            "Migrations can make huge changes. They can be cancelled early, but any work they do cannot always be undone. Please check that this summary looks correct:\n\ntaking {} {} {filters} from \"{}\" and {action} \"{}\"\n\nIf you plan to make a very big change (especially a mass delete), I recommend making a backup of your database before going ahead, just in case something unexpected happens.",
            status_label(self.status),
            content_label(self.content),
            self.endpoint_description(true),
            self.endpoint_description(false)
        )
    }
}
/// Reference source archive picker prompt.
pub const SOURCE_ARCHIVE_PROMPT: &str = "Select the Archive to pull data from.";
/// Reference destination archive picker prompt.
pub const DESTINATION_ARCHIVE_PROMPT: &str = "Select the destination location for the Archive. Existing Archives are also ok, and will be appended to.";
/// Exact second confirmation outside advanced mode.
pub const LAST_CHANCE: &str = "Are you absolutely sure you set up the filters and everything how you wanted? Last chance to turn back.";
/// Reference choice labels.
pub fn status_label(status: Status) -> &'static str {
    match status {
        Status::Current => "current",
        Status::CurrentAndPending => "current and pending",
        Status::Pending => "pending",
        Status::Deleted => "deleted",
    }
}
/// Reference content labels.
pub fn content_label(content: Content) -> &'static str {
    match content {
        Content::Mappings => "tag mappings",
        Content::Siblings => "tag siblings",
        Content::Parents => "tag parents",
    }
}
/// Reference action labels.
pub fn action_label(action: Action) -> &'static str {
    match action {
        Action::Add => "add",
        Action::Delete => "delete",
        Action::ClearDeletion => "clear deletion record",
        Action::Pend => "pending",
        Action::Petition => "petition",
    }
}

/// Hash labels offered for archive destinations.
pub fn hash_label(kind: HashKind) -> &'static str {
    match kind {
        HashKind::Sha256 => "sha256",
        HashKind::Md5 => "md5",
        HashKind::Sha1 => "sha1",
        HashKind::Sha512 => "sha512",
    }
}

/// The actual reference's speed label, based on accepted entries this batch.
pub fn speed_statement(rows: usize, elapsed: std::time::Duration) -> String {
    let speed = if rows == 0 {
        0
    } else {
        (rows as f64 / elapsed.as_secs_f64().max(f64::MIN_POSITIVE)) as u64
    };
    format!("{speed} rows/s")
}
/// Text displayed by the reference job's first status line.
pub fn event_text(event: hydrus_store::tag_migration::Event, previous_accepted: usize) -> String {
    use hydrus_store::tag_migration::Event;
    match event {
        Event::PreparingSource => "preparing source".into(),
        Event::PreparingDestination => "preparing destination".into(),
        Event::BeginningWork => "beginning work".into(),
        Event::Batch { progress, elapsed } => {
            speed_statement(progress.accepted.saturating_sub(previous_accepted), elapsed)
        }
        Event::CleaningSource => "done, cleaning up source".into(),
        Event::CleaningDestination => "done, cleaning up destination".into(),
        Event::Done(_) => "done!".into(),
    }
}
