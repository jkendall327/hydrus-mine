//! Database > db maintenance > review vacuum data: its rows and question
//! (`ReviewVacuumData`; the estimates are the reference's
//! `GetApproxVacuumIntoDuration` and `TimeDeltaToPrettyTimeDelta` outputs).
use hydrus_gui_model::vacuum_review::{estimate, question, row};
use hydrus_store::vacuum::VacuumData;

fn data(last: Option<i64>) -> VacuumData {
    VacuumData {
        name: "main".into(),
        path: std::path::PathBuf::from("/db/hydrus.db"),
        page_size: 4096,
        page_count: 25_600,
        freelist_count: 6_400,
        last_vacuumed_ms: last,
    }
}

#[test]
fn estimates_read_as_the_reference_s() {
    assert_eq!(estimate(0), "0 seconds to 0 seconds");
    assert_eq!(estimate(4_096_000), "0 seconds to 0 seconds");
    assert_eq!(estimate(104_857_600), "600 milliseconds to 12 seconds");
    assert_eq!(
        estimate(5_368_709_120),
        "30.7 seconds to 10 minutes 14 seconds"
    );
    assert_eq!(
        question(104_857_600),
        "Do vacuum now? Estimated time to vacuum is 600 milliseconds to 12 seconds."
    );
}

#[test]
fn a_row_says_size_free_space_and_readiness() {
    let now = 1_000_000;
    let [name, size, free, last, can, time] = row(&data(None), None, now);
    assert_eq!(name, "main");
    assert_eq!(size, "100 MB");
    assert_eq!(free, "25 MB (25%)");
    assert_eq!(last, "never done");
    assert_eq!(can, "yes!");
    assert_eq!(time, "600 milliseconds to 12 seconds");
    let [.., last, can, _] = row(&data(Some((now - 7200) * 1000)), Some(1_000), now);
    assert_eq!(last, "2 hours ago");
    assert!(can.starts_with("I believe you need about "));
}
