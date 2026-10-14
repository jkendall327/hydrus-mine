//! The tag migration window's mapping-count gates for sibling and parent pairs, chosen
//! with its check boxes and "in service" selector and replayed against the reference's
//! (`oracle/fixtures/tag_archives.json`, `counts`), from an archive and from a service.
use hydrus_core::{HashId, ServiceKey, Tag};
use hydrus_gui::{headless, tag_migration_window};
use hydrus_store::{
    Store,
    content::{
        MappingAction,
        tag_relations::{self, RelationAction, RelationUpdate},
    },
    display::RelationKind,
    services::{self, ServiceKind},
};
use slint::Model as _;

// leaf: migration-pair-count
#[test]
fn the_window_s_count_gates_pick_the_pairs_the_reference_picked() {
    let recording = hydrus_testkit::fixture_json("tag_archives.json");
    let _windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let local = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let repo = ServiceKey::new(vec![66; 16]);
    let key = repo.clone();
    store
        .write_and_refresh(move |ctx| {
            services::insert(
                ctx.conn(),
                &key,
                "count repository",
                &ServiceKind::TagRepository(services::RepositoryConfig::default()),
            )?;
            Ok(())
        })
        .unwrap();
    let sid = store.snapshot().services.by_key(&local).unwrap().id;
    let rid = store.snapshot().services.by_key(&repo).unwrap().id;
    store
        .write_content(move |writer| {
            let file =
                hydrus_store::master::intern_hash(writer.conn(), &hydrus_core::Sha256([1; 32]))?;
            let file: HashId = file;
            for (service, action, text) in [
                (sid, MappingAction::Add, "archive:left"),
                (sid, MappingAction::Add, "archive:ideal"),
                (rid, MappingAction::Pend, "archive:pending"),
            ] {
                let tag =
                    hydrus_store::master::intern_tag(writer.conn(), &Tag::new(text).unwrap())?;
                writer.update_mappings(service, &action, tag, &[file])?;
            }
            Ok(())
        })
        .unwrap();
    tag_relations::apply(
        &store,
        RelationKind::Siblings,
        vec![RelationUpdate {
            service: sid,
            left: Tag::new("archive:right").unwrap(),
            right: Tag::new("archive:ideal").unwrap(),
            action: RelationAction::Add,
        }],
    )
    .unwrap();
    for (content, kind) in [
        ("siblings", RelationKind::Siblings),
        ("parents", RelationKind::Parents),
    ] {
        let archive = recording["archives"]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["kind"] == content)
            .unwrap();
        let updates = archive["pairs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| RelationUpdate {
                service: sid,
                left: Tag::new(pair[0].as_str().unwrap()).unwrap(),
                right: Tag::new(pair[1].as_str().unwrap()).unwrap(),
                action: RelationAction::Add,
            })
            .collect();
        tag_relations::apply(&store, kind, updates).unwrap();
    }
    for (index, case) in recording["counts"].as_array().unwrap().iter().enumerate() {
        for archive_source in [true, false] {
            let label = format!("{case} from an archive: {archive_source}");
            let slot = tag_migration_window::Slot::default();
            let window =
                tag_migration_window::open(&store, &local, vec![], &slot, std::rc::Rc::new(|| {}))
                    .unwrap();
            let siblings = case["content"] == "siblings";
            window.set_content(if siblings { 1 } else { 2 });
            window.invoke_choices_changed();
            let services: Vec<String> = window
                .get_services()
                .iter()
                .map(|s| s.to_string())
                .collect();
            let archive = i32::try_from(services.len() - 1).unwrap();
            let at = |name: &str| {
                i32::try_from(services.iter().position(|s| s == name).unwrap()).unwrap()
            };
            window.set_source(if archive_source {
                archive
            } else {
                at("my tags")
            });
            window.set_destination(archive);
            window.invoke_choices_changed();
            if archive_source {
                let path = hydrus_testkit::fixture_path(format!(
                    "tag_archive_{}.db",
                    case["content"].as_str().unwrap()
                ));
                window.invoke_archive_path_chosen(true, path.to_string_lossy().as_ref().into());
            }
            let output = dir
                .path()
                .join(format!("counts-{index}-{archive_source}.db"));
            window.invoke_archive_path_chosen(false, output.to_string_lossy().as_ref().into());
            // The gates: the check boxes, and the service whose counts are asked.
            let count_services: Vec<String> = window
                .get_count_services()
                .iter()
                .map(|s| s.to_string())
                .collect();
            let wanted = if case["service"] == "local" {
                "my tags"
            } else {
                "count repository"
            };
            window.set_count_service(
                i32::try_from(count_services.iter().position(|s| s == wanted).unwrap()).unwrap(),
            );
            window.set_count_left(case["left"].as_bool().unwrap());
            window.set_count_right(case["right"].as_bool().unwrap());
            window.set_count_either(case["either"].as_bool().unwrap());
            window.invoke_choices_changed();
            assert!(
                window.get_error().is_empty(),
                "{label}: {}",
                window.get_error()
            );
            window.invoke_go();
            // The summary the user confirms names the enabled gates and the service.
            let summary = window.get_question().to_string();
            assert!(summary.contains(wanted), "{label}: {summary}");
            while !window.get_question().is_empty() {
                window.invoke_answer(true);
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            while window.get_running() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(50));
                slint::platform::update_timers_and_animations();
            }
            assert!(!window.get_running(), "{label}");
            assert!(
                window.get_error().is_empty(),
                "{label}: {}",
                window.get_error()
            );
            let conn = rusqlite::Connection::open(&output).unwrap();
            let rows = conn
                .prepare("SELECT a.tag,b.tag FROM pairs p JOIN tags a ON p.tag_id_1=a.tag_id JOIN tags b ON p.tag_id_2=b.tag_id ORDER BY a.tag,b.tag")
                .unwrap()
                .query_map([], |r| Ok([r.get::<_, String>(0)?, r.get::<_, String>(1)?]))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(serde_json::json!(rows), case["pairs"], "{label}");
            window.invoke_close_clicked();
        }
    }
}
