//! A thumbnail's manage > "file relationships" submenu
//! (`ClientGUIMediaMenus.AddDuplicatesMenu`): what the focused file's
//! duplicate relationships are in the page's domain (and local file
//! storage's, where they differ), and what can be set, removed or reset for
//! it and the selection.

use hydrus_core::numbers::human_int;
use hydrus_core::{HashId, ServiceId};
use hydrus_store::Store;
use hydrus_store::duplicates::{FileRelationships, FileScope, PairRelationship};

/// The relationships a "view" entry opens a page of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Duplicates,
    Alternates,
    FalsePositives,
    Potentials,
}

impl Kind {
    /// `duplicate_type_string_lookup`.
    pub const fn text(self) -> &'static str {
        match self {
            Self::Duplicates => "duplicates",
            Self::Alternates => "alternates",
            Self::FalsePositives => "not related/false positive",
            Self::Potentials => "potential duplicates",
        }
    }
}

/// Which files an entry acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Whose {
    Focused,
    Selected,
}

/// What an entry does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// A label (copies itself).
    Label,
    /// Open a page of the focused file's group's best file, in the page's
    /// domain (`false`) or local file storage's.
    ShowKing {
        local: bool,
    },
    View {
        kind: Kind,
        local: bool,
    },
    SetKing,
    SetBetter,
    SetSameQuality,
    SetAlternates,
    /// Edit the default merge options for this relationship.
    MergeOptions(PairRelationship),
    SetPotential,
    RemoveFromDuplicates,
    RemoveFromAlternates,
    DissolveDuplicates(Whose),
    DissolveAlternates(Whose),
    ClearFalsePositives(Whose),
    ClearInternalFalsePositives,
    ResetSearch(Whose),
    RemovePotentials(Whose),
}

pub type Entry = (String, Act);

/// The submenu, in the template's parts: runs of entries before the merge
/// options submenu, that submenu, runs after it, then the remove and reset
/// submenus (their runs between separators). Empty parts are left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Menu {
    pub before: Vec<Vec<Entry>>,
    pub merge: Vec<Entry>,
    pub after: Vec<Vec<Entry>>,
    pub remove_one: Vec<Entry>,
    pub reset_one: Vec<Vec<Entry>>,
    pub remove_all: Vec<Entry>,
    pub reset_all: Vec<Vec<Entry>>,
}

/// One domain's facts (`file_duplicate_info`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    pub is_king: bool,
    /// Whether it is in a duplicate group with other files.
    pub in_group: bool,
    pub duplicates: usize,
    pub alternates: usize,
    pub false_positives: usize,
    pub potentials: usize,
}

impl Info {
    pub fn of(relationships: &FileRelationships, focused: HashId) -> Self {
        let is_king = relationships.king == focused;
        Self {
            is_king,
            in_group: !is_king || !relationships.duplicates.is_empty(),
            // (the group's others in the domain; without the king there
            // is no best file to show)
            duplicates: relationships.duplicates.len(),
            alternates: relationships.alternates.len(),
            false_positives: relationships.false_positives.len(),
            potentials: relationships.potentials.len(),
        }
    }

    fn any(&self) -> bool {
        self.in_group
            || self.duplicates + self.alternates + self.false_positives + self.potentials > 0
    }
}

/// Read the focused file's facts in the page's domain and local file
/// storage's.
pub fn read(
    store: &Store,
    scope: &FileScope,
    focused: HashId,
) -> hydrus_store::Result<(Info, Option<Info>)> {
    let local =
        hydrus_store::content::DomainRoles::new(&store.snapshot().services)?.local_file_storage;
    store.read(|conn| {
        let here = hydrus_store::duplicates::file_relationships(conn, scope, local, focused)?;
        let local_scope = FileScope::Domains {
            current: vec![local],
            deleted: Vec::new(),
        };
        let there =
            hydrus_store::duplicates::file_relationships(conn, &local_scope, local, focused)?;
        Ok((Info::of(&here, focused), Some(Info::of(&there, focused))))
    })
}

