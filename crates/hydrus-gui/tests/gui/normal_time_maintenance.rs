//! Saved normal-time gates reach real off-thread trash/deferred consumers and live idle.
use hydrus_core::{HashId, Mime, Sha256};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{
    Store,
    content::DomainRoles,
    maintenance_gates::{self, Preferences, Worker},
    settings::{self, GuiIdleSettings},
    transfer::{TransferMode, transfer_media},
};
use slint::{ComponentHandle as _, Model as _};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
const LABELS: [&str; 2] = [
    "Allow trash maintenance during normal time: ",
    "Allow deferred file deletes during normal time: ",
];
fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> (OptionsWindow, [i32; 2]) {
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
    let rows = std::array::from_fn(|i| {
        i32::try_from(
            window
                .get_rows()
                .iter()
                .position(|row| row.label == LABELS[i])
                .unwrap(),
        )
        .unwrap()
    });
    (window, rows)
}
pub(super) fn owned() -> (
    [tempfile::TempDir; 2],
    Arc<Store>,
    Vec<(HashId, std::path::PathBuf)>,
) {
    let (dirs, store) = super::namespace_sorts::store();
    let database = store.dir().join(hydrus_store::store::DB_FILE_NAME);
    // The transfer updates storage locations directly; reopen their snapshot.
    drop(store);
    transfer_media(
        &database,
        &dirs[1].path().join("owned-media"),
        TransferMode::Copy,
    )
    .unwrap();
    let store = Store::open(dirs[1].path()).unwrap();
    let snapshot = store.snapshot();
    let storage = DomainRoles::new(&snapshot.services)
        .unwrap()
        .local_file_storage;
    let files=store.read(|conn| {
        let mut query=conn.prepare("SELECT h.hash_id,h.sha256,f.mime FROM file_domain_current d JOIN hashes h USING(hash_id) JOIN files f USING(hash_id) WHERE service_id=? ORDER BY hash_id LIMIT 4")?;
        let rows=query.query_map([storage],|r|Ok((r.get::<_,HashId>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,u8>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().map(|(id,hash,mime)|(id,snapshot.storage.file_path(&Sha256::from_slice(&hash).unwrap(),Mime::from_code(mime).unwrap()).unwrap())).collect::<Vec<_>>())
    }).unwrap();
    assert_eq!(files.len(), 4);
    assert!(files.iter().all(|(_, p)| p.is_file()));
    store
        .write(|ctx| {
            let mut folders: settings::FolderSettings = settings::get(ctx.conn())?;
            folders.delete_to_recycle_bin = false;
            settings::set(ctx.conn(), &folders)?;
            settings::set(
                ctx.conn(),
                &Preferences {
                    trash_normal: false,
                    deferred_normal: false,
                },
            )?;
            settings::set(
                ctx.conn(),
                &hydrus_store::physical_delete::Preferences { wait_ms: 20 },
            )?;
            settings::set(
                ctx.conn(),
                &hydrus_store::trash::TrashSettings {
                    max_age_hours: None,
                    max_size_mb: None,
                },
            )
        })
        .unwrap();
    (dirs, store, files)
}
pub(super) fn wait(mut check: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !check() {
        assert!(
            Instant::now() < until,
            "owned real maintenance did not complete"
        );
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(10));
    }
}
pub(super) fn queue(store: &Store, id: HashId) {
    store
        .write_content(move |w| {
            w.inbox(&[id])?;
            w.delete_files(w.roles().local_file_storage, &[id], None)
        })
        .unwrap();
}
// leaf: audit-options-files-and-trash-allow-deferred-file-deletes-during-normal-time
#[test]
fn staged_controls_cancel_reopen_hidden_retired_and_peer_merge() {
    let fixture = hydrus_testkit::fixture_json("normal_time_maintenance.json");
    let (_dirs, store, _files) = owned();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    for case in fixture["controls"].as_array().unwrap() {
        store
            .write(|ctx| settings::set(ctx.conn(), &Preferences::default()))
            .unwrap();
        let (w, rows) = open(&ui, &bound);
        for (i, row) in rows.into_iter().enumerate() {
            let shown = w
                .get_rows()
                .row_data(usize::try_from(row).unwrap())
                .unwrap();
            assert_eq!(shown.kind, 1);
            assert!(shown.enabled);
            w.invoke_check_toggled(row, case["typed"][i].as_bool().unwrap());
        }
        if case["apply"] == true {
            w.invoke_apply();
        } else {
            w.invoke_cancel();
        }
        let saved = store.read(maintenance_gates::load).unwrap();
        assert_eq!(
            [saved.trash_normal, saved.deferred_normal],
            [
                case["saved"][0].as_bool().unwrap(),
                case["saved"][1].as_bool().unwrap()
            ]
        );
    }
    store
        .write(|ctx| settings::set(ctx.conn(), &Preferences::default()))
        .unwrap();
    let (w, rows) = open(&ui, &bound);
    w.hide().unwrap();
    w.invoke_check_toggled(rows[0], false);
    w.show().unwrap();
    w.invoke_apply();
    assert_eq!(
        store.read(maintenance_gates::load).unwrap(),
        Preferences::default()
    );
    let (w, rows) = open(&ui, &bound);
    w.invoke_check_toggled(rows[0], false);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &Preferences {
                    trash_normal: true,
                    deferred_normal: false,
                },
            )
        })
        .unwrap();
    w.invoke_apply();
    assert_eq!(
        store.read(maintenance_gates::load).unwrap(),
        Preferences {
            trash_normal: false,
            deferred_normal: false
        }
    );
    let (old, rows) = open(&ui, &bound);
    old.invoke_check_toggled(rows[1], true);
    let replacement = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    old.show().unwrap();
    old.invoke_apply();
    old.hide().unwrap();
    assert_eq!(
        store.read(maintenance_gates::load).unwrap(),
        Preferences {
            trash_normal: false,
            deferred_normal: false
        }
    );
    let (w, _) = open(&ui, &replacement);
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1000, 1000);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("normal_time_maintenance_options.png"),
        &pixels,
        1000,
        1000,
    )
    .unwrap();
    w.invoke_cancel();
}
// leaf: audit-options-files-and-trash-allow-deferred-file-deletes-during-normal-time
#[test]
fn unchecked_normal_flags_block_real_workers_but_current_live_idle_admits_both() {
    let (_dirs, store, files) = owned();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "maintenance fixture",
            None,
            files.iter().map(|(id, _)| *id).collect(),
        )),
    );
    ui.show().unwrap();
    queue(&store, files[0].0);
    let trash = files[1].0;
    store
        .write_content(move |w| {
            w.delete_files(w.roles().combined_local_media, &[trash], None)?;
            w.inbox(&[trash])
        })
        .unwrap();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &hydrus_store::trash::TrashSettings {
                    max_age_hours: None,
                    max_size_mb: Some(0),
                },
            )
        })
        .unwrap();
    let first = bound.maintenance.started_ms() + 30_000;
    bound.session_autosave.user_at(first);
    bound.maintenance.poll_at(first).unwrap();
    assert!(!bound.maintenance.running(Worker::Trash));
    assert!(!bound.maintenance.running(Worker::Deferred));
    assert!(files[0].1.exists());
    assert!(files[1].1.exists());
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
    let idle = bound
        .maintenance
        .deadline(Worker::Trash)
        .max(bound.maintenance.deadline(Worker::Deferred));
    assert!(bound.session_autosave.idle_at(idle));
    bound.maintenance.poll_at(idle).unwrap();
    wait(|| {
        bound.maintenance.poll_at(idle).unwrap();
        let stats = bound.maintenance.statistics();
        stats.trash_passes == 1 && stats.deferred_passes == 1
    });
    let stats = bound.maintenance.statistics();
    assert!(stats.trashed_files_removed >= 1);
    assert!(stats.physical_files_removed >= 1);
    assert!(!files[0].1.exists());
    // Trash and deferred jobs are separate. A trash enqueue accepted after the
    // current physical snapshot is consumed by its next independently gated pass.
    let next = bound.maintenance.deadline(Worker::Deferred);
    bound.maintenance.poll_at(next).unwrap();
    wait(|| {
        bound.maintenance.poll_at(next).unwrap();
        !files[1].1.exists()
    });
    assert!(files[2].1.exists());
}
// leaf: audit-options-files-and-trash-allow-deferred-file-deletes-during-normal-time
#[test]
fn normal_time_apply_reaches_waiting_consumer_and_terminal_owner_wakes_without_successor() {
    let (_dirs, store, files) = owned();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    queue(&store, files[0].0);
    queue(&store, files[1].0);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &hydrus_store::physical_delete::Preferences { wait_ms: 60_000 },
            )
        })
        .unwrap();
    let (w, rows) = open(&ui, &bound);
    w.invoke_check_toggled(rows[1], true);
    w.invoke_apply();
    let now = bound.maintenance.started_ms() + 30_000;
    bound.session_autosave.user_at(now);
    bound.maintenance.poll_at(now).unwrap();
    wait(|| !files[0].1.exists());
    assert!(files[1].1.exists());
    assert!(bound.maintenance.running(Worker::Deferred));
    let (w, rows) = open(&ui, &bound);
    w.invoke_check_toggled(rows[1], false);
    w.invoke_apply();
    assert!(
        bound.maintenance.running(Worker::Deferred),
        "a settings edit does not cancel its already admitted pass"
    );
    store
        .write(|ctx| {
            let mut gui: settings::GuiSettings = settings::get(ctx.conn())?;
            gui.confirm_exit = true;
            // Isolate confirmed owner retirement from shutdown maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(ctx.conn())?;
            shutdown.action = 0;
            hydrus_store::settings::set(ctx.conn(), &shutdown)?;
            settings::set(ctx.conn(), &gui)
        })
        .unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(false);
    assert!(bound.maintenance.running(Worker::Deferred));
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!bound.maintenance.running(Worker::Deferred));
    ui.show().unwrap();
    bound.maintenance.poll_at(now + 1_000_000).unwrap();
    assert!(files[1].1.exists());
    assert_eq!(
        bound.maintenance.statistics().deferred_passes,
        0,
        "late retired completion is not presented"
    );
    let fresh = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    // The old cancellation wakes outside the writer; exact fresh-owner work
    // can read/write the Store and finish the still queued successor pair.
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &hydrus_store::physical_delete::Preferences { wait_ms: 20 },
            )
        })
        .unwrap();
    let (w, rows) = open(&ui, &fresh);
    w.invoke_check_toggled(rows[1], true);
    w.invoke_apply();
    let due = fresh.maintenance.started_ms() + 30_000;
    fresh.session_autosave.user_at(due);
    fresh.maintenance.poll_at(due).unwrap();
    wait(|| {
        fresh.maintenance.poll_at(due).unwrap();
        !files[1].1.exists()
    });
    assert!(files[2].1.exists());
}

