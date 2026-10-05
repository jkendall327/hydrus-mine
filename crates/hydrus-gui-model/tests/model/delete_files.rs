//! Actual advanced deletion radio/reason behavior, recorded from the Qt panel.
use hydrus_core::{HashId, Sha256};
use hydrus_gui_model::delete_files::{Draft, Reason, ReasonQueue};
use hydrus_store::Store;
use hydrus_store::settings::{DeletionAction, DeletionPreferences};
use std::sync::Arc;
const DEFAULT_REASON: &str = "Deleted from Preview or Media Viewer.";

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
fn file(store: &Store, hash: &str) -> HashId {
    let hash: Sha256 = hash.parse().unwrap();
    store
        .read(|c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap()
}
fn preferences(store: &Store, prefs: DeletionPreferences) {
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &prefs))
        .unwrap();
}

#[test]
fn advanced_reasons_match_actual_qt_defaults_existing_and_remembered_selection() {
    let recording = hydrus_testkit::fixture_json("files_trash.json");
    for case in recording["advanced"].as_array().unwrap() {
        let (_dir, store) = store();
        let file = file(&store, case["hash"].as_str().unwrap());
        let existing = case["existing"].as_str().map(str::to_owned);
        store
            .write(move |ctx| {
                ctx.conn()
                    .execute("DELETE FROM file_deletion_reasons WHERE hash_id=?1", [file])?;
                if let Some(reason) = existing {
                    let reason_id = hydrus_store::master::intern_text(ctx.conn(), &reason)?;
                    ctx.conn().execute(
                        "INSERT INTO file_deletion_reasons(hash_id,reason_id) VALUES (?1,?2)",
                        rusqlite::params![file, reason_id],
                    )?;
                }
                Ok(())
            })
            .unwrap();
        preferences(
            &store,
            DeletionPreferences {
                advanced: true,
                remember_action: true,
                reasons: ["alpha", "beta", "beta", ""]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                remember_reason: case["remember"].as_bool().unwrap(),
                last_reason: case["last"].as_str().map(str::to_owned),
                ..DeletionPreferences::default()
            },
        );
        let mut draft = Draft::load(&store, &[file], None, DEFAULT_REASON).unwrap();
        let expected = &case["initial"];
        assert_eq!(
            draft
                .choices
                .iter()
                .map(|c| c.label.as_str())
                .collect::<Vec<_>>(),
            expected["actions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            draft
                .reasons
                .iter()
                .map(|r| r.label.as_str())
                .collect::<Vec<_>>(),
            expected["reasons"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(draft.action, expected["action"].as_u64().unwrap() as usize);
        assert_eq!(draft.reason, expected["reason"].as_u64().unwrap() as usize);
        assert_eq!(draft.custom, expected["custom"].as_str().unwrap());
        draft.reason = draft
            .reasons
            .iter()
            .position(|r| r.reason == Reason::Custom)
            .unwrap();
        draft.custom = "synthetic custom 日本".into();
        draft.apply(&store).unwrap();
        let after: DeletionPreferences = store.read(hydrus_store::settings::get).unwrap();
        assert_eq!(after.last_reason.as_deref(), case["saved_reason"].as_str());
        let batch = store
            .read(|c| hydrus_store::media::load(c, &store.snapshot().services, None, &[file]))
            .unwrap();
        assert_eq!(
            batch.results[0].deletion_reason.as_deref(),
            Some("synthetic custom 日本")
        );
        assert_eq!(
            after.last_action,
            Some(draft.choices[draft.action].action.clone())
        );
    }
}

#[test]
fn clean_delete_preserves_locked_files_and_remembered_fields_merge_with_options() {
    let (_dir, store) = store();
    let recording = hydrus_testkit::fixture_json("files_trash.json");
    let file = file(&store, recording["advanced"][0]["hash"].as_str().unwrap());
    preferences(
        &store,
        DeletionPreferences {
            advanced: true,
            remember_action: true,
            ..DeletionPreferences::default()
        },
    );
    let before_locked = store
        .read(|c| hydrus_store::media::load(c, &store.snapshot().services, None, &[file]))
        .unwrap();
    let original_reason = before_locked.results[0].deletion_reason.clone();
    let original_deleted = before_locked.results[0].deleted.clone();
    let mut draft = Draft::load(&store, &[file], None, DEFAULT_REASON).unwrap();
    draft.action = draft
        .choices
        .iter()
        .position(|c| c.action == DeletionAction::ClearRecord)
        .unwrap();
    hydrus_gui_model::media_actions::archive(&store, &[file]).unwrap();
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::delete_lock::DeleteLock {
                    archived: true,
                    ..hydrus_store::delete_lock::DeleteLock::default()
                },
            )
        })
        .unwrap();
    draft.apply(&store).unwrap();
    let batch = store
        .read(|c| hydrus_store::media::load(c, &store.snapshot().services, None, &[file]))
        .unwrap();
    assert!(
        batch.results[0].is_current_in(
            hydrus_store::content::DomainRoles::new(&store.snapshot().services)
                .unwrap()
                .local_file_storage
        )
    );
    assert_eq!(batch.results[0].deletion_reason, original_reason);
    assert_eq!(batch.results[0].deleted, original_deleted);
    hydrus_gui_model::media_actions::inbox(&store, &[file]).unwrap();
    draft.apply(&store).unwrap();
    let batch = store
        .read(|c| hydrus_store::media::load(c, &store.snapshot().services, None, &[file]))
        .unwrap();
    assert!(batch.results[0].current.is_empty());
    assert!(batch.results[0].deleted.is_empty());
    assert!(batch.results[0].deletion_reason.is_none());
    let before = store
        .read(hydrus_gui_model::options::Settings::load)
        .unwrap();
    let mut changed = before.clone();
    changed.deletion.confirm_archive = false;
    store
        .write(|ctx| {
            let mut prefs: DeletionPreferences = hydrus_store::settings::get(ctx.conn())?;
            prefs.last_reason = Some("remembered concurrently".into());
            hydrus_store::settings::set(ctx.conn(), &prefs)
        })
        .unwrap();
    store
        .write(move |ctx| changed.save(ctx.conn(), &before))
        .unwrap();
    let after: DeletionPreferences = store.read(hydrus_store::settings::get).unwrap();
    assert!(!after.confirm_archive);
    assert_eq!(
        after.last_reason.as_deref(),
        Some("remembered concurrently")
    );
}