/// The submenu (`AddDuplicatesMenu`): `here` in the page's domain,
/// `local` in local file storage (shown too when it differs, under
/// `local_name`), whether the focused file can be searched for similar
/// files, how many files are selected, and advanced mode.
pub fn menu(
    here: &Info,
    local: Option<(&Info, &str)>,
    can_be_searched: bool,
    num_selected: usize,
    advanced: bool,
) -> Menu {
    let multiple = num_selected > 1;
    let label = |text: &str| (text.to_owned(), Act::Label);
    let mut out = Menu::default();
    let mut views: Vec<Entry> = Vec::new();
    let mut jobs: Vec<(&Info, bool)> = Vec::new();
    if here.any() {
        jobs.push((here, false));
    }
    if let Some((local, _)) = local
        && local.any()
        && local != here
    {
        jobs.push((local, true));
    }
    let (mut in_group, mut in_alternates, mut has_fps, mut has_potentials) =
        (false, false, false, false);
    for &(info, is_local) in &jobs {
        if jobs.len() > 1 {
            views.push(label(&if is_local {
                format!("-for {}-", local.map_or("", |(_, name)| name))
            } else {
                "-for this page's domain-".to_owned()
            }));
        }
        if info.in_group {
            views.push(label("this file is in a duplicate file group"));
            if info.is_king {
                views.push(label("this is the best quality file of its group"));
            } else if info.duplicates == 0 {
                views.push(label(
                    "cannot show the best quality file of this file's group here, it is not in this domain",
                ));
            } else {
                views.push((
                    "show the best quality file of this file's group".into(),
                    Act::ShowKing { local: is_local },
                ));
            }
        }
        for (kind, count) in [
            (Kind::Duplicates, info.duplicates),
            (Kind::Alternates, info.alternates),
            (Kind::FalsePositives, info.false_positives),
            (Kind::Potentials, info.potentials),
        ] {
            if count > 0 {
                views.push((
                    format!("view {} {}", human_int(count as u64), kind.text()),
                    Act::View {
                        kind,
                        local: is_local,
                    },
                ));
                match kind {
                    Kind::Duplicates => in_group = true,
                    Kind::Alternates => in_alternates = true,
                    Kind::FalsePositives => has_fps = true,
                    Kind::Potentials => has_potentials = true,
                }
            }
        }
    }
    if views.is_empty() {
        views.push(label("this file has no duplicate relationships"));
    }
    out.before.push(views);
    let definitely_king = here.is_king;
    let dissolution = can_be_searched || in_group || in_alternates || has_fps;
    let single = dissolution || !definitely_king;
    if multiple || single {
        if !definitely_king {
            out.before.push(vec![(
                "set this file as the best quality of its group".into(),
                Act::SetKing,
            )]);
        }
        if multiple {
            out.before.push(vec![
                (
                    format!(
                        "set this file as better than the {} other selected",
                        human_int(num_selected as u64 - 1)
                    ),
                    Act::SetBetter,
                ),
                (
                    "set all selected as same quality duplicates".into(),
                    Act::SetSameQuality,
                ),
            ]);
            out.before.push(vec![(
                "set all selected as alternates".into(),
                Act::SetAlternates,
            )]);
            out.merge = vec![
                (
                    "for this is a better duplicate".into(),
                    Act::MergeOptions(PairRelationship::Better),
                ),
                (
                    "for same quality".into(),
                    Act::MergeOptions(PairRelationship::SameQuality),
                ),
            ];
            if advanced {
                out.merge.push((
                    "for alternates (advanced!)".into(),
                    Act::MergeOptions(PairRelationship::Alternate),
                ));
            }
            out.after.push(vec![(
                "set all possible pair combinations as 'potential' duplicates for the duplicates filter."
                    .into(),
                Act::SetPotential,
            )]);
        }
        if in_group && !definitely_king {
            out.remove_one.push((
                "remove this file from its duplicate group".into(),
                Act::RemoveFromDuplicates,
            ));
        }
        if in_alternates {
            out.remove_one.push((
                "remove this file's duplicate group from its alternate group".into(),
                Act::RemoveFromAlternates,
            ));
        }
        if dissolution {
            let mut wipes = Vec::new();
            if in_group {
                wipes.push((
                    "DUPLICATE WIPE: dissolve this file's duplicate group".into(),
                    Act::DissolveDuplicates(Whose::Focused),
                ));
            }
            if in_alternates {
                wipes.push((
                    "EVEN BIGGER WIPE: dissolve this file's alternate group".into(),
                    Act::DissolveAlternates(Whose::Focused),
                ));
            }
            if has_fps {
                wipes.push((
                    "delete all false-positive relationships this file's alternate group has with other groups"
                        .into(),
                    Act::ClearFalsePositives(Whose::Focused),
                ));
            }
            let mut searches = Vec::new();
            if can_be_searched {
                searches.push((
                    "schedule this file to be searched for potentials again".into(),
                    Act::ResetSearch(Whose::Focused),
                ));
            }
            if has_potentials {
                searches.push((
                    "delete all this file's potential relationships".into(),
                    Act::RemovePotentials(Whose::Focused),
                ));
            }
            out.reset_one = [wipes, searches]
                .into_iter()
                .filter(|run| !run.is_empty())
                .collect();
        }
        if multiple {
            out.remove_all = vec![(
                "delete all false-positive relationships these files' alternate groups have between each other"
                    .into(),
                Act::ClearInternalFalsePositives,
            )];
            out.reset_all = vec![
                vec![
                    (
                        "DUPLICATE WIPE: completely dissolve these files' duplicate groups".into(),
                        Act::DissolveDuplicates(Whose::Selected),
                    ),
                    (
                        "EVEN BIGGER WIPE: completely dissolve these files' alternate groups"
                            .into(),
                        Act::DissolveAlternates(Whose::Selected),
                    ),
                ],
                vec![(
                    "delete all false-positive relationships these files' alternate groups have with other groups"
                        .into(),
                    Act::ClearFalsePositives(Whose::Selected),
                )],
                vec![
                    (
                        "schedule these files to be searched for potentials again".into(),
                        Act::ResetSearch(Whose::Selected),
                    ),
                    (
                        "delete these files' potential relationships".into(),
                        Act::RemovePotentials(Whose::Selected),
                    ),
                ],
            ];
        }
    }
    out
}

