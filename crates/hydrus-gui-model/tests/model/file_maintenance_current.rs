use hydrus_gui_model::file_maintenance_current::Queue;
use hydrus_store::file_maintenance::JobType;
use std::collections::BTreeMap;

#[test]
fn recorded_counts_selection_due_gate_and_numeric_sort_use_typed_identity() {
    let fixture = hydrus_testkit::fixture_json("file_maintenance_current.json");
    let mut queue = Queue::new();
    for event in fixture["events"].as_array().unwrap() {
        let counts: BTreeMap<_, _> = event["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let code = row["code"].as_i64().unwrap();
                let due = row["sort"][1].as_u64().unwrap();
                let text = row["display"][1].as_str().unwrap();
                let future = text.split('(').nth(1).map_or(0, |text| {
                    text.split_whitespace()
                        .next()
                        .unwrap()
                        .replace(',', "")
                        .parse::<u64>()
                        .unwrap()
                });
                (JobType::from_code(code).unwrap(), (due, future))
            })
            .collect();
        queue.replace(counts);
        // SelectDatas in the recording adds to selection unless told to deselect.
        for code in event["selected"].as_array().unwrap() {
            let job = JobType::from_code(code.as_i64().unwrap()).unwrap();
            let rows = queue.rows();
            let index = rows.iter().position(|row| row.job == job).unwrap();
            if !rows[index].selected {
                queue.click(index, true, false);
            }
        }
        assert_eq!(
            queue.can_work(true),
            event["do_work"].as_bool().unwrap(),
            "{}",
            event["name"]
        );
        assert_eq!(
            queue.can_work(false),
            event["do_all"].as_bool().unwrap(),
            "{}",
            event["name"]
        );
        for row in queue.rows() {
            let recorded = event["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|value| value["code"] == row.job.code())
                .unwrap();
            assert_eq!(row.name, recorded["display"][0]);
            assert_eq!(row.count_text(), recorded["display"][1]);
        }
    }
    queue.replace(
        [
            (JobType::HasExif, (1000, 9)),
            (JobType::HasIccProfile, (2, 0)),
        ]
        .into(),
    );
    queue.numeric = true;
    queue.click(0, false, false);
    queue.ascending = false;
    assert_eq!(queue.rows()[0].due, 1000);
    assert_eq!(queue.selected(), [JobType::HasIccProfile]);
}