#[test]
fn reason_queue_matches_recorded_cancel_empty_duplicate_movement_and_delete() {
    let recording = hydrus_testkit::fixture_json("files_trash.json");
    let expected = &recording["reason_queue"];
    let mut queue = ReasonQueue::new(
        &["alpha", "beta", "beta", ""]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>(),
    );
    for event in expected["events"].as_array().unwrap() {
        if let Some(value) = event["answer"].as_str() {
            if event["default"] == "beta" {
                queue.click(1, false, false);
                let key = queue.editing().unwrap().0;
                queue.replace(key, value.into());
            } else {
                queue.add(value.into());
            }
        }
        assert_eq!(
            queue.values(),
            event["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
    }
    queue.move_selected(false);
    assert_eq!(
        queue.values(),
        expected["up"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    queue.move_selected(true);
    assert_eq!(
        queue.values(),
        expected["down"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        queue.removal_question().as_deref(),
        expected["delete_question"][0].as_str()
    );
    queue.remove_selected();
    assert_eq!(
        queue.values(),
        expected["deleted_rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
}

#[test]
fn physical_only_dialog_remembers_builtin_and_custom_service_keys_like_qt() {
    let recording = hydrus_testkit::fixture_json("files_trash.json");
    for case in recording["physical_remember"].as_array().unwrap() {
        let (_dir, store) = store();
        let file = file(&store, recording["advanced"][0]["hash"].as_str().unwrap());
        hydrus_gui_model::media_actions::delete(
            &store,
            &[file],
            &hydrus_gui_model::media_actions::Deletion::ToTrash,
        )
        .unwrap();
        let decode = |value: &serde_json::Value| {
            value.as_str().map(|value| match value {
                "physical_delete" => DeletionAction::Physical,
                "clear_delete" => DeletionAction::ClearRecord,
                key => {
                    DeletionAction::Domain(hydrus_core::ServiceKey::new(hex::decode(key).unwrap()))
                }
            })
        };
        preferences(
            &store,
            DeletionPreferences {
                advanced: true,
                remember_action: true,
                last_action: decode(&case["before"]),
                ..DeletionPreferences::default()
            },
        );
        let mut draft = Draft::load(&store, &[file], None, DEFAULT_REASON).unwrap();
        draft.action = 0;
        draft.apply(&store).unwrap();
        let after: DeletionPreferences = store.read(hydrus_store::settings::get).unwrap();
        assert_eq!(after.last_action, decode(&case["after"]));
    }
}

#[test]
fn mixed_existing_reasons_match_qt_preserve_each_file_and_do_not_replace_last_reason() {
    let (_dir, store) = store();
    let recording = hydrus_testkit::fixture_json("files_trash.json");
    let single = file(&store, recording["advanced"][0]["hash"].as_str().unwrap());
    let snapshot = store.snapshot();
    let roles = hydrus_store::content::DomainRoles::new(&snapshot.services).unwrap();
    let ids = store
        .read(|c| {
            Ok(c.prepare("SELECT hash_id FROM hashes")?
                .query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<HashId>>>()?)
        })
        .unwrap();
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &ids))
        .unwrap();
    let double = batch
        .results
        .iter()
        .find(|m| {
            m.current
                .iter()
                .filter(|c| roles.local.contains(&c.service))
                .count()
                == 2
        })
        .unwrap()
        .hash_id;
    store.write(move |ctx| {
        for (file,text) in [(single,"alpha"),(double,"different existing")] {
            let reason=hydrus_store::master::intern_text(ctx.conn(),text)?;
            ctx.conn().execute("INSERT OR REPLACE INTO file_deletion_reasons(hash_id,reason_id) VALUES (?1,?2)",rusqlite::params![file,reason])?;
        }
        Ok(())
    }).unwrap();
    preferences(
        &store,
        DeletionPreferences {
            advanced: true,
            remember_action: true,
            reasons: ["alpha", "beta", "beta", ""]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            last_reason: Some("synthetic custom 日本".into()),
            last_action: Some(DeletionAction::ClearRecord),
            ..DeletionPreferences::default()
        },
    );
    let mut draft = Draft::load(&store, &[single, double], None, DEFAULT_REASON).unwrap();
    let expected = &recording["mixed_existing"];
    assert_eq!(
        draft
            .choices
            .iter()
            .map(|c| c.label.as_str())
            .collect::<Vec<_>>(),
        expected["actions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        draft
            .reasons
            .iter()
            .map(|c| c.label.as_str())
            .collect::<Vec<_>>(),
        expected["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(draft.reason, expected["reason"].as_u64().unwrap() as usize);
    assert_eq!(draft.action, expected["action"].as_u64().unwrap() as usize);
    draft.action = draft
        .choices
        .iter()
        .position(|c| c.label == "Delete from all local services? (force send to trash)")
        .unwrap();
    draft.apply(&store).unwrap();
    let after = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[single, double]))
        .unwrap();
    assert_eq!(after.results[0].deletion_reason.as_deref(), Some("alpha"));
    assert_eq!(
        after.results[1].deletion_reason.as_deref(),
        Some("different existing")
    );
    let prefs: DeletionPreferences = store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(prefs.last_reason.as_deref(), Some("synthetic custom 日本"));
}
