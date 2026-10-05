//! Actual staged flags, cancellation/reopen and concurrent independent worker gates.
use hydrus_gui_model::options::{Editor, Row, Settings};
use hydrus_store::{
    Store,
    maintenance_gates::{self, Preferences, Worker},
    settings,
};
const LABELS: [&str; 2] = [
    "Allow trash maintenance during normal time: ",
    "Allow deferred file deletes during normal time: ",
];
fn editor(store: &Store) -> (Editor, [usize; 2]) {
    let mut draft = Editor::new(store.read(Settings::load).unwrap());
    let page = draft
        .page_names()
        .iter()
        .position(|p| *p == "files and trash")
        .unwrap();
    draft.show_page(page);
    let rows = std::array::from_fn(|i| {
        draft
            .rows()
            .iter()
            .position(|row| matches!(row,Row::Opt {option,..} if option.label==LABELS[i]))
            .unwrap()
    });
    (draft, rows)
}
#[test]
fn recorded_flags_cancel_apply_reopen_and_live_peer_preservation() {
    let fixture = hydrus_testkit::fixture_json("normal_time_maintenance.json");
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    for case in fixture["controls"].as_array().unwrap() {
        store
            .write(|ctx| settings::set(ctx.conn(), &Preferences::default()))
            .unwrap();
        let (mut draft, rows) = editor(&store);
        for (i, row) in rows.into_iter().enumerate() {
            draft.check(row, case["typed"][i].as_bool().unwrap());
        }
        if case["apply"] == true {
            let (after, before, errors) = draft.applied();
            assert!(errors.is_empty());
            let before = before.clone();
            store
                .write(move |ctx| after.save(ctx.conn(), &before))
                .unwrap();
        }
        let saved = store.read(maintenance_gates::load).unwrap();
        assert_eq!(
            [saved.trash_normal, saved.deferred_normal],
            [
                case["saved"][0].as_bool().unwrap(),
                case["saved"][1].as_bool().unwrap()
            ]
        );
        let saved = Store::open(directory.path())
            .unwrap()
            .read(maintenance_gates::load)
            .unwrap();
        assert_eq!(
            [saved.trash_normal, saved.deferred_normal],
            [
                case["reopened"][0].as_bool().unwrap(),
                case["reopened"][1].as_bool().unwrap()
            ]
        );
    }
    store
        .write(|ctx| settings::set(ctx.conn(), &Preferences::default()))
        .unwrap();
    let (mut draft, rows) = editor(&store);
    draft.check(rows[0], false);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &Preferences {
                    trash_normal: true,
                    deferred_normal: false,
                },
            )?;
            settings::set(
                ctx.conn(),
                &hydrus_store::physical_delete::Preferences { wait_ms: 900 },
            )
        })
        .unwrap();
    let (after, before, errors) = draft.applied();
    assert!(errors.is_empty());
    let before = before.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    assert_eq!(
        store.read(maintenance_gates::load).unwrap(),
        Preferences {
            trash_normal: false,
            deferred_normal: false
        }
    );
    assert_eq!(
        store
            .read(hydrus_store::physical_delete::load)
            .unwrap()
            .wait_ms,
        900
    );
    for case in fixture["passes"].as_array().unwrap() {
        let worker = if case["worker"] == "trash" {
            Worker::Trash
        } else {
            Worker::Deferred
        };
        let normal = case["normal"].as_bool().unwrap();
        let policy = Preferences {
            trash_normal: normal,
            deferred_normal: normal,
        };
        let admitted = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e[0] == "read");
        assert_eq!(
            policy.allows(worker, case["idle"].as_bool().unwrap()),
            admitted
        );
    }
}
