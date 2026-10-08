//! Real Qt transfer selections, questions, yes/no and destination restoration.
use hydrus_core::{HashId, Sha256};
use hydrus_gui_model::{
    local_transfer::Transfer,
    options::{Editor, Row, Settings},
};
use hydrus_store::{
    Store,
    content::TransferKind,
    settings::{self, LocalTransferPreferences},
};
use serde_json::{Value, json};
use std::sync::Arc;
const LABELS: [&str; 2] = [
    "Confirm when copying files across local file domains: ",
    "Confirm when moving files across local file domains: ",
];
fn store() -> (tempfile::TempDir, Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}
fn files(store: &Store, case: &Value) -> Vec<HashId> {
    case["before"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            let hash: Sha256 = f["hash"].as_str().unwrap().parse().unwrap();
            store
                .read(|c| hydrus_store::master::hash_id(c, &hash))
                .unwrap()
                .unwrap()
        })
        .collect()
}
fn seed(store: &Store, case: &Value) -> Vec<HashId> {
    let ids = files(store, case);
    let rows = case["before"].as_array().unwrap().clone();
    let captured = ids.clone();
    store
        .write_content(move |writer| {
            let domains = writer.roles().local.clone();
            for domain in &domains {
                writer.delete_files(*domain, &captured, None)?;
            }
            let source = writer.snapshot().services.by_name("art").unwrap().id;
            // First restore every file to a local domain, permitting deletion-record clear.
            writer.add_files(
                source,
                &captured
                    .iter()
                    .map(|&f| (f, Some(1_234_567_890_000)))
                    .collect::<Vec<_>>(),
            )?;
            writer.clear_local_delete_records(Some(&captured))?;
            for (&id, row) in captured.iter().zip(&rows) {
                for (name, time) in row["imports"].as_object().unwrap() {
                    if let Some(time) = time.as_i64() {
                        let domain = writer.snapshot().services.by_name(name).unwrap().id;
                        writer.add_files(domain, &[(id, Some(time))])?;
                    }
                }
                if row["domains"].as_array().unwrap().is_empty() {
                    writer.delete_files(source, &[id], None)?;
                }
                if row["inbox"].as_bool().unwrap() {
                    writer.inbox(&[id])?;
                } else {
                    writer.archive(&[id])?;
                }
            }
            Ok(())
        })
        .unwrap();
    ids
}
fn state(store: &Store, ids: &[HashId]) -> Value {
    let snap = store.snapshot();
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snap.services, None, ids))
        .unwrap();
    let rows = batch.results.iter().map(|media| {
        let mut domains = media.current.iter().filter_map(|location| snap.services.get(location.service).ok().filter(|service| service.service_type() == hydrus_core::ServiceType::LocalFileDomain).map(|service| service.name.clone())).collect::<Vec<_>>();
        domains.sort();
        let imports = ["art", "my files"].into_iter().map(|name| {
            let id = snap.services.by_name(name).unwrap().id;
            (name, media.current.iter().find(|location| location.service == id).and_then(|location| location.added).map(|time| time.0))
        }).collect::<std::collections::BTreeMap<_,_>>();
        json!({"hash":media.hash.to_string(),"domains":domains,"imports":imports,"inbox":media.inbox})
    }).collect::<Vec<_>>();
    json!(rows)
}

