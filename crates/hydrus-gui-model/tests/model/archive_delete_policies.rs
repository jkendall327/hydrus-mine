//! Real Qt domain choices, saved policy and durable scoped deletion.
use hydrus_core::{HashId, ServiceKey};
use hydrus_gui_model::{
    archive_delete::ArchiveDeleteFilter,
    options::{Editor, Row, Settings},
};
use hydrus_search::LocationContext;
use hydrus_store::{
    Store,
    archive_delete_preferences::{self, Preferences},
    settings,
};
use std::sync::Arc;
const ALL: &str =
    "When finishing archive/delete filtering, always delete from all possible domains: ";
const DELAY: &str = "When finishing archive/delete filtering, delay activation of multiple deletion choice buttons: ";
fn store() -> (tempfile::TempDir, Arc<Store>) {
    let dir = tempfile::tempdir().unwrap();
    let legacy = hydrus_testkit::legacy_fixture("basic");
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}
fn draft(store: &Store) -> Editor {
    let mut e = Editor::new(store.read(Settings::load).unwrap());
    let index = e
        .page_names()
        .iter()
        .position(|name| *name == "files and trash")
        .unwrap();
    e.show_page(index);
    e
}
fn row(e: &Editor, label: &str) -> usize {
    e.rows()
        .iter()
        .position(|r| matches!(r,Row::Opt{option,..} if option.label==label))
        .unwrap()
}
fn save(store: &Store, e: &Editor) {
    let (after, before, problems) = e.applied();
    assert!(problems.is_empty());
    let before = before.clone();
    store
        .write(move |tx| after.save(tx.conn(), &before))
        .unwrap();
}
#[test]
fn real_independent_controls_cancel_reopen_merge_and_retained_legacy_native_precedence() {
    let qt = hydrus_testkit::fixture_json("archive_delete_policies.json");
    let (dir, store) = store();
    assert_eq!(
        serde_json::json!([
            Preferences::default().all_domains,
            Preferences::default().delay_multiple
        ]),
        qt["initial"]
    );
    assert_eq!(
        serde_json::from_str::<Preferences>("{}").unwrap(),
        Preferences::default()
    );
    for case in qt["settings"].as_array().unwrap() {
        let mut e = draft(&store);
        e.check(row(&e, ALL), case["draft"][0].as_bool().unwrap());
        e.check(row(&e, DELAY), case["draft"][1].as_bool().unwrap());
        save(&store, &e);
        let p = store.read(archive_delete_preferences::load).unwrap();
        assert_eq!(
            serde_json::json!([p.all_domains, p.delay_multiple]),
            case["saved"]
        );
    }
    let before = store.read(archive_delete_preferences::load).unwrap();
    let mut cancelled = draft(&store);
    cancelled.check(row(&cancelled, ALL), !before.all_domains);
    drop(cancelled);
    assert_eq!(
        store.read(archive_delete_preferences::load).unwrap(),
        before
    );
    let mut e = draft(&store);
    e.check(row(&e, ALL), !before.all_domains);
    store
        .write(move |tx| {
            settings::set(
                tx.conn(),
                &Preferences {
                    all_domains: before.all_domains,
                    delay_multiple: !before.delay_multiple,
                },
            )
        })
        .unwrap();
    save(&store, &e);
    let saved = store.read(archive_delete_preferences::load).unwrap();
    assert_eq!(
        saved,
        Preferences {
            all_domains: !before.all_domains,
            delay_multiple: !before.delay_multiple
        }
    );
    drop(e);
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.read(archive_delete_preferences::load).unwrap(), saved);
    store.write(|tx| {
  let kind=u32::from(hydrus_legacy::serialisable::SerialisableType::CLIENT_OPTIONS.0);
  let(_,dump)=hydrus_store::legacy::singleton(tx.conn(),kind)?.unwrap();
  let from=r#"[[0, "only_show_delete_from_all_local_domains_when_filtering"], [0, false]]"#;assert!(dump.contains(from));
  let dump=dump.replace(from,r#"[[0, "only_show_delete_from_all_local_domains_when_filtering"], [0, true]]"#);
  let from=r#"[[0, "archive_delete_commit_panel_delays_multiple_delete_choices"], [0, true]]"#;assert!(dump.contains(from));
  let dump=dump.replace(from,r#"[[0, "archive_delete_commit_panel_delays_multiple_delete_choices"], [0, false]]"#);
  tx.conn().execute("UPDATE legacy_objects SET dump=?1 WHERE source='json_dumps' AND type_id=?2",rusqlite::params![dump,kind])?;
  tx.conn().execute("DELETE FROM settings WHERE key='archive_delete_finish'",[])?;Ok(())
 }).unwrap();
    assert_eq!(
        store.read(archive_delete_preferences::load).unwrap(),
        Preferences {
            all_domains: true,
            delay_multiple: false
        }
    );
    store
        .write(|tx| settings::set(tx.conn(), &Preferences::default()))
        .unwrap();
    assert_eq!(
        store.read(archive_delete_preferences::load).unwrap(),
        Preferences::default()
    );
}
#[test]
fn actual_qt_domain_alternatives_priorities_delay_and_selected_transaction() {
    let qt = hydrus_testkit::fixture_json("archive_delete_policies.json");
    let (_dir, store) = store();
    let ids: Vec<_> = qt["domains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            let key: ServiceKey = d["key"].as_str().unwrap().parse().unwrap();
            store.snapshot().services.by_key(&key).unwrap().id
        })
        .collect();
    let combined = store
        .snapshot()
        .services
        .by_key(&qt["combined_key"].as_str().unwrap().parse().unwrap())
        .unwrap()
        .id;
    let files:Vec<HashId>=store.read(|conn|{let mut q=conn.prepare("SELECT hash_id FROM file_domain_current WHERE service_id=? ORDER BY hash_id LIMIT 2")?;Ok(q.query_map([ids[0]],|r|r.get(0))?.collect::<Result<_,_>>()?)}).unwrap();
    assert_eq!(files.len(), 2);
    for case in qt["events"].as_array().unwrap() {
        let preferences = Preferences {
            all_domains: case["all"].as_bool().unwrap(),
            delay_multiple: case["delay"].as_bool().unwrap(),
        };
        store
            .write(move |tx| settings::set(tx.conn(), &preferences))
            .unwrap();
        let domains = ids.clone();
        let written_files = files.clone();
        let members = case["members"].as_u64().unwrap();
        store
            .write_content(move |w| {
                w.inbox(&written_files)?;
                for id in &domains {
                    w.delete_files(*id, &written_files, None)?;
                }
                w.add_files(
                    domains[0],
                    &written_files
                        .iter()
                        .map(|file| (*file, None))
                        .collect::<Vec<_>>(),
                )?;
                if members == 2 {
                    w.add_files(
                        domains[1],
                        &written_files
                            .iter()
                            .map(|file| (*file, None))
                            .collect::<Vec<_>>(),
                    )?;
                }
                w.inbox(&written_files)
            })
            .unwrap();
        let keys = match case["page"].as_str().unwrap() {
            "source" | "mixed" | "same_deleted" => {
                vec![store.snapshot().services.get(ids[0]).unwrap().key.clone()]
            }
            "both" => ids
                .iter()
                .map(|id| store.snapshot().services.get(*id).unwrap().key.clone())
                .collect(),
            "combined" => vec![store.snapshot().services.get(combined).unwrap().key.clone()],
            "all" => vec![ServiceKey::new(
                hydrus_core::service::builtin_keys::COMBINED_FILE,
            )],
            _ => panic!(),
        };
        let mut model = ArchiveDeleteFilter::new(
            store.clone(),
            files.clone(),
            LocationContext::new(
                keys,
                match case["page"].as_str().unwrap() {
                    "mixed" => vec![store.snapshot().services.get(ids[1]).unwrap().key.clone()],
                    "same_deleted" => {
                        vec![store.snapshot().services.get(ids[0]).unwrap().key.clone()]
                    }
                    _ => vec![],
                },
            ),
        )
        .unwrap();
        model.keep();
        model.delete();
        let choices = model.deletion_choices();
        let recorded = case["options"].as_array().unwrap();
        assert_eq!(choices.len(), recorded.len());
        assert_eq!(choices[0].label, recorded[0]["label"].as_str().unwrap());
        // Qt's remaining local domains originate in set iteration; keep the first
        // page/combined priority, compare the unordered secondary choices as a set.
        let mut actual = choices
            .iter()
            .skip(1)
            .map(|c| c.label.clone())
            .collect::<Vec<_>>();
        actual.sort();
        let mut expected = recorded
            .iter()
            .skip(1)
            .map(|c| c["label"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        expected.sort();
        assert_eq!(actual, expected);
        assert_eq!(
            model.delay_multiple_choices(),
            !case["delays"].as_array().unwrap().is_empty()
        );
        let selected = recorded[usize::try_from(case["pick"].as_u64().unwrap()).unwrap()]["label"]
            .as_str()
            .unwrap();
        let choice = choices.iter().find(|c| c.label == selected).unwrap();
        model.commit_choice_changed(Some(choice)).unwrap();
        let remaining = store
            .read(|conn| hydrus_store::media::current_domains(conn, &files))
            .unwrap();
        let chosen = case["selected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect::<Vec<_>>();
        for id in &ids {
            let key = store.snapshot().services.get(*id).unwrap().key.to_string();
            let removed = chosen.contains(&key.as_str())
                || chosen.contains(&qt["combined_key"].as_str().unwrap());
            assert_eq!(
                remaining[&files[1]].contains(id),
                (case["members"] == 2 || *id == ids[0]) && !removed
            );
        }
        let batch = store
            .read(|conn| hydrus_store::media::load(conn, &store.snapshot().services, None, &files))
            .unwrap();
        assert!(
            !batch
                .results
                .iter()
                .find(|r| r.hash_id == files[0])
                .unwrap()
                .inbox
        );
    }
}
#[test]
fn replaced_domain_identity_rejects_entire_commit_before_archiving_kept_files() {
    let (_dir, store) = store();
    let snapshot = store.snapshot();
    let source = snapshot.services.by_name("art").unwrap().id;
    let files:Vec<HashId>=store.read(|conn|{let mut q=conn.prepare("SELECT hash_id FROM file_domain_current WHERE service_id=? ORDER BY hash_id LIMIT 2")?;Ok(q.query_map([source],|r|r.get(0))?.collect::<Result<_,_>>()?)}).unwrap();
    assert_eq!(files.len(), 2);
    let written = files.clone();
    store.write_content(move |w| w.inbox(&written)).unwrap();
    let mut model = ArchiveDeleteFilter::new(
        store.clone(),
        files.clone(),
        LocationContext::single(snapshot.services.get(source).unwrap().key.clone()),
    )
    .unwrap();
    model.keep();
    model.delete();
    let choices = model.deletion_choices();
    let captured = choices
        .iter()
        .find(|c| c.label == "delete 1 from art")
        .unwrap()
        .clone();
    store
        .write_and_refresh(move |ctx| {
            ctx.conn().execute(
                "DELETE FROM file_domain_current WHERE service_id=?1",
                [source],
            )?;
            ctx.conn().execute(
                "DELETE FROM file_domain_deleted WHERE service_id=?1",
                [source],
            )?;
            hydrus_store::services::delete(ctx.conn(), source)?;
            hydrus_store::services::insert_with_id(
                ctx.conn(),
                source,
                &ServiceKey::new(vec![241; 32]),
                "replacement domain",
                &hydrus_store::services::ServiceKind::LocalFiles,
            )
        })
        .unwrap();
    assert!(model.commit_choice_changed(Some(&captured)).is_err());
    let inbox = store
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM file_inbox WHERE hash_id=?)",
                [files[0]],
                |row| row.get::<_, bool>(0),
            )?)
        })
        .unwrap();
    assert!(
        inbox,
        "atomic identity rejection must not archive kept files"
    );
}
