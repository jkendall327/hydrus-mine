//! "After archive/delete filter, ensure deletees are inboxed before delete"
//! (Options > files and trash > delete lock) against the reference's own
//! filter commit, replaying `oracle/fixtures/archive_delete_reinbox.json`
//! (`oracle/record_archive_delete_reinbox.py`): for each setting of it and of
//! the archived-file delete lock, set through the real Options window, the
//! recorded files are put in the inbox or archived, kept or deleted one by
//! one in the real archive/delete filter window and committed; each file's
//! inbox, "my files" and trash state is then the reference's, and so is
//! what the real maintenance runtime's trash pass (limited to 0 MB) leaves
//! stored.

use std::collections::BTreeMap;

use hydrus_core::{HashId, Sha256};
use hydrus_gui::{Pages, SearchPage};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::maintenance_gates::Worker;
use serde_json::Value;
use slint::ComponentHandle as _;

use super::normal_time_maintenance::wait;
use super::options_system_consumers::{Client, check, client, noneable, store};

const LOCK: &str = "Do not permit archived files to be deleted from the trash: ";
const REINBOX: &str = "After archive/delete filter, ensure deletees are inboxed before delete: ";

/// Each named file's (inbox, in "my files", in the trash, stored).
fn states(client: &Client, files: &[(String, HashId)]) -> BTreeMap<String, Value> {
    let snapshot = client.store.snapshot();
    let services = &snapshot.services;
    let key = |k: &[u8]| services.builtin(k).unwrap().id;
    let my_files = key(hydrus_core::service::builtin_keys::MY_FILES);
    let trash = key(hydrus_core::service::builtin_keys::TRASH);
    let storage = hydrus_store::content::DomainRoles::new(services)
        .unwrap()
        .local_file_storage;
    files
        .iter()
        .map(|(name, id)| {
            let id = *id;
            let (inbox, domains): (bool, Vec<hydrus_core::ServiceId>) = client
                .store
                .read(move |c| {
                    let inbox = c
                        .query_row("SELECT 1 FROM file_inbox WHERE hash_id = ?1", [id], |_| Ok(()))
                        .is_ok();
                    let mut q =
                        c.prepare("SELECT service_id FROM file_domain_current WHERE hash_id = ?1")?;
                    let domains = q.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
                    Ok((inbox, domains))
                })
                .unwrap();
            let stored = domains.contains(&storage);
            (
                name.clone(),
                serde_json::json!({
                    // (a file deleted for good is in no inbox)
                    "inbox": inbox && stored,
                    "my_files": domains.contains(&my_files),
                    "trash": domains.contains(&trash),
                    "stored": stored,
                }),
            )
        })
        .collect()
}

// leaf: audit-options-files-and-trash-delete-lock-after-archive-delete-filter-ensure-deletees-are-inboxed-before-delete
#[test]
fn the_archive_delete_filter_reinboxes_deletees_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("archive_delete_reinbox.json");
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    let hash_of = |name: &str| -> Sha256 {
        manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == name)
            .unwrap()["hash"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()
    };
    let names: Vec<String> = recorded["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_owned())
        .collect();
    let kept: Vec<&str> = recorded["kept"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    let archived: Vec<&str> = recorded["archived_before"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    assert_eq!(recorded["deleted_from"], "my files");
    let mut client = client();
    for setting in recorded["settings"].as_array().unwrap() {
        let (lock, reinbox) = (
            setting["lock"].as_bool().unwrap(),
            setting["reinbox"].as_bool().unwrap(),
        );
        let context = format!("lock {lock}, reinbox {reinbox}");
        // a fresh `basic` store, its trash emptied
        let (dirs, fresh) = store();
        client.store = fresh;
        client._dirs = dirs;
        let store = client.store.clone();
        let files: Vec<(String, HashId)> = names
            .iter()
            .map(|name| {
                let hash = hash_of(name);
                let id = store
                    .read(move |c| hydrus_store::master::hash_id(c, &hash))
                    .unwrap()
                    .unwrap();
                (name.clone(), id)
            })
            .collect();
        let inbox: Vec<HashId> = files.iter().map(|f| f.1).collect();
        let archive: Vec<HashId> = files
            .iter()
            .filter(|f| archived.contains(&f.0.as_str()))
            .map(|f| f.1)
            .collect();
        store
            .write_content(move |w| {
                let trash = w.roles().trash;
                let storage = w.roles().local_file_storage;
                let mut q = w
                    .conn()
                    .prepare("SELECT hash_id FROM file_domain_current WHERE service_id = ?1")?;
                let trashed: Vec<HashId> =
                    q.query_map([trash], |r| r.get(0))?.collect::<Result<_, _>>()?;
                drop(q);
                w.delete_files(storage, &trashed, None)?;
                w.inbox(&inbox)?;
                w.archive(&archive)
            })
            .unwrap();
        // the files on a "my files" page, in the recorded order
        let my_files = store
            .snapshot()
            .services
            .builtin(hydrus_core::service::builtin_keys::MY_FILES)
            .unwrap()
            .key
            .clone();
        let page = SearchPage::restored(
            store.clone(),
            FileSearchContext {
                location: LocationContext::single(my_files),
                ..Default::default()
            },
            false,
            None,
            files.iter().map(|f| f.1).collect(),
        );
        client.bound = hydrus_gui::bind(&client.ui, Pages::single(page));
        // the options, through File > options
        let window = client.options("files and trash");
        check(&window, LOCK, lock);
        check(&window, REINBOX, reinbox);
        window.invoke_apply();

        // keep or delete each, then commit
        client.ui.invoke_archive_delete_filter();
        let filter = client
            .bound
            .archive_delete
            .borrow()
            .as_ref()
            .expect("the filter opened")
            .clone_strong();
        assert_eq!(filter.get_caption(), format!("1/{}", files.len()));
        for (name, _) in &files {
            if kept.contains(&name.as_str()) {
                filter.invoke_keep();
            } else {
                filter.invoke_delete();
            }
        }
        let question = filter.get_question().to_string();
        assert!(
            question.starts_with(&format!(
                "keep {} and delete {} from ",
                kept.len(),
                files.len() - kept.len()
            )),
            "{question}"
        );
        filter.invoke_commit();
        assert!(client.bound.archive_delete.borrow().is_none(), "it closed");
        let shown = states(&client, &files);
        let expected: BTreeMap<String, Value> = setting["after_commit"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert_eq!(shown, expected, "after the commit, {context}");

        // empty the trash: the lock keeps what it keeps
        let window = client.options("files and trash");
        noneable(
            &window,
            "Number of hours a file will stay in the trash before being deleted: ",
            "no age limit",
            None,
        );
        noneable(
            &window,
            "Maximum size of trash (MB): ",
            "no size limit",
            Some(0),
        );
        window.invoke_apply();
        let maintenance = &client.bound.maintenance;
        let passes = maintenance.statistics().trash_passes;
        let due = maintenance.deadline(Worker::Trash);
        wait(|| {
            maintenance.poll_at(due).unwrap();
            maintenance.statistics().trash_passes > passes
        });
        assert_eq!(maintenance.statistics().last_error, None);
        let expected: BTreeMap<String, Value> = setting["after_emptying_trash"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert_eq!(
            states(&client, &files),
            expected,
            "after emptying the trash, {context}"
        );
    }
}
