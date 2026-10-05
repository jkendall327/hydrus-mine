//! Inherited active-list clipboard and new-page routes, without GUI ownership.
use hydrus_core::{HashKind, search::predicate::SystemPredicate, tag::split_tag};
use hydrus_search::{Predicate, TextContext, predicate_text};
use std::collections::BTreeSet;

/// Clipboard variants offered by the reference's count-free predicate list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Copy {
    /// Selected predicates, expanding OR members.
    Selected,
    /// Selected text after splitting off namespaces.
    Subtags,
    /// Selected subtags with spaces replaced by underscores.
    Underscores,
    /// Selected OR containers written as a single expression.
    Collapsed,
    /// Every predicate in list order.
    All,
    /// Every predicate's subtag in list order.
    AllSubtags,
}

/// New-page variants offered for a captured selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Open {
    /// All selected predicates in one AND search.
    Search,
    /// All selected predicates in one OR search.
    Or,
    /// One search page per selected predicate.
    Each,
    /// A duplicate filter whose first search contains the selected predicates.
    Duplicates,
}

/// A captured inherited route. IDs are separate from active search commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Write actual predicate text to the clipboard.
    Copy(Copy),
    /// Open real search or duplicate-filter pages.
    Open(Open),
}
impl Route {
    /// Stable native menu transport identifier.
    pub fn id(self) -> i32 {
        match self {
            Self::Copy(copy) => 100 + copy as i32,
            Self::Open(open) => 200 + open as i32,
        }
    }
    /// Menu group: clipboard before open before search commands.
    pub fn group(self) -> i32 {
        match self {
            Self::Copy(_) => 0,
            Self::Open(_) => 1,
        }
    }
}

fn export_text(predicate: &Predicate, text: &TextContext) -> String {
    match predicate {
        Predicate::System(SystemPredicate::Hash { hashes, inclusive }) => {
            let base = match hashes.kind() {
                HashKind::Sha256 => "hash",
                HashKind::Md5 => "hash (md5)",
                HashKind::Sha1 => "hash (sha1)",
                HashKind::Sha512 => "hash (sha512)",
            };
            let is = if *inclusive { "is" } else { "is not" };
            let in_word = if hashes.len() > 1 { " in" } else { "" };
            format!("system:{base} {is}{in_word} {}", hashes.to_hex().join(", "))
        }
        Predicate::System(SystemPredicate::SimilarToFiles {
            files,
            max_distance,
        }) => {
            let hashes = files
                .iter()
                .map(hydrus_core::Sha256::to_hex)
                .collect::<Vec<_>>();
            format!(
                "system:similar to {} with distance of {max_distance}",
                hashes.join(", ")
            )
        }
        Predicate::System(SystemPredicate::SimilarToData {
            pixel_hashes,
            perceptual_hashes,
            max_distance,
        }) => {
            let hashes = pixel_hashes
                .iter()
                .map(hydrus_core::Sha256::to_hex)
                .chain(
                    perceptual_hashes
                        .iter()
                        .map(hydrus_core::PerceptualHash::to_hex),
                )
                .collect::<Vec<_>>();
            let base = format!("system:similar to data {}", hashes.join(", "));
            if perceptual_hashes.is_empty() {
                base
            } else {
                format!("{base} with distance of {max_distance}")
            }
        }
        _ => predicate_text(predicate, text),
    }
}

