//! The thumbnail menu's manage > maintenance entries against
//! `oracle/dump_regen_jobs.py`, and their questions.
use hydrus_gui_model::thumbnail_maintenance::{
    Asking, HUMAN_ORDER, clear_viewing_stats_question, description, regenerate_question,
};
use hydrus_store::file_maintenance::JobType;

#[test]
fn jobs_are_in_the_reference_s_order_with_its_words() {
    let fixture = hydrus_testkit::fixture_json("regen_jobs.json");
    let recorded = fixture.as_array().unwrap();
    assert_eq!(recorded.len(), HUMAN_ORDER.len());
    for (job, recorded) in HUMAN_ORDER.iter().zip(recorded) {
        assert_eq!(job.code(), recorded["code"].as_i64().unwrap());
        assert_eq!(job.description(), recorded["label"]);
        assert_eq!(description(*job), recorded["description"]);
    }
}

#[test]
fn questions_follow_the_selection_size() {
    assert_eq!(regenerate_question(JobType::ForceThumbnail, 0), None);
    assert_eq!(
        regenerate_question(JobType::ForceThumbnail, 3),
        Some(Asking::YesNo(
            "This will force-regenerate the 3 selected files' thumbnails.".into()
        ))
    );
    let Some(Asking::NowOrLater(message)) = regenerate_question(JobType::Blurhash, 1234) else {
        panic!("over fifty files offer now or later");
    };
    assert!(message.starts_with(description(JobType::Blurhash)));
    assert!(message.ends_with("You have selected 1,234 files, so this job may take some time. You can run it all now or schedule it to the overall file maintenance queue for later spread-out processing."));
    assert_eq!(
        clear_viewing_stats_question(1).unwrap(),
        "Clear the file viewing count/duration and 'last viewed time' for this file?"
    );
    assert_eq!(
        clear_viewing_stats_question(2000).unwrap(),
        "Clear the file viewing count/duration and 'last viewed time' for these 2,000 files?"
    );
}