// leaf: audit-options-files-and-trash-allow-deferred-file-deletes-during-normal-time
#[test]
fn rebind_wakes_previous_pass_and_preserves_queued_pair_for_current_owner() {
    let (_dirs, store, files) = owned();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let old = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    queue(&store, files[0].0);
    queue(&store, files[1].0);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &Preferences {
                    trash_normal: false,
                    deferred_normal: true,
                },
            )?;
            settings::set(
                ctx.conn(),
                &hydrus_store::physical_delete::Preferences { wait_ms: 60_000 },
            )
        })
        .unwrap();
    let now = old.maintenance.started_ms() + 30_000;
    old.session_autosave.user_at(now);
    old.maintenance.poll_at(now).unwrap();
    wait(|| !files[0].1.exists());
    let current = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    assert!(!old.maintenance.running(Worker::Deferred));
    // The writer can save settings while the previous worker waits outside it.
    // Cancellation wakes that wait and prevents its next pair admission.
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &hydrus_store::physical_delete::Preferences { wait_ms: 20 },
            )
        })
        .unwrap();
    old.maintenance.poll_at(now + 1_000_000).unwrap();
    assert!(files[1].1.exists());
    assert_eq!(old.maintenance.statistics().deferred_passes, 0);
    let due = current.maintenance.started_ms() + 30_000;
    current.session_autosave.user_at(due);
    current.maintenance.poll_at(due).unwrap();
    wait(|| {
        current.maintenance.poll_at(due).unwrap();
        current.maintenance.statistics().deferred_passes == 1
    });
    assert!(!files[1].1.exists());
    assert!(files[2].1.exists());
    assert!(files[3].1.exists());
}