/// Copy plain stored values, preserving reference list order and deduplication.
/// Namespace/wildcard list items copy their raw pattern without an exclusion
/// prefix, whereas members expanded from an OR use ordinary predicate text.
pub fn copy_text(
    selected: &[Predicate],
    current: &[Predicate],
    copy: Copy,
    text: &TextContext,
) -> String {
    let text = TextContext {
        presentation: None,
        ..text.clone()
    };
    let values = if matches!(copy, Copy::All | Copy::AllSubtags) {
        current
    } else {
        selected
    };
    let mut rows = Vec::new();
    for predicate in values {
        match predicate {
            Predicate::Or(children) if copy != Copy::Collapsed => {
                rows.extend(children.iter().map(|p| predicate_text(p, &text)));
            }
            Predicate::Namespace { namespace, .. } => rows.push(format!("{namespace}:*")),
            Predicate::Wildcard { pattern, .. } => rows.push(pattern.as_str().to_owned()),
            _ => rows.push(export_text(predicate, &text)),
        }
    }
    let mut seen = BTreeSet::new();
    rows.into_iter()
        .filter_map(|mut row| {
            if matches!(copy, Copy::Subtags | Copy::Underscores | Copy::AllSubtags) {
                row = split_tag(&row).1.to_owned();
            }
            if copy == Copy::Underscores {
                row = row.replace(' ', "_");
            }
            if row.is_empty() || !seen.insert(row.clone()) {
                None
            } else {
                Some(row)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Predicate batches for real native page creation; OR structure is retained.
pub fn searches(selected: &[Predicate], open: Open) -> Vec<Vec<Predicate>> {
    match open {
        Open::Search | Open::Duplicates => vec![selected.to_vec()],
        Open::Or => vec![vec![Predicate::Or(selected.to_vec())]],
        Open::Each => selected.iter().map(|p| vec![p.clone()]).collect(),
    }
}

/// Exact copy/open labels, including the OR-collapsed selection description.
pub fn menu(
    selected: &[Predicate],
    current: &[Predicate],
    text: &TextContext,
) -> Vec<(Route, String)> {
    if selected.is_empty() {
        return Vec::new();
    }
    let full = copy_text(selected, current, Copy::Selected, text);
    let sub = copy_text(selected, current, Copy::Subtags, text);
    let underscores = copy_text(selected, current, Copy::Underscores, text);
    let collapsed = copy_text(selected, current, Copy::Collapsed, text);
    let lines = |s: &str| s.lines().map(str::to_owned).collect::<BTreeSet<_>>();
    let label = |s: &str, suffix: &str| {
        if s.lines().count() == 1 {
            s.to_owned()
        } else {
            format!(
                "{} selected{suffix}",
                hydrus_core::numbers::human_int(s.lines().count() as u64)
            )
        }
    };
    let mut description = label(&full, "");
    let mut out = vec![(Route::Copy(Copy::Selected), description.clone())];
    if !sub.is_empty() && lines(&sub) != lines(&full) {
        out.push((Route::Copy(Copy::Subtags), label(&sub, " subtags")));
    }
    if !underscores.is_empty() && lines(&underscores) != lines(&sub) {
        out.push((
            Route::Copy(Copy::Underscores),
            label(&underscores, " subtags with underscores"),
        ));
    }
    if collapsed.lines().count() < full.lines().count() {
        description = format!(
            "{} selected, with OR predicates collapsed",
            hydrus_core::numbers::human_int(collapsed.lines().count() as u64)
        );
        out.push((Route::Copy(Copy::Collapsed), description.clone()));
    }
    if current.len() > selected.len() {
        out.extend([
            (Route::Copy(Copy::All), "all tags".into()),
            (Route::Copy(Copy::AllSubtags), "all subtags".into()),
        ]);
    }
    out.push((
        Route::Open(Open::Search),
        format!("open a new search page for {description}"),
    ));
    if selected.len() > 1 && !selected.iter().any(|p| matches!(p, Predicate::Or(_))) {
        out.push((
            Route::Open(Open::Or),
            format!("open a new OR search page for {description}"),
        ));
    }
    if selected.len() > 1 {
        out.push((
            Route::Open(Open::Each),
            "open new search pages for each in selection".into(),
        ));
    }
    out.push((
        Route::Open(Open::Duplicates),
        format!("open a new duplicate filter page for {description}"),
    ));
    for (_, label) in &mut out {
        // ClientGUIMenus.SetMenuTexts elides the displayed caption, never the
        // clipboard payload or predicates sent to a new page.
        let escaped = label.replace('&', "&&");
        if escaped.chars().count() > 128 {
            let head = escaped.chars().take(111).collect::<String>();
            let tail = escaped
                .chars()
                .rev()
                .take(16)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>();
            *label = format!("{head}…{tail}").replace("&&", "&");
        }
    }
    out
}

/// New page titles use plain predicate text, lexically sorted by Qt.
/// Unlike copying, a many-hash predicate keeps its readable summary here.
pub fn page_name(predicates: &[Predicate], text: &TextContext, duplicates: bool) -> String {
    let text = TextContext {
        presentation: None,
        ..text.clone()
    };
    let mut names = predicates
        .iter()
        .map(|p| predicate_text(p, &text))
        .collect::<Vec<_>>();
    names.sort();
    let name = names.join(", ");
    if duplicates {
        format!("duplicates: {name}")
    } else {
        name
    }
}