/// Do an entry's work on `focused` and `selected` (the merge options and
/// views are the window's); what it says on failure is the caller's.
pub fn run(
    store: &Store,
    act: Act,
    focused: HashId,
    selected: &[HashId],
    advanced: bool,
) -> hydrus_store::Result<()> {
    let relationship = |r: PairRelationship| -> hydrus_store::Result<()> {
        // (the focused file first: it is the better one)
        let mut ordered = vec![focused];
        ordered.extend(selected.iter().copied().filter(|h| *h != focused));
        crate::duplicates_filtering::set_duplicates(store, &ordered, r, advanced).map(|_| ())
    };
    match act {
        Act::SetBetter => return relationship(PairRelationship::Better),
        Act::SetSameQuality => return relationship(PairRelationship::SameQuality),
        Act::SetAlternates => return relationship(PairRelationship::Alternate),
        Act::SetPotential => return relationship(PairRelationship::Potential),
        _ => {}
    }
    let selected = selected.to_vec();
    store.write_content(move |w| {
        let local: ServiceId = w.roles().local_file_storage;
        let writer = hydrus_store::duplicates::RelationshipWriter::new(w.conn(), local);
        let files = |whose: Whose| -> Vec<HashId> {
            match whose {
                Whose::Focused => vec![focused],
                Whose::Selected => selected.clone(),
            }
        };
        match act {
            Act::SetKing => writer.set_king(focused)?,
            Act::RemoveFromDuplicates => writer.remove_from_duplicate_group(focused)?,
            Act::RemoveFromAlternates => writer.remove_from_alternate_group(focused)?,
            Act::DissolveDuplicates(whose) => writer.dissolve_groups_of(&files(whose))?,
            Act::DissolveAlternates(whose) => {
                writer.dissolve_alternate_groups_of(&files(whose))?;
            }
            Act::ClearFalsePositives(whose) => {
                writer.clear_false_positives(&files(whose), false)?;
            }
            Act::ClearInternalFalsePositives => writer.clear_false_positives(&selected, true)?,
            Act::ResetSearch(whose) => writer.reset_potential_search(&files(whose))?,
            Act::RemovePotentials(whose) => {
                for hash_id in files(whose) {
                    writer.remove_potentials(hash_id)?;
                }
            }
            _ => {}
        }
        Ok(())
    })
}

/// The duplicates scope of a page's location.
pub fn scope_of(
    store: &Store,
    location: &hydrus_core::search::context::LocationContext,
) -> FileScope {
    if location.is_all_known_files() {
        return FileScope::AllKnownFiles;
    }
    let services = &store.snapshot().services;
    let ids = |keys: &std::collections::BTreeSet<hydrus_core::ServiceKey>| {
        keys.iter()
            .filter_map(|key| services.by_key(key).ok().map(|s| s.id))
            .collect()
    };
    FileScope::Domains {
        current: ids(location.current()),
        deleted: ids(location.deleted()),
    }
}

/// The files a "view" or "show the best quality file" entry opens a page
/// of (`ShowDuplicatesInNewPage`), in `scope` (the page's domain) or local
/// file storage's, the focused file first.
pub fn files_to_show(
    store: &Store,
    scope: &FileScope,
    focused: HashId,
    act: Act,
) -> hydrus_store::Result<Vec<HashId>> {
    let local_storage =
        hydrus_store::content::DomainRoles::new(&store.snapshot().services)?.local_file_storage;
    let local_scope = FileScope::Domains {
        current: vec![local_storage],
        deleted: Vec::new(),
    };
    let (kind, local) = match act {
        Act::View { kind, local } => (Some(kind), local),
        Act::ShowKing { local } => (None, local),
        _ => return Ok(Vec::new()),
    };
    let scope = if local { &local_scope } else { scope };
    let r = store.read(|conn| {
        hydrus_store::duplicates::file_relationships(conn, scope, local_storage, focused)
    })?;
    Ok(match kind {
        None => vec![r.king],
        Some(kind) => {
            let others = match kind {
                Kind::Duplicates => r.duplicates,
                Kind::Alternates => r.alternates,
                Kind::FalsePositives => r.false_positives,
                Kind::Potentials => r.potentials,
            };
            std::iter::once(focused).chain(others).collect()
        }
    })
}
