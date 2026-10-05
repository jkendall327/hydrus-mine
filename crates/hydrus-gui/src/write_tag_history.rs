//! Stateless history operations; each native write-tag input owns its own stacks.
use crate::{TagTextEdit, TagTextHistoryState, TagTextState};
use slint::{Model as _, ModelRc, VecModel};

fn model<T: Clone + 'static>(rows: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(rows))
}

pub(crate) fn record(
    history: TagTextHistoryState,
    before: TagTextState,
    after: TagTextState,
    kind: i32,
    group: i32,
) -> TagTextHistoryState {
    let mut undos = if history.state.text == before.text {
        history.undos.iter().collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if before.text == after.text {
        return TagTextHistoryState {
            state: after,
            ..history
        };
    }
    // Qt separates a paste from subsequent typing, but combines contiguous
    // ordinary insertions/deletions into one typing action.
    let mut merged = false;
    if kind != 0
        && history.redos.row_count() == 0
        && before.anchor == before.cursor
        && let Some(last) = undos.last_mut()
        && last.kind == kind
        && last.group == group
        && last.after.text == before.text
        && last.after.cursor == before.cursor
    {
        last.after.clone_from(&after);
        merged = true;
    }
    if !merged {
        undos.push(TagTextEdit {
            before,
            after: after.clone(),
            kind,
            group,
        });
    }
    TagTextHistoryState {
        undos: model(undos),
        redos: ModelRc::default(),
        state: after,
    }
}

pub(crate) fn undo(history: TagTextHistoryState) -> TagTextHistoryState {
    let mut undos = history.undos.iter().collect::<Vec<_>>();
    let Some(edit) = undos.pop() else {
        return history;
    };
    let state = edit.before.clone();
    let mut redos = history.redos.iter().collect::<Vec<_>>();
    redos.push(edit);
    TagTextHistoryState {
        undos: model(undos),
        redos: model(redos),
        state,
    }
}

pub(crate) fn redo(history: TagTextHistoryState) -> TagTextHistoryState {
    let mut redos = history.redos.iter().collect::<Vec<_>>();
    let Some(edit) = redos.pop() else {
        return history;
    };
    let state = edit.after.clone();
    let mut undos = history.undos.iter().collect::<Vec<_>>();
    undos.push(edit);
    TagTextHistoryState {
        undos: model(undos),
        redos: model(redos),
        state,
    }
}

#[cfg(test)]
mod tests {
    use super::{record, undo};
    use crate::{TagTextHistoryState, TagTextState};
    use slint::Model as _;

    fn state(text: &str, cursor: i32) -> TagTextState {
        TagTextState {
            text: text.into(),
            anchor: cursor,
            cursor,
        }
    }

    fn recorded_final_text(name: &str) -> String {
        let fixture = hydrus_testkit::fixture_json("write_tag_selection.json");
        fixture["normal_paste_history"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap()["steps"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["text"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn caret_and_undo_boundaries_keep_new_typing_separate() {
        let first = record(
            TagTextHistoryState::default(),
            state("abc", 3),
            state("abcx", 4),
            1,
            0,
        );
        let adjacent = record(first.clone(), state("abcx", 4), state("abcxy", 5), 1, 0);
        assert_eq!(
            undo(adjacent).state.text,
            recorded_final_text("adjacent_typing")
        );
        let moved = record(first.clone(), state("abcx", 4), state("abcxy", 5), 1, 2);
        assert_eq!(
            undo(moved).state.text,
            recorded_final_text("cursor_movement_splits_typing")
        );
        let pasted = record(first, state("abcx", 4), state("abcxpaste", 9), 0, 0);
        let undone = undo(pasted);
        assert_eq!(undone.redos.row_count(), 1);
        // The pure helper rejects merging even if an input did not advance its
        // group: a nonempty redo stack proves an intervening Undo boundary.
        let fresh = record(undone, state("abcx", 4), state("abcxy", 5), 1, 0);
        assert_eq!(fresh.redos.row_count(), 0);
        assert_eq!(
            undo(fresh).state.text,
            recorded_final_text("typing_after_undo_is_new_command")
        );
    }

    #[test]
    fn delete_direction_is_a_command_boundary_but_repeated_delete_coalesces() {
        let forward = record(
            TagTextHistoryState::default(),
            state("abcdef", 3),
            state("abcef", 3),
            3,
            0,
        );
        let backward = record(forward.clone(), state("abcef", 3), state("abef", 2), 2, 0);
        assert_eq!(
            undo(backward).state.text,
            recorded_final_text("delete_direction_splits_commands")
        );
        let same_direction = record(forward, state("abcef", 3), state("abcf", 3), 3, 0);
        assert_eq!(
            undo(same_direction).state.text,
            recorded_final_text("same_direction_deletes_merge")
        );
    }
}
