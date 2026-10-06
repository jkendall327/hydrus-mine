//! A thumbnail's manage > "file relationships" submenu, as
//! `AddDuplicatesMenu` builds it, and its actions on the basic client.
use hydrus_core::HashId;
use hydrus_gui_model::file_relationships::{
    Act, Info, Kind, Menu, Whose, files_to_show, menu, read, run,
};
use hydrus_store::duplicates::{FileScope, PairRelationship};

fn labels(menu: &Menu) -> Vec<String> {
    menu.before
        .iter()
        .flatten()
        .map(|(l, _)| l.clone())
        .collect()
}

fn alone() -> Info {
    Info {
        is_king: true,
        in_group: false,
        duplicates: 0,
        alternates: 0,
        false_positives: 0,
        potentials: 0,
    }
}

#[test]
fn a_file_alone_offers_only_a_search_reset() {
    let m = menu(
        &alone(),
        Some((&alone(), "hydrus local file storage")),
        true,
        1,
        false,
    );
    assert_eq!(labels(&m), ["this file has no duplicate relationships"]);
    assert!(m.merge.is_empty() && m.remove_one.is_empty() && m.remove_all.is_empty());
    assert_eq!(
        m.reset_one,
        vec![vec![(
            "schedule this file to be searched for potentials again".to_owned(),
            Act::ResetSearch(Whose::Focused)
        )]]
    );
    // nothing at all for an unsearchable lone king
    let m = menu(&alone(), None, false, 1, false);
    assert_eq!(m.before.len(), 1);
    assert!(m.reset_one.is_empty());
}

#[test]
fn a_grouped_file_shows_its_group_and_what_can_be_undone() {
    let here = Info {
        is_king: false,
        in_group: true,
        duplicates: 2,
        alternates: 1,
        false_positives: 0,
        potentials: 3,
    };
    let local = Info {
        duplicates: 3,
        ..here.clone()
    };
    let m = menu(
        &here,
        Some((&local, "hydrus local file storage")),
        true,
        3,
        true,
    );
    let shown = labels(&m);
    assert_eq!(shown[0], "-for this page's domain-");
    assert!(shown.contains(&"view 2 duplicates".to_owned()));
    assert!(shown.contains(&"-for hydrus local file storage-".to_owned()));
    assert!(shown.contains(&"view 3 potential duplicates".to_owned()));
    assert!(shown.contains(&"set this file as the best quality of its group".to_owned()));
    assert!(shown.contains(&"set this file as better than the 2 other selected".to_owned()));
    assert_eq!(m.merge.len(), 3, "advanced mode adds alternates");
    assert_eq!(m.remove_one.len(), 2);
    assert_eq!(m.reset_one[0][0].1, Act::DissolveDuplicates(Whose::Focused));
    assert_eq!(m.reset_all.len(), 3);
    assert_eq!(Kind::FalsePositives.text(), "not related/false positive");
}

#[test]
fn setting_and_undoing_relationships_reaches_the_store() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let files: Vec<HashId> = store
        .read(|conn| {
            let mut q = conn.prepare(
                "SELECT DISTINCT hash_id FROM file_domain_current ORDER BY hash_id LIMIT 3",
            )?;
            Ok(q.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?)
        })
        .unwrap();
    assert_eq!(files.len(), 3);
    let scope = FileScope::AllKnownFiles;
    let (a, rest) = (files[0], &files[..]);
    run(&store, Act::SetAlternates, a, rest, false).unwrap();
    let (info, _) = read(&store, &scope, a).unwrap();
    assert_eq!(info.alternates, 2);
    let shown = files_to_show(
        &store,
        &scope,
        a,
        Act::View {
            kind: Kind::Alternates,
            local: false,
        },
    )
    .unwrap();
    assert_eq!(shown.len(), 3);
    assert_eq!(shown[0], a);
    run(&store, Act::RemoveFromAlternates, a, rest, false).unwrap();
    assert_eq!(read(&store, &scope, a).unwrap().0.alternates, 0);
    // same quality makes a group the focused file leads
    run(&store, Act::SetBetter, a, &files[..2], false).unwrap();
    let (info, _) = read(&store, &scope, files[1]).unwrap();
    assert!(info.in_group && !info.is_king);
    run(&store, Act::SetKing, files[1], &files[..2], false).unwrap();
    assert!(read(&store, &scope, files[1]).unwrap().0.is_king);
    run(
        &store,
        Act::DissolveDuplicates(Whose::Focused),
        a,
        &files[..2],
        false,
    )
    .unwrap();
    let (info, _) = read(&store, &scope, a).unwrap();
    assert!(!info.in_group && info.duplicates == 0);
    // false positives, then cleared
    run(
        &store,
        Act::MergeOptions(PairRelationship::Better),
        a,
        rest,
        false,
    )
    .unwrap();
    hydrus_gui_model::duplicates_filtering::set_duplicates(
        &store,
        &files[..2],
        PairRelationship::FalsePositive,
        false,
    )
    .unwrap();
    assert_eq!(read(&store, &scope, a).unwrap().0.false_positives, 1);
    run(
        &store,
        Act::ClearInternalFalsePositives,
        a,
        &files[..2],
        false,
    )
    .unwrap();
    assert_eq!(read(&store, &scope, a).unwrap().0.false_positives, 0);
}
