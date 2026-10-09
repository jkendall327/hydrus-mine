//! "Allow trash maintenance during normal time" against the reference's trash
//! daemon (`normal_time_maintenance.json`, recorded by
//! `oracle/record_normal_time_maintenance.py` on the real
//! `DAEMONMaintainTrash`): the option set in the real Options window, the
//! GUI's real trash worker admitted or not by it and the live idle state, an
//! option change during an admitted pass not stopping it, and an exit between
//! two groups of eight stopping before the second.

use std::sync::{Arc, mpsc};
use std::time::Duration;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::HashId;
use hydrus_gui::{Bound, MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::maintenance_gates::{self, Worker};
use hydrus_store::settings::{self, GuiIdleSettings};

const LABEL: &str = "Allow trash maintenance during normal time: ";

/// The basic store with `n` files in the trash, all over a 0 MB limit.
fn trashed(n: usize) -> ([tempfile::TempDir; 2], Arc<Store>, Vec<HashId>) {
    let (dirs, store) = super::namespace_sorts::store();
    let storage = hydrus_store::content::DomainRoles::new(&store.snapshot().services)
        .unwrap()
        .local_file_storage;
    let files: Vec<HashId> = store
        .read(move |c| {
            let mut q = c.prepare(
                "SELECT hash_id FROM file_domain_current WHERE service_id = ? ORDER BY hash_id",
            )?;
            Ok(q.query_map([storage], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .unwrap();
    assert!(files.len() >= n);
    let files = files[..n].to_vec();
    let mine = files.clone();
    store
        .write_content(move |w| w.delete_files(w.roles().combined_local_media, &files, None))
        .unwrap();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &hydrus_store::trash::TrashSettings {
                    max_age_hours: None,
                    max_size_mb: Some(0),
                },
            )?;
            settings::set(
                ctx.conn(),
                &hydrus_store::maintenance_gates::Preferences {
                    trash_normal: false,
                    deferred_normal: false,
                },
            )
        })
        .unwrap();
    (dirs, store, mine)
}

/// How many of `files` are in the trash.
fn in_trash(store: &Store, files: &[HashId]) -> usize {
    let trash = hydrus_store::content::DomainRoles::new(&store.snapshot().services)
        .unwrap()
        .trash;
    let files = files.to_vec();
    store
        .read(move |c| hydrus_store::media::current_in(c, trash, &files))
        .unwrap()
        .len()
}

/// How many files are in the trash.
fn trash_size(store: &Store) -> usize {
    let trash = hydrus_store::content::DomainRoles::new(&store.snapshot().services)
        .unwrap()
        .trash;
    store
        .read(move |c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM file_domain_current WHERE service_id = ?",
                [trash],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .map(|n| usize::try_from(n).unwrap())
        .unwrap()
}

/// Options > files and trash, the trash row set and applied.
fn set_normal_time(ui: &MainWindow, bound: &Bound, on: bool) {
    ui.invoke_menu_title_pressed(0, 20., 22.);
    let index = ui
        .get_menu_panes()
        .row_data(0)
        .unwrap()
        .lines
        .iter()
        .position(|row| row.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0., 0., 0.);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|page| page.text == "files and trash")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    let row = window
        .get_rows()
        .iter()
        .position(|row| row.label == LABEL)
        .unwrap();
    window.invoke_check_toggled(i32::try_from(row).unwrap(), on);
    window.invoke_apply();
}

/// The client idle (no idle timers to wait for) or in normal use.
fn be_idle(store: &Store, bound: &Bound, idle: bool, at: i64) {
    if idle {
        store
            .write(|ctx| {
                settings::set(
                    ctx.conn(),
                    &GuiIdleSettings {
                        enabled: true,
                        user_seconds: None,
                        mouse_seconds: None,
                        api_seconds: None,
                        busy_cpu_percent: 50,
                        busy_cpu_count: None,
                    },
                )
            })
            .unwrap();
        assert!(bound.session_autosave.idle_at(at));
    } else {
        bound.session_autosave.user_at(at);
        assert!(!bound.session_autosave.idle_at(at));
    }
}

/// Poll the worker at `at` until its pass, if one was admitted, is done.
fn run_pass(bound: &Bound, at: i64) -> bool {
    bound.maintenance.poll_at(at).unwrap();
    let admitted = bound.maintenance.running(Worker::Trash);
    super::normal_time_maintenance::wait(|| {
        bound.maintenance.poll_at(at).unwrap();
        !bound.maintenance.running(Worker::Trash)
    });
    admitted
}

struct Client {
    _dirs: [tempfile::TempDir; 2],
    store: Arc<Store>,
    mine: Vec<HashId>,
    ui: MainWindow,
    bound: Bound,
}

fn client(trashed_files: usize) -> Client {
    let (dirs, store, mine) = trashed(trashed_files);
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    Client {
        _dirs: dirs,
        store,
        mine,
        ui,
        bound,
    }
}

// leaf: audit-options-files-and-trash-allow-trash-maintenance-during-normal-time
#[test]
fn the_trash_worker_runs_in_normal_time_as_the_reference_s_does() {
    let _windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("normal_time_maintenance.json");

    // each recorded admission: one file in the trash over its limit, the
    // client idle or in normal use, the option off or on
    for pass in fixture["passes"].as_array().unwrap() {
        if pass["worker"] != "trash" {
            continue;
        }
        let idle = pass["idle"].as_bool().unwrap();
        let normal = pass["normal"].as_bool().unwrap();
        let what = format!("idle {idle}, normal time {normal}");
        let c = client(1);
        set_normal_time(&c.ui, &c.bound, normal);
        assert_eq!(
            c.store.read(maintenance_gates::load).unwrap().trash_normal,
            normal
        );
        // (idle time starts a while after the client does)
        let at = c.bound.maintenance.started_ms() + if idle { 600_000 } else { 30_000 };
        c.bound
            .session_autosave
            .user_at(c.bound.maintenance.started_ms());
        be_idle(&c.store, &c.bound, idle, at);
        let admitted = run_pass(&c.bound, at);
        // (the reference reads the trash only when admitted)
        let read = pass["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e[0] == "read");
        assert_eq!(admitted, read, "{what}");
        assert_eq!(
            in_trash(&c.store, &c.mine),
            usize::try_from(pass["remaining"].as_u64().unwrap()).unwrap(),
            "{what}"
        );
    }

    // an admitted pass of sixteen (two groups of eight) carries on though the
    // option is turned off after its first group
    let recorded = &fixture["trash_admitted_setting_change"];
    let writes: Vec<u64> = recorded["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e[0] == "write")
        .map(|e| e[2].as_u64().unwrap())
        .collect();
    assert_eq!(writes, [8, 8]);
    let c = client(16);
    let start = trash_size(&c.store);
    set_normal_time(&c.ui, &c.bound, true);
    let (after_first, first_written) = mpsc::channel::<()>();
    let (go_on, carry_on) = mpsc::channel::<()>();
    let groups = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    c.bound.maintenance.purge_control().after_each_write({
        let groups = groups.clone();
        move || {
            if groups.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                after_first.send(()).unwrap();
                carry_on.recv().unwrap();
            }
        }
    });
    let at = c.bound.maintenance.started_ms() + 30_000;
    be_idle(&c.store, &c.bound, false, at);
    c.bound.maintenance.poll_at(at).unwrap();
    assert!(c.bound.maintenance.running(Worker::Trash), "admitted");
    first_written
        .recv_timeout(Duration::from_secs(20))
        .expect("the first group");
    assert_eq!(trash_size(&c.store), start - 8, "one group written");
    set_normal_time(&c.ui, &c.bound, false);
    go_on.send(()).unwrap();
    super::normal_time_maintenance::wait(|| {
        c.bound.maintenance.poll_at(at).unwrap();
        !c.bound.maintenance.running(Worker::Trash)
    });
    // (the reference's whole queue went: nothing left)
    assert_eq!(
        trash_size(&c.store),
        usize::try_from(recorded["remaining"].as_u64().unwrap()).unwrap()
    );
    assert_eq!(
        c.store.read(maintenance_gates::load).unwrap().trash_normal,
        recorded["saved_normal"].as_bool().unwrap()
    );
    // (groups of eight: the reference's sixteen were two; the store's own
    // trashed files add a third here)
    assert_eq!(
        groups.load(std::sync::atomic::Ordering::SeqCst),
        start.div_ceil(8)
    );

    // exiting between the groups stops the pass before the second
    let recorded = &fixture["trash_shutdown_between_groups"];
    let c = client(16);
    let start = trash_size(&c.store);
    set_normal_time(&c.ui, &c.bound, true);
    let (after_first, first_written) = mpsc::channel::<()>();
    let (go_on, carry_on) = mpsc::channel::<()>();
    let groups = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    c.bound.maintenance.purge_control().after_each_write({
        let groups = groups.clone();
        move || {
            if groups.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                after_first.send(()).unwrap();
                carry_on.recv().unwrap();
            }
        }
    });
    let at = c.bound.maintenance.started_ms() + 30_000;
    be_idle(&c.store, &c.bound, false, at);
    c.bound.maintenance.poll_at(at).unwrap();
    first_written
        .recv_timeout(Duration::from_secs(20))
        .expect("the first group");
    // (the exit: the binding retires its work)
    c.bound.maintenance.retire();
    go_on.send(()).unwrap();
    // (the worker sees it is retired before another write; give it time to
    // write one if it would)
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(groups.load(std::sync::atomic::Ordering::SeqCst), 1);
    // (the reference left `remaining` of the sixteen)
    assert_eq!(
        start - trash_size(&c.store),
        16 - usize::try_from(recorded["remaining"].as_u64().unwrap()).unwrap()
    );
}