fn kind(case: &Value) -> TransferKind {
    match case["kind"].as_str().unwrap() {
        "copy" => TransferKind::Copy,
        "move" => TransferKind::Move,
        _ => TransferKind::Merge,
    }
}
// leaf: audit-options-files-and-trash-confirm-when-copying-files-across-local-file-domains
// leaf: audit-options-files-and-trash-confirm-when-moving-files-across-local-file-domains
#[test]
fn actual_qt_options_defaults_drafts_and_field_scoped_save() {
    let fixture = hydrus_testkit::fixture_json("local_transfer_confirmations.json");
    let (dir, store) = store();
    let values = |p: LocalTransferPreferences| json!([p.copy, p.move_files]);
    assert_eq!(
        values(store.read(settings::get).unwrap()),
        fixture["loaded"]
    );
    assert_eq!(
        values(LocalTransferPreferences::default()),
        fixture["defaults"]
    );
    for event in fixture["options"].as_array().unwrap() {
        let before = store.read(Settings::load).unwrap();
        let mut editor = Editor::new(before.clone());
        let page = editor
            .page_names()
            .iter()
            .position(|p| *p == "files and trash")
            .unwrap();
        editor.show_page(page);
        for (label, value) in LABELS.iter().zip(event["input"].as_array().unwrap()) {
            let index = editor
                .rows()
                .iter()
                .position(|r| matches!(r,Row::Opt{option,..} if option.label==*label))
                .unwrap();
            editor.check(index, value.as_bool().unwrap());
        }
        assert_eq!(values(store.read(settings::get).unwrap()), event["staged"]);
        let (after, _, errors) = editor.applied();
        assert!(errors.is_empty());
        store
            .write(move |ctx| after.save(ctx.conn(), &before))
            .unwrap();
        assert_eq!(
            values(
                Store::open(dir.path())
                    .unwrap()
                    .read(settings::get)
                    .unwrap()
            ),
            event["reopened"]
        );
    }
    // A concurrent edit to the other preference survives this draft's Apply.
    let before = store.read(Settings::load).unwrap();
    let mut after = before.clone();
    after.local_transfer.copy = false;
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &LocalTransferPreferences {
                    copy: true,
                    move_files: false,
                },
            )
        })
        .unwrap();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    assert_eq!(
        store
            .read(settings::get::<LocalTransferPreferences>)
            .unwrap(),
        LocalTransferPreferences {
            copy: false,
            move_files: false
        }
    );
}
// leaf: audit-options-files-and-trash-confirm-when-copying-files-across-local-file-domains
// leaf: audit-options-files-and-trash-confirm-when-moving-files-across-local-file-domains
#[test]
fn recorded_copy_strict_and_merge_questions_drive_real_memberships() {
    let fixture = hydrus_testkit::fixture_json("local_transfer_confirmations.json");
    for case in fixture["transfers"].as_array().unwrap() {
        let (_dir, store) = store();
        let ids = seed(&store, case);
        assert_eq!(state(&store, &ids), case["before"]);
        let confirm = case["confirm"].as_bool().unwrap();
        store
            .write(move |ctx| {
                settings::set(
                    ctx.conn(),
                    &LocalTransferPreferences {
                        copy: confirm,
                        move_files: confirm,
                    },
                )
            })
            .unwrap();
        let snap = store.snapshot();
        let destination = snap.services.by_name("my files").unwrap().id;
        let source = snap.services.by_name("art").unwrap().id;
        let kind = kind(case);
        let transfer = Transfer::load(
            &store,
            kind,
            destination,
            (kind != TransferKind::Copy).then_some(source),
            &ids,
        )
        .unwrap()
        .unwrap();
        assert_eq!(transfer.confirm, confirm);
        if confirm {
            assert_eq!(transfer.question, case["questions"][0].as_str().unwrap());
        }
        let planned = transfer
            .files
            .iter()
            .map(|&id| {
                store
                    .read(|c| hydrus_store::master::hash(c, id))
                    .unwrap()
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<_>>();
        if !confirm || case["accepted"].as_bool().unwrap() {
            assert_eq!(json!(planned), case["jobs"][0]);
            let started = hydrus_core::TimestampMs::now().0;
            transfer.apply(&store).unwrap();
            let ended = hydrus_core::TimestampMs::now().0;
            let mut actual = state(&store, &ids);
            let mut expected = case["after"].clone();
            // New additions use their transaction clock, unlike restored timestamps.
            for (actual, expected) in actual
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .zip(expected.as_array_mut().unwrap())
            {
                if expected["imports"]["my files"] == json!(1_700_000_000_000_i64) {
                    assert!(
                        (started..=ended)
                            .contains(&actual["imports"]["my files"].as_i64().unwrap())
                    );
                    actual["imports"]["my files"] = json!("new");
                    expected["imports"]["my files"] = json!("new");
                }
            }
            assert_eq!(actual, expected);
        } else {
            assert_eq!(state(&store, &ids), case["after"]);
        }
    }
}
// leaf: audit-options-files-and-trash-confirm-when-copying-files-across-local-file-domains
// leaf: audit-options-files-and-trash-confirm-when-moving-files-across-local-file-domains
#[test]
fn deleted_destination_restores_timestamp_and_stale_source_cannot_move() {
    let fixture = hydrus_testkit::fixture_json("local_transfer_confirmations.json");
    let case = &fixture["undelete"];
    let (_dir, store) = store();
    let ids = seed(&store, case);
    let snap = store.snapshot();
    let destination = snap.services.by_name("my files").unwrap().id;
    let source = snap.services.by_name("art").unwrap().id;
    let file = ids[0];
    store
        .write_content(move |w| {
            w.add_files(destination, &[(file, Some(1_234_567_890_200))])?;
            w.delete_files(destination, &[file], None)
        })
        .unwrap();
    let transfer = Transfer::load(
        &store,
        TransferKind::Move,
        destination,
        Some(source),
        &ids[..1],
    )
    .unwrap()
    .unwrap();
    transfer.apply(&store).unwrap();
    assert_eq!(state(&store, &ids), case["after"]);
    let ids = seed(&store, &fixture["transfers"][0]);
    let file = ids[0];
    let transfer = Transfer::load(
        &store,
        TransferKind::Move,
        destination,
        Some(source),
        &ids[..1],
    )
    .unwrap()
    .unwrap();
    store
        .write_content(move |w| w.delete_files(source, &[file], None))
        .unwrap();
    assert!(transfer.apply(&store).unwrap().is_empty());
    assert!(
        !store
            .read(|c| hydrus_store::media::current_domains(c, &[file]))
            .unwrap()[&file]
            .contains(&destination)
    );
}

// leaf: audit-options-files-and-trash-confirm-when-copying-files-across-local-file-domains
// leaf: audit-options-files-and-trash-confirm-when-moving-files-across-local-file-domains
#[test]
fn stale_destination_membership_and_reused_service_id_never_retarget_a_question() {
    let fixture = hydrus_testkit::fixture_json("local_transfer_confirmations.json");
    let (_dir, store) = store();
    let ids = seed(&store, &fixture["transfers"][0]);
    let snap = store.snapshot();
    let dest = snap.services.by_name("my files").unwrap().id;
    let source = snap.services.by_name("art").unwrap().id;
    let file = ids[0];
    let strict = Transfer::load(&store, TransferKind::Move, dest, Some(source), &[file])
        .unwrap()
        .unwrap();
    store
        .write_content(move |w| w.add_files(dest, &[(file, Some(1234))]))
        .unwrap();
    assert!(strict.apply(&store).unwrap().is_empty());
    assert!(
        store
            .read(|c| hydrus_store::media::current_domains(c, &[file]))
            .unwrap()[&file]
            .contains(&source)
    );
    let ids = seed(&store, &fixture["transfers"][0]);
    let transfer = Transfer::load(&store, TransferKind::Copy, dest, None, &ids[..1])
        .unwrap()
        .unwrap();
    store
        .write_and_refresh(move |ctx| {
            ctx.conn().execute(
                "DELETE FROM file_domain_current WHERE service_id=?1",
                [dest],
            )?;
            ctx.conn().execute(
                "DELETE FROM file_domain_deleted WHERE service_id=?1",
                [dest],
            )?;
            hydrus_store::services::delete(ctx.conn(), dest)?;
            hydrus_store::services::insert_with_id(
                ctx.conn(),
                dest,
                &hydrus_core::ServiceKey::new(vec![249; 32]),
                "replacement local domain",
                &hydrus_store::services::ServiceKind::LocalFiles,
            )
        })
        .unwrap();
    assert!(transfer.apply(&store).is_err());
    assert!(
        !store
            .read(|c| hydrus_store::media::current_domains(c, &ids[..1]))
            .unwrap()[&ids[0]]
            .contains(&dest)
    );
}
