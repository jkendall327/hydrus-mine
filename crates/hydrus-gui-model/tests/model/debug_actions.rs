//! Help > debug's actions: the popups "make some popups" shows, the modal
//! popup's countdown and "show env"'s text, as the reference's
//! `ClientGUI` debug methods and `HydrusEnvironment.DumpEnv` make them.
use hydrus_core::Sha256;
use hydrus_gui_model::debug_actions::{
    PROFILE_MESSAGE, delayed_popup_text, env_text, modal_text, some_popups,
};

#[test]
fn some_popups_are_the_reference_s_less_its_callbacks() {
    let mut n = 0u8;
    let popups = some_popups(5.0, || {
        n += 1;
        Sha256([n; 32])
    });
    let texts: Vec<&str> = popups
        .iter()
        .map(|p| p.status_text_1.as_deref().unwrap_or_default())
        .collect();
    assert_eq!(texts[0], "This is a test popup message -- 1");
    assert_eq!(texts[5], "This is a test popup message -- 6");
    assert!(texts[6].starts_with("This is a very long message:  \n\n++++What"));
    assert!(texts[6].ends_with("++++The Emperor Protects++++"));
    // the merge test's two popups share a file, so they can merge
    let merging: Vec<_> = popups
        .iter()
        .filter(|p| p.attached_files_mergable)
        .collect();
    assert_eq!(merging.len(), 2);
    let (a, _) = merging[0].files.clone().unwrap();
    let (b, label) = merging[1].files.clone().unwrap();
    assert!(b.contains(&a[0]));
    assert_eq!(label.as_deref(), Some("cool pics"));
    let job = popups
        .iter()
        .find(|p| p.status_title.as_deref() == Some("test job"))
        .unwrap();
    assert!(job.pausable && job.cancellable && !job.done);
    assert_eq!(job.popup_gauge_1, Some((4, 8)));
    let error = popups.last().unwrap();
    assert!(error.had_error);
    assert_eq!(error.status_title.as_deref(), Some("DataMissing"));
    assert_eq!(
        delayed_popup_text(2),
        "This is a delayed popup message -- 2"
    );
    assert!(PROFILE_MESSAGE.ends_with("under 'reducing lag'."));
}

#[test]
fn modal_countdown_and_env_dump_read_as_the_reference_s() {
    assert_eq!(modal_text(0), "Will auto-dismiss in 10 seconds.");
    assert_eq!(modal_text(9), "Will auto-dismiss in 1 second.");
    let text = env_text(
        [
            ("PATH".to_owned(), "/bin:/usr/bin".to_owned()),
            ("HOME".to_owned(), "/home/x".to_owned()),
            ("XDG_DATA_DIRS".to_owned(), "/a".to_owned()),
        ],
        ':',
    );
    assert_eq!(
        text,
        "Full environment:\nHOME: /home/x\nPATH:\n    /bin\n    /usr/bin\nXDG_DATA_DIRS: /a"
    );
}

// leaf: audit-options-help-debug-action-scan-file-storage-folders
#[test]
fn scan_file_storage_folders_finds_the_prefix_folders_or_gives_up_quickly() {
    use hydrus_gui_model::debug_actions::{presumptive_subfolders, scan_text};
    let dir = tempfile::tempdir().unwrap();
    for prefix in ["fa0", "fa1", "t00"] {
        std::fs::create_dir_all(dir.path().join(prefix)).unwrap();
    }
    std::fs::write(dir.path().join("client.db"), b"").unwrap();
    // granularity 2: the directories directly beneath
    let found = presumptive_subfolders(dir.path(), 2).unwrap();
    assert_eq!(found.len(), 3);
    // granularity 3: the directories one level down
    std::fs::create_dir_all(dir.path().join("fa0").join("fa00")).unwrap();
    std::fs::create_dir_all(dir.path().join("fa0").join("fa01")).unwrap();
    assert_eq!(presumptive_subfolders(dir.path(), 3).unwrap().len(), 2);
    // too many files at the top level
    for i in 0..17 {
        std::fs::write(dir.path().join(format!("file {i}")), b"").unwrap();
    }
    assert_eq!(
        presumptive_subfolders(dir.path(), 2).unwrap_err(),
        "Too many files!"
    );
    // and the text it says
    assert_eq!(
        scan_text(&Err("Too many files!".into()), 0.5),
        "It cancelled: Too many files!"
    );
    assert_eq!(
        scan_text(&Ok(1234), 0.5),
        "It found 1,234 subfolders and took 500 milliseconds"
    );
}
