//! Importers' status texts against the reference's
//! (`oracle/record_import_status.py`): a file log's in full and short (with
//! the short summary's options each way) and its value and range, and a
//! search log's, for 401 sets of counts by status; and a download's line
//! under them (`NetworkJobControl`) for 337 jobs.

use serde_json::Value as Json;

use hydrus_store::queues::{
    SeedStatus, StatusCounts, file_log_short_status, file_log_status, file_log_value_range,
    search_log_status,
};

fn counts(recorded: &Json) -> StatusCounts {
    recorded
        .as_object()
        .unwrap()
        .iter()
        .map(|(status, n)| {
            (
                SeedStatus::from_code(status.parse().unwrap()).unwrap(),
                usize::try_from(n.as_u64().unwrap()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn file_and_search_logs_say_what_the_reference_says() {
    let recorded = hydrus_testkit::fixture_json("import_status.json");
    let file_logs = recorded["file_logs"].as_array().unwrap();
    assert!(file_logs.len() > 400);
    for case in file_logs {
        let counts = counts(&case["counts"]);
        assert_eq!(file_log_status(&counts), case["full"], "{case}");
        for (key, show_new, show_deleted) in [
            ("00", false, false),
            ("01", false, true),
            ("10", true, false),
            ("11", true, true),
        ] {
            assert_eq!(
                file_log_short_status(&counts, show_new, show_deleted),
                case["simple"][key],
                "{key}: {case}"
            );
        }
        let (value, range) = file_log_value_range(&counts);
        assert_eq!(
            serde_json::json!([value, range]),
            case["value_range"],
            "{case}"
        );
    }
    for case in recorded["search_logs"].as_array().unwrap() {
        let (status, (value, range)) = search_log_status(&counts(&case["counts"]));
        assert_eq!(status, case["status"], "{case}");
        assert_eq!(
            serde_json::json!([value, range]),
            case["value_range"],
            "{case}"
        );
    }
}

#[test]
fn downloads_lines_say_what_the_references_network_job_control_says() {
    use hydrus_store::live::{JobLine, network_job_line};
    let recorded = hydrus_testkit::fixture_json("import_status.json");
    let jobs = recorded["network_jobs"].as_array().unwrap();
    assert!(jobs.len() > 300);
    for case in jobs {
        let job = &case["job"];
        let line = if job.is_null() || job["no_engine_yet"].as_bool().unwrap() {
            JobLine::default()
        } else {
            network_job_line(
                job["status_text"].as_str().unwrap(),
                job["speed"].as_u64().unwrap(),
                job["bytes_read"].as_u64(),
                job["bytes_to_read"].as_u64(),
                job["has_error"].as_bool().unwrap(),
                job["is_done"].as_bool().unwrap(),
            )
        };
        assert_eq!(line.left, case["left"], "{case}");
        assert_eq!(line.right, case["right"], "{case}");
        let gauge = &case["gauge"];
        assert_eq!(
            line.gauge,
            (gauge[0].as_u64().unwrap(), gauge[1].as_u64().unwrap()),
            "{case}"
        );
        assert_eq!(line.can_cancel, case["can_cancel"], "{case}");
    }
}
