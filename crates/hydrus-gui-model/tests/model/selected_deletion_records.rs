//! Accepted identities, real import status and independent 64-file commits.
use hydrus_core::Sha256;
use hydrus_gui_model::selected_deletion_records::Plan;
use serde_json::json;
#[path = "../support/selected_deletion_records_seed.rs"]
mod seed;

#[test]
fn captured_clearable_records_match_actual_reference_changes_during_question() {
    let fixture = hydrus_testkit::fixture_json("selected_deletion_records.json");
    let (_dirs, store) = seed::store();
    let files = seed::files(&store);
    for case in fixture["actions"].as_array().unwrap() {
        seed::reset(&store, &files);
        assert_eq!(seed::state(&store, &files), case["before"]);
        let selected: Vec<_> = case["selected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| files[usize::try_from(i.as_u64().unwrap()).unwrap()])
            .collect();
        let plan = Plan::capture(&store, &selected).unwrap();
        let questions = case["questions"].as_array().unwrap();
        if questions.is_empty() {
            assert!(plan.files.is_empty());
        } else {
            assert_eq!(plan.question(), questions[0]["message"]);
        }
        let local = store.snapshot().services.by_name("my files").unwrap().id;
        match case["case"].as_str().unwrap() {
            "readd_during_question" => {
                let file = files[0];
                store
                    .write_content(move |writer| {
                        writer.add_files(local, &[(file, Some(1_700_000_000_100))])
                    })
                    .unwrap();
            }
            "new_delete_during_question" => {
                let file = files[3];
                store
                    .write_content(move |writer| {
                        writer.delete_files(writer.roles().combined_local_media, &[file], None)?;
                        writer.delete_files(writer.roles().local_file_storage, &[file], None)
                    })
                    .unwrap();
            }
            _ => {}
        }
        let queued = seed::queue(&store);
        if case["accepted"].as_bool().unwrap() {
            plan.apply(&store).unwrap();
        }
        assert_eq!(
            seed::state(&store, &files),
            case["after"],
            "{}",
            case["case"]
        );
        assert_eq!(
            seed::queue(&store),
            queued,
            "record clear must preserve physical deletion queue"
        );
    }
}

#[test]
fn real_writer_later_batch_error_preserves_first_commit_and_remaining_records() {
    let (_dirs, store) = seed::store();
    let original = seed::files(&store)[0];
    let roles = hydrus_store::content::DomainRoles::new(&store.snapshot().services).unwrap();
    let local = store.snapshot().services.by_name("my files").unwrap().id;
    let files=store.write_content(move |writer| {
        let mut files=Vec::new();
        for i in 1..=130 {
            let hash:Sha256=format!("{i:064x}").parse().unwrap();
            let id=hydrus_store::master::intern_hash(writer.conn(),&hash)?;
            writer.conn().execute("INSERT INTO files(hash_id,size,mime) SELECT ?1,size,mime FROM files WHERE hash_id=?2",rusqlite::params![id,original])?;
            files.push(id);
        }
        let rows:Vec<_>=files.iter().map(|&file|(file,Some(1_700_000_000_000))).collect();
        writer.add_files(local,&rows)?;
        writer.delete_files(roles.combined_local_media,&files,None)?;
        writer.delete_files(roles.local_file_storage,&files,None)?;
        Ok(files)
    }).unwrap();
    let plan = Plan::capture(&store, &files).unwrap();
    let fixture = hydrus_testkit::fixture_json("selected_deletion_records.json");
    assert_eq!(
        json!(plan.files.chunks(64).map(<[_]>::len).collect::<Vec<_>>()),
        fixture["batches"][2]["sizes"]
    );
    let failure = files[64];
    store.write(move |ctx| {
        ctx.conn().execute_batch(&format!("CREATE TRIGGER fail_second_clear BEFORE DELETE ON file_domain_deleted WHEN OLD.hash_id={} AND OLD.service_id={} BEGIN SELECT RAISE(ABORT,'scripted later batch failure'); END;",failure.get(),roles.local_file_storage.get()))?;
        Ok(())
    }).unwrap();
    let queue = seed::queue(&store);
    assert!(plan.apply(&store).is_err());
    let after = seed::state(&store, &files);
    assert!(
        after["status"].as_array().unwrap()[..64]
            .iter()
            .all(|status| status == 0)
    );
    assert!(
        after["status"].as_array().unwrap()[64..]
            .iter()
            .all(|status| status == 3)
    );
    assert_eq!(seed::queue(&store), queue);
    store
        .write(|ctx| {
            ctx.conn().execute_batch("DROP TRIGGER fail_second_clear")?;
            Ok(())
        })
        .unwrap();
    Plan::capture(&store, &files)
        .unwrap()
        .apply(&store)
        .unwrap();
    assert!(
        seed::state(&store, &files)["status"]
            .as_array()
            .unwrap()
            .iter()
            .all(|status| status == 0)
    );
}
