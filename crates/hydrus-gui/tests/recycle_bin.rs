//! The recycle-bin option sends the reference's deferred physical deletes to
//! the OS bin or not. Alone in its own test binary, with the OS bin pointed at
//! a temporary directory, so nothing reaches (or is left in) the real one.
#![cfg(all(unix, not(target_os = "macos")))]
use hydrus_core::{HashId, Mime, Sha256};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{
    Store,
    content::DomainRoles,
    maintenance_gates::{Preferences, Worker},
    settings,
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

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}
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
fn owned() -> (
    [tempfile::TempDir; 2],
    Arc<Store>,
    Vec<(HashId, std::path::PathBuf)>,
) {
    let (dirs, store) = store();
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
fn wait(mut check: impl FnMut() -> bool) {
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
fn queue(store: &Store, id: HashId) {
    store
        .write_content(move |w| {
            w.inbox(&[id])?;
            w.delete_files(w.roles().local_file_storage, &[id], None)
        })
        .unwrap();
}
// leaf: audit-options-files-and-trash-when-physically-deleting-files-or-folders-send-them-to-the-os-s-recycle-bin
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
    // the OS bin lives under XDG_DATA_HOME; this binary has this one test
    let bin = tempfile::tempdir().unwrap();
    // SAFETY: this binary has one test, so no other thread reads the environment.
    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var("XDG_DATA_HOME", bin.path());
    };
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
