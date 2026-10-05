//! Captured active-list search commands and exact edit deltas.
pub mod routes;
use hydrus_core::sort::human_sort_key;
use hydrus_search::{Predicate, TextContext, predicate_text};

/// A represented search-submenu command, captured with its selected values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Reopen one represented existing system value.
    Edit,
    /// Remove the captured terms.
    Remove,
    /// Add inverse terms without removing an inverse already present.
    Invert,
    /// Replace the selected terms with their OR container.
    ReplaceOr,
    /// Replace selected OR containers with their constituent terms.
    DissolveOr,
    /// Add the common selected-tag namespace.
    Namespace,
    /// Exclude the common selected-tag namespace.
    ExcludeNamespace,
    /// Ctrl activation toggles an already-present inverse; the menu only adds.
    InvertToggle,
    /// Stage the selected terms in the populated OR editor before replacement.
    StartOr,
}
impl Command {
    /// Stable native transport identifier.
    pub fn id(self) -> i32 {
        self as i32
    }
}

/// Actual reference labels for the currently represented search commands.
pub fn menu(
    selected: &[Predicate],
    current: &[Predicate],
    text: &TextContext,
    editable: Option<&[Predicate]>,
) -> Vec<(Command, String)> {
    if selected.is_empty() {
        return Vec::new();
    }
    let description = if selected.len() == 1 {
        predicate_text(&selected[0], text)
    } else {
        "selected".into()
    };
    let mut out = Vec::new();
    if let Some(editable) = editable {
        let description = if editable.len() == 1 {
            predicate_text(&editable[0], text)
        } else {
            format!("{} search terms", editable.len())
        };
        out.push((Command::Edit, format!("edit {description}")));
    }
    out.push((
        Command::Remove,
        format!("remove {description} from current search"),
    ));
    let inverses = inverses(selected, text);
    if !inverses.is_empty() {
        out.push((
            Command::Invert,
            if inverses.len() == 1 {
                format!(
                    "invert: add {} to current search",
                    predicate_text(&inverses[0], text)
                )
            } else {
                "invert selected and add to current search".into()
            },
        ));
    }
    if selected.len() > 1 && !selected.iter().any(|p| matches!(p, Predicate::Or(_))) {
        out.push((
            Command::ReplaceOr,
            format!("replace {description} with their OR"),
        ));
    }
    if !selected.iter().any(|p| matches!(p, Predicate::Or(_))) {
        out.push((
            Command::StartOr,
            format!("start an OR predicate with {description}"),
        ));
    }
    if selected.iter().all(|p| matches!(p, Predicate::Or(_))) {
        out.push((
            Command::DissolveOr,
            format!("dissolve {description} into single predicates"),
        ));
    }
    if let Some(namespace) = namespace(selected) {
        let label = predicate_text(&namespace, text);
        if !current.contains(&namespace) {
            out.push((Command::Namespace, format!("add {label} to current search")));
        }
        if let Some(inverse) = namespace.inverse(&|_| false)
            && !current.contains(&inverse)
        {
            out.push((
                Command::ExcludeNamespace,
                format!("exclude {label} from the current search"),
            ));
        }
    }
    out
}

/// Invert only reference-invertible values, resolving counter rating services.
pub fn inverses(selected: &[Predicate], text: &TextContext) -> Vec<Predicate> {
    selected
        .iter()
        .filter_map(|p| p.inverse(&|s| hydrus_search::entry::is_incdec(s, text)))
        .collect()
}
fn namespace(selected: &[Predicate]) -> Option<Predicate> {
    let Predicate::Tag { tag, .. } = selected.first()? else {
        return None;
    };
    let namespace = tag.namespace();
    selected
        .iter()
        .all(|p| matches!(p, Predicate::Tag { tag, .. } if tag.namespace() == namespace))
        .then(|| Predicate::Namespace {
            namespace: namespace.to_owned(),
            inclusive: true,
        })
}

/// Apply the reference edit's set difference, without toggling unchanged values.
pub fn replace(
    current: &mut Vec<Predicate>,
    original: &[Predicate],
    edited: &[Predicate],
    text: &TextContext,
) -> bool {
    let before = current.clone();
    current.retain(|p| !original.contains(p) || edited.contains(p));
    for p in edited {
        if !current.contains(p) {
            current.push(p.clone());
        }
    }
    let plain = TextContext {
        presentation: None,
        ..text.clone()
    };
    current.sort_by_cached_key(|p| {
        let label = match p {
            Predicate::Or(terms) => terms
                .iter()
                .map(|p| predicate_text(p, &plain))
                .min_by_key(|s| human_sort_key(s))
                .unwrap_or_default(),
            _ => predicate_text(p, &plain),
        };
        human_sort_key(&label)
    });
    *current != before
}

/// Execute a captured command. Edit is handled by the owned GUI child.
pub fn apply(
    current: &mut Vec<Predicate>,
    selected: &[Predicate],
    command: Command,
    text: &TextContext,
) -> bool {
    let before = current.clone();
    match command {
        Command::Edit | Command::StartOr => return false,
        Command::Remove => current.retain(|p| !selected.contains(p)),
        Command::Invert => {
            let add: Vec<_> = inverses(selected, text)
                .into_iter()
                .filter(|p| !current.contains(p))
                .collect();
            hydrus_search::enter_predicates(current, &add, text);
        }
        Command::InvertToggle => {
            hydrus_search::enter_predicates(current, &inverses(selected, text), text)
        }
        Command::ReplaceOr => {
            replace(current, selected, &[Predicate::Or(selected.to_vec())], text);
        }
        Command::DissolveOr => {
            let terms = selected
                .iter()
                .flat_map(|p| {
                    if let Predicate::Or(terms) = p {
                        terms.clone()
                    } else {
                        Vec::new()
                    }
                })
                .collect::<Vec<_>>();
            current.retain(|p| !selected.contains(p));
            let add: Vec<_> = terms.into_iter().filter(|p| !current.contains(p)).collect();
            hydrus_search::enter_predicates(current, &add, text);
        }
        Command::Namespace | Command::ExcludeNamespace => {
            if let Some(mut p) = namespace(selected) {
                if command == Command::ExcludeNamespace {
                    p = p.inverse(&|_| false).unwrap_or(p);
                }
                if !current.contains(&p) {
                    hydrus_search::enter_predicates(current, &[p], text);
                }
            }
        }
    }
    *current != before
}
