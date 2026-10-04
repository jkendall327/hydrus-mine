//! Migration choices and reviewable confirmations, independent of the window.
use hydrus_core::search::context::LocationContext;
use hydrus_core::{HashId, ServiceKey, ServiceType, TagFilter};
pub use hydrus_store::tag_migration::{Action, Content, Progress, Request, Scope, Status};
use hydrus_store::{Result, Store};
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
        };
        model.normalize();
        Ok(model)
    }
    /// Status choices permitted by this source kind.
    pub fn statuses(&self) -> Vec<Status> {
        if self.services[self.source].local {
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
        hydrus_store::tag_migration::actions(
            self.services[self.destination].local,
            self.source == self.destination,
            self.content,
            self.status,
        )
    }
    /// Keep dependent choices valid after any source/content/destination change.
    pub fn normalize(&mut self) {
        self.source = self.source.min(self.services.len() - 1);
        self.destination = self.destination.min(self.services.len() - 1);
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
            source: self.services[self.source].key.clone(),
            destination: self.services[self.destination].key.clone(),
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
            format!(
                "for \"{}\" on the left and for \"{}\" on the right",
                self.left_filter.to_filter_string(),
                self.right_filter.to_filter_string()
            )
        };
        format!(
            "Migrations can make huge changes. They can be cancelled early, but any work they do cannot always be undone. Please check that this summary looks correct:\n\ntaking {} {} {filters} from \"{}\" and {action} \"{}\"\n\nIf you plan to make a very big change (especially a mass delete), I recommend making a backup of your database before going ahead, just in case something unexpected happens.",
            status_label(self.status),
            content_label(self.content),
            self.services[self.source].name,
            self.services[self.destination].name
        )
    }
}
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
