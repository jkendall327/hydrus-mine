//! Exact global archive-time maintenance questions and population choices.
use hydrus_core::numbers::human_int;
use hydrus_store::archive_repair::{Plan, Population};

pub const SCAN_QUESTION: &str = "There are a couple of ways your client may be missing archive times for your files. This will scan for missing times and then present you with the results and a choice on what to do.\n\nThe scan may take a while. It will have a popup showing its work, but it may lock up your client for a bit while it works.";
pub const LEGACY_EXPLANATION: &str = "These are files that were archived before hydrus started tracking archive time (2022-02). If you select to fill these in, hydrus will insert a synthetic time that is import time + 20% of the time to 2022-02 or any file deletion time.";
pub const IMPORT_EXPLANATION: &str = "These are most likely files that were imported with \"automatically archive\", which for some period until 2024-12 were not recording archive times due to a bug. It may include a few other instances of missing archived files (e.g. you manually deleted one). If you select to fill these in, hydrus will insert a synthetic time that is the same as the import time.";
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub label: &'static str,
    pub populations: Vec<Population>,
}
pub fn choices(plan: &Plan) -> Vec<Choice> {
    let legacy = plan.count(Population::Legacy) > 0;
    let import = plan.count(Population::Import) > 0;
    let mut choices = Vec::new();
    if legacy {
        choices.push(Choice {
            label: "do legacy times",
            populations: vec![Population::Legacy],
        });
    }
    if import {
        choices.push(Choice {
            label: "do import times",
            populations: vec![Population::Import],
        });
    }
    if legacy && import {
        choices.push(Choice {
            label: "do both",
            populations: vec![Population::Legacy, Population::Import],
        });
    }
    choices
}
pub fn question(plan: &Plan) -> String {
    let mut text = "It looks like there are some missing archive times. You have:".to_string();
    for (population, label, explanation) in [
        (Population::Legacy, "Legacy", LEGACY_EXPLANATION),
        (Population::Import, "Import", IMPORT_EXPLANATION),
    ] {
        let count = plan.count(population);
        if count > 0 {
            text.push_str(&format!(
                "\n\n--{} Missing {label} Times--\n\n{explanation}",
                human_int(count as u64)
            ));
        }
    }
    text
}
