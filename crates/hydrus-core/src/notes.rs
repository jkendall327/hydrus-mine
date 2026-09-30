//! Merging new notes into a file's existing notes, as the reference's note
//! import options do (used by `/add_notes/set_notes` with `merge_cleverly`).

use std::collections::BTreeMap;

/// What to do when a note name is already taken by different text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteConflict {
    /// Overwrite the existing note.
    Replace = 0,
    /// Keep the existing note; drop the new one.
    Ignore = 1,
    /// Append the new text to the existing note.
    Append = 2,
    /// Add the new note under a free name, `name (1)`, `name (2)`, ...
    Rename = 3,
}

impl NoteConflict {
    pub const fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => Self::Replace,
            1 => Self::Ignore,
            2 => Self::Append,
            3 => Self::Rename,
            _ => return None,
        })
    }
}

/// How to merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NoteMerge {
    /// If a new note's text contains an existing note's, replace the latter.
    pub extend_existing: bool,
    pub conflict: NoteConflict,
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

/// `longer` strictly extends `shorter` (is longer and contains it).
fn extends(longer: &str, shorter: &str) -> bool {
    chars(longer) > chars(shorter) && longer.contains(shorter)
}

/// `name`, or `name (1)`, `name (2)`, ... whichever is first not taken.
fn free_name(name: &str, taken: &BTreeMap<String, String>) -> String {
    let mut candidate = name.to_owned();
    let mut i = 1;
    while taken.contains_key(&candidate) {
        candidate = format!("{name} ({i})");
        i += 1;
    }
    candidate
}

impl NoteMerge {
    /// The notes to set so that `incoming` is merged into `existing`.
    pub fn merge(
        &self,
        existing: &BTreeMap<String, String>,
        incoming: &[(String, String)],
    ) -> BTreeMap<String, String> {
        let mut existing = existing.clone();
        let mut updates = BTreeMap::new();
        let mut incoming = incoming.to_vec();
        incoming.sort();
        for (name, note) in incoming {
            let mut name = name;
            let mut note = note;
            let current = existing.get(&name).cloned();
            let same = current.as_deref() == Some(note.as_str());
            let extends_current = current.as_deref().is_some_and(|c| extends(&note, c));

            // the same text under any name means there's nothing to add
            let mut text_elsewhere = existing.values().any(|n| *n == note);
            let mut extended_name = None;
            if !text_elsewhere {
                for (other_name, other_note) in &existing {
                    if *other_name == name
                        || !(other_name.starts_with(name.as_str()) || *other_note == note)
                    {
                        continue;
                    }
                    if *other_note == note {
                        text_elsewhere = true;
                        break;
                    }
                    if extends(&note, other_note) {
                        extended_name = Some(other_name.clone());
                        break;
                    }
                }
            }

            let mut apply = !same && !text_elsewhere;
            if apply && let Some(current) = current {
                if extends_current && self.extend_existing {
                    // extend in place
                } else if let Some(other) = extended_name.filter(|_| self.extend_existing) {
                    name = other;
                } else {
                    match self.conflict {
                        NoteConflict::Ignore => apply = false,
                        NoteConflict::Rename => name = free_name(&name, &existing),
                        NoteConflict::Append => {
                            if extends(&current, &note) {
                                apply = false;
                            } else {
                                note = format!("{current}\n\n{note}");
                            }
                        }
                        NoteConflict::Replace => {}
                    }
                }
            }
            if apply {
                updates.insert(name.clone(), note.clone());
                existing.insert(name, note);
            }
        }
        updates
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::fixture;

    #[test]
    fn merges_like_the_reference() {
        let data = fixture("note_merges.json");
        let cases = data["cases"].as_array().unwrap();
        for case in cases {
            let existing: BTreeMap<String, String> =
                serde_json::from_value(case["existing"].clone()).unwrap();
            let incoming: Vec<(String, String)> =
                serde_json::from_value(case["incoming"].clone()).unwrap();
            let merge = NoteMerge {
                extend_existing: case["extend_existing_note_if_possible"].as_bool().unwrap(),
                conflict: NoteConflict::from_code(case["conflict_resolution"].as_i64().unwrap())
                    .unwrap(),
            };
            let expected: BTreeMap<String, String> =
                serde_json::from_value(case["result"].clone()).unwrap();
            assert_eq!(merge.merge(&existing, &incoming), expected, "{case}");
        }
        // every conflict code the reference knows is one we know
        for code in data["conflict_resolutions"].as_object().unwrap().keys() {
            assert!(NoteConflict::from_code(code.parse().unwrap()).is_some());
        }
    }
}
