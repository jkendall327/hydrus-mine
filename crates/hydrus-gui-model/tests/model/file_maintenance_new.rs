//! Database > file maintenance > add new work: its words, its file
//! choices and the jobs it queues (`ReviewFileMaintenance`).
use hydrus_gui_model::file_maintenance_new::{
    JOBS, Pick, description, find, found, job_labels, schedule, schedule_question, selected,
};
use hydrus_store::Store;
use hydrus_store::file_maintenance::{self, JobType};

#[test]
fn the_tab_says_what_the_reference_does() {
    assert_eq!(found(1_234), "1,234 files found");
    assert_eq!(selected(3), "3 files selected");
    assert_eq!(job_labels()[0], JOBS[0].description());
    assert_eq!(job_labels().len(), 27);
    assert!(description(JobType::Blurhash).ends_with(
        "\n\nThis job has weight 15, where a normalised unit of file work has value 100."
    ));
    assert_eq!(schedule_question(JobType::Blurhash, 1000), None);
    assert_eq!(
        schedule_question(JobType::Blurhash, 1001),
        Some(format!(
            "Are you sure you want to schedule \"{}\" on 1,001 files?",
            JobType::Blurhash.description()
        ))
    );
}

#[test]
fn chosen_files_get_the_job_queued() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let media = find(&store, Pick::AllMedia, &[]).unwrap();
    assert!(!media.is_empty());
    let searched = find(&store, Pick::Search, &[]).unwrap();
    assert!(searched.len() <= media.len());
    assert!(find(&store, Pick::AllUpdates, &[]).unwrap().is_empty());
    schedule(&store, media.clone(), JobType::Blurhash).unwrap();
    let counts = store
        .read(|conn| file_maintenance::job_counts(conn, i64::MAX / 2))
        .unwrap();
    assert_eq!(counts[&JobType::Blurhash].0, media.len() as u64);
}
