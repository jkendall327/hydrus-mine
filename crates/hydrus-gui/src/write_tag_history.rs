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
    typing: bool,
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
    if typing
        && before.anchor == before.cursor
        && let Some(last) = undos.last_mut()
    {
        let grows = after.text.len() > before.text.len();
        let grew = last.after.text.len() > last.before.text.len();
        if last.typing
            && last.after.text == before.text
            && last.after.cursor == before.cursor
            && grows == grew
        {
            last.after.clone_from(&after);
            merged = true;
        }
    }
    if !merged {
        undos.push(TagTextEdit {
            before,
            after: after.clone(),
            typing,
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