// leaf: audit-options-files-and-trash-allow-deferred-file-deletes-during-normal-time
#[test]
fn dropped_binding_retires_retained_control_and_wakes_its_real_held_wait() {
    let (_dirs, store, files) = owned();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    queue(&store, files[0].0);
    queue(&store, files[1].0);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &Preferences {
                    trash_normal: false,
                    deferred_normal: true,
                },
            )?;
            settings::set(
                ctx.conn(),
                &hydrus_store::physical_delete::Preferences { wait_ms: 60_000 },
            )
        })
        .unwrap();
    let retained = bound.maintenance.clone();
    let now = retained.started_ms() + 30_000;
    bound.session_autosave.user_at(now);
    retained.poll_at(now).unwrap();
    wait(|| !files[0].1.exists());
    assert!(retained.running(Worker::Deferred));
    let binding_clone = bound.clone();
    drop(bound);
    assert!(
        retained.running(Worker::Deferred),
        "another Bound clone still owns the admitted pass"
    );
    assert!(files[1].1.exists());
    drop(binding_clone);
    assert!(ui.window().is_visible());
    assert!(!retained.running(Worker::Deferred));
    retained.poll_at(now + 1_000_000).unwrap();
    assert!(files[1].1.exists());
    assert_eq!(retained.statistics().deferred_passes, 0);
    // Main callbacks still retain the old Control, but cannot keep its work live.
    // A new binding alone can admit the still queued next pair.
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &hydrus_store::physical_delete::Preferences { wait_ms: 20 },
            )
        })
        .unwrap();
    let next = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let due = next.maintenance.started_ms() + 30_000;
    next.session_autosave.user_at(due);
    next.maintenance.poll_at(due).unwrap();
    wait(|| {
        next.maintenance.poll_at(due).unwrap();
        next.maintenance.statistics().deferred_passes == 1
    });
    assert!(!files[1].1.exists());
    assert!(files[2].1.exists());
    assert!(files[3].1.exists());
    // Destroy the emitting component, releasing Slint's shown-window retention.
    // The owned timer/poll must notice despite retained Bound and public Control.
    let weak_main = ui.as_weak();
    ui.hide().unwrap();
    drop(ui);
    assert!(weak_main.upgrade().is_none());
    next.maintenance.poll_at(due + 1_000_000).unwrap();
    assert!(!next.maintenance.running(Worker::Trash));
    assert!(!next.maintenance.running(Worker::Deferred));
}

