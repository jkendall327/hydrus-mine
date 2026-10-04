//! Owned sort-cog choices; the sort retains its independent full tag context.
use hydrus_core::{
    ServiceKey, ServiceType,
    pages::{PageSort, PageSortBy},
};
use hydrus_store::Store;

pub const GROUPS: [&str; 2] = ["tag service", "ADVANCED: tag display type"];
pub const VIEWS: [(&str, i64); 3] = [
    ("display tags", 1),
    ("multiple media view tags", 3),
    ("single media view tags", 2),
];
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Service(ServiceKey),
    View(i64),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub label: String,
    pub checked: bool,
    pub action: Option<Action>,
}
pub fn groups(sort: &PageSort) -> Vec<&'static str> {
    match sort.by {
        PageSortBy::Namespaces { .. } => GROUPS.to_vec(),
        PageSortBy::System(code)
            if hydrus_search::SortBy::from_code(code) == Some(hydrus_search::SortBy::NumTags) =>
        {
            vec![GROUPS[0]]
        }
        _ => Vec::new(),
    }
}
pub fn entries(store: &Store, sort: &PageSort, group: usize) -> Vec<Entry> {
    if group >= groups(sort).len() {
        return Vec::new();
    }
    if group == 1 {
        let PageSortBy::Namespaces {
            tag_display_type, ..
        } = sort.by
        else {
            return Vec::new();
        };
        return VIEWS
            .iter()
            .map(|(name, code)| Entry {
                label: (*name).into(),
                checked: *code == tag_display_type,
                action: Some(Action::View(*code)),
            })
            .collect();
    }
    let snapshot = store.snapshot();
    let mut out = Vec::new();
    for kind in [
        ServiceType::LocalTag,
        ServiceType::TagRepository,
        ServiceType::CombinedTag,
    ] {
        let mut services = snapshot.services.of_type(kind).collect::<Vec<_>>();
        services.sort_by_key(|service| service.name.to_lowercase());
        if !services.is_empty() && !out.is_empty() {
            out.push(Entry {
                label: "---".into(),
                checked: false,
                action: None,
            });
        }
        out.extend(services.into_iter().map(|service| Entry {
            label: service.name.clone(),
            checked: service.key == sort.tag_context.service,
            action: Some(Action::Service(service.key.clone())),
        }));
    }
    out
}
pub fn choose(sort: &mut PageSort, action: &Action) -> bool {
    if groups(sort).is_empty() {
        return false;
    }
    match action {
        Action::Service(service) => sort.tag_context.service = service.clone(),
        Action::View(view) => {
            if !VIEWS.iter().any(|(_, code)| code == view) {
                return false;
            }
            let PageSortBy::Namespaces {
                tag_display_type, ..
            } = &mut sort.by
            else {
                return false;
            };
            *tag_display_type = *view;
        }
    }
    true
}
