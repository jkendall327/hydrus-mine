//! Database > backup: its menu as the updater shows it, and what its
//! entries say and ask (`_SetupBackupPath`, `_BackupDatabase`,
//! `RestoreDatabase`).
use hydrus_gui_model::database_backup::{
    Action, Chosen, chosen_question, intro, restore_question, update_label, update_question,
};
use hydrus_gui_model::main_menu::{Command, Entry, Facts, menubar};

fn backup_menu(facts: &Facts) -> Vec<String> {
    let menus = menubar(facts);
    let Some(Entry::Menu { entries, .. }) = menus.iter().find_map(|m| match m {
        Entry::Menu { label, entries, .. } if label == "&database" => {
            entries.iter().find(|e| e.label() == "backup")
        }
        _ => None,
    }) else {
        panic!("a backup menu")
    };
    entries.iter().map(|e| e.label().to_owned()).collect()
}

#[test]
fn the_menu_follows_the_backup_location_and_storage() {
    let mut facts = Facts {
        locations_default: true,
        now: 10_000,
        ..Facts::default()
    };
    assert_eq!(
        backup_menu(&facts),
        [
            "set up a database backup location\u{2026}",
            "",
            "restore from a database backup\u{2026}"
        ]
    );
    facts.backup.path = Some("/backups".into());
    facts.backup.last_backup = Some(10_000 - 60);
    assert_eq!(
        backup_menu(&facts)[..2],
        [
            "update database backup (did one recently)\u{2026}",
            "change database backup location\u{2026}"
        ]
    );
    facts.locations_default = false;
    assert_eq!(
        backup_menu(&facts),
        ["database is stored in multiple locations"]
    );
    assert_eq!(update_label(None, 0), "update database backup");
    assert_eq!(
        update_label(Some(0), 3 * 86_400),
        "update database backup (last 3 days ago)"
    );
    let _ = Command::Backup(Action::SetUp);
}

#[test]
fn questions_name_the_paths_as_the_reference_does() {
    assert!(intro(None).starts_with("Everything in your client is stored in the 'database'"));
    assert!(intro(Some("/b")).starts_with("Your current backup location is \"/b\"."));
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(Chosen::of(dir.path(), "hydrus.db"), Chosen::Empty);
    std::fs::write(dir.path().join("hydrus.db"), b"").unwrap();
    assert_eq!(Chosen::of(dir.path(), "hydrus.db"), Chosen::HasDatabase);
    assert_eq!(
        Chosen::of(&dir.path().join("none"), "hydrus.db"),
        Chosen::Missing
    );
    assert!(chosen_question("/b", Chosen::Missing).contains(
        "The path does not exist yet--it will be created when you make your first backup."
    ));
    assert!(update_question("/b", true).starts_with("Update the existing backup at \"/b\"?"));
    assert!(update_question("/b", false).starts_with("Create a new backup at \"/b\"?"));
    assert!(
        restore_question("/b").contains("Everything in your current database will be deleted!")
    );
}