// leaf: audit-options-files-and-trash-when-physically-deleting-files-or-folders-send-them-to-the-os-s-recycle-bin
#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn the_recycle_bin_option_decides_whether_physical_deletes_go_to_the_os_bin() {
    const RECYCLE: &str =
        "When physically deleting files or folders, send them to the OS's recycle bin: ";
    /// Empties the OS bin of what the test sent there, however it ends.
    struct Purge(Vec<std::path::PathBuf>);
    impl Drop for Purge {
        fn drop(&mut self) {
            for path in &self.0 {
                hydrus_store::paths::purge_from_recycle_bin(path);
            }
        }
    }
    let (dirs, store, files) = owned();
    // Where the OS has no usable bin for this disk, a recycled file is just
    // deleted (as `delete_or_recycle` falls back to), and the test cannot tell.
    let probe = dirs[1].path().join("recycle-probe");
    std::fs::write(&probe, b"probe").unwrap();
    let _purge = Purge(
        std::iter::once(probe.clone())
            .chain(files.iter().map(|(_, path)| path.clone()))
            .collect(),
    );
    hydrus_store::paths::delete_or_recycle(&probe, true).unwrap();
    if !hydrus_store::paths::recycle_bin_holds(&probe) {
        eprintln!("the OS has no usable recycle bin here; skipped");
        return;
    }
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    let recycle = || {
        store
            .read(settings::get::<settings::FolderSettings>)
            .unwrap()
            .delete_to_recycle_bin
    };
    let set_recycle = |on: bool| {
        let (w, rows) = open(&ui, &bound);
        let at = i32::try_from(
            w.get_rows()
                .iter()
                .position(|row| row.label == RECYCLE)
                .unwrap(),
        )
        .unwrap();
        let shown = w.get_rows().row_data(at as usize).unwrap();
        assert_eq!(shown.kind, 1);
        assert_eq!(shown.checked, recycle(), "shows what is saved");
        w.invoke_check_toggled(at, on);
        // (and the deferred deletes may run in normal time)
        w.invoke_check_toggled(rows[1], true);
        w.invoke_apply();
        assert_eq!(recycle(), on);
    };
    let delete = |file: usize, now: i64| {
        queue(&store, files[file].0);
        let now = now.max(bound.maintenance.deadline(Worker::Deferred));
        bound.session_autosave.user_at(now);
        bound.maintenance.poll_at(now).unwrap();
        wait(|| {
            bound.maintenance.poll_at(now).unwrap();
            !files[file].1.exists()
        });
    };
    assert!(!recycle(), "(this store starts with it off)");
    let now = bound.maintenance.started_ms() + 30_000;
    // each file's thumbnail, which goes with it
    let thumbnail = |file: usize| {
        let stem = files[file].1.file_stem().unwrap().to_str().unwrap();
        let hash = Sha256::from_slice(&hex::decode(stem).unwrap()).unwrap();
        store.snapshot().storage.thumbnail_path(&hash).unwrap()
    };
    let thumbnails = [thumbnail(0), thumbnail(1)];
    for path in &thumbnails {
        // (this copied storage has none; any thumbnail there goes with it)
        if !path.is_file() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"thumbnail").unwrap();
        }
    }

    // The reference's own deferred physical delete, off then on
    // (oracle/fixtures/file_paths_options.json, record_file_paths_options.py):
    // off, file and thumbnail are gone for good; on, the file is in the OS's
    // recycle bin (its .trashinfo naming where it was) and the thumbnail is
    // still deleted for good.
    let recorded = hydrus_testkit::fixture_json("file_paths_options.json");
    let recorded = recorded["recycle"].as_array().unwrap();
    for (file, case) in recorded.iter().enumerate() {
        let on = case["recycle"].as_bool().unwrap();
        set_recycle(on);
        delete(file, now + 120_000 * file as i64);
        assert_eq!(files[file].1.exists(), case["file_still_there"] == true);
        assert_eq!(
            hydrus_store::paths::recycle_bin_holds(&files[file].1),
            case["file_in_trash"]["file"] == true,
            "{on}: {} in the bin",
            files[file].1.display()
        );
        if on {
            assert_eq!(case["file_in_trash"]["info_path_is_original"], true);
        }
        wait(|| !thumbnails[file].exists());
        assert_eq!(case["thumbnail_still_there"], false);
        assert_eq!(
            hydrus_store::paths::recycle_bin_holds(&thumbnails[file]),
            case["thumbnail_in_trash"]["file"] == true,
            "{on}: the thumbnail is never recycled"
        );
    }
}
