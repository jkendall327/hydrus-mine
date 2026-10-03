//! The string processor editor, bound (`ui/string_processor.slint`):
//! hydrus-gui-model's [`ProcessorEditor`] in a window, and the editors of
//! its steps: a splitter, joiner, selector/slicer or sorter in a step
//! window, as is a string match; a tag filter in the tag filter editor; a
//! converter in the converter editor ([`open_converter`]), whose
//! conversions each open in a conversion window.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::url::strings::{
    Conversion, ProcessingStep, StringConverter, StringProcessor, TagFilterStep,
};
use hydrus_store::Store;

use crate::string_editors::{
    ADD_CHOICES, ADD_TITLE, CHARACTER_SETS, CONVERSION_TITLE, CONVERSION_TYPES, ConversionEditor,
    ConverterEditor, DATEPARSER_NOTE, ENCODINGS, HASH_FUNCTIONS, JoinerEditor, MatchEditor,
    ProcessorEditor, SORT_TYPES, STEP_TITLE, SlicerEditor, SorterEditor, SplitterEditor,
};
use crate::{
    ConversionWindow, StringConverterWindow, StringProcessorWindow, StringStepWindow, TableRow,
};

thread_local! {
    /// The last conversion "ok"ed in any conversion window, which "add"
    /// starts from (the reference keeps it in its options).
    static LAST_CONVERSION: RefCell<Option<Conversion>> = const { RefCell::new(None) };
}

/// The windows while they are open.
#[derive(Clone, Default)]
pub struct Slots {
    pub processor: Rc<RefCell<Option<StringProcessorWindow>>>,
    pub step: Rc<RefCell<Option<StringStepWindow>>>,
    pub tag_filter: crate::tag_filter_window::Slot,
    pub converter: Rc<RefCell<Option<StringConverterWindow>>>,
    pub conversion: Rc<RefCell<Option<ConversionWindow>>>,
}

impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slots")
            .field("processor", &self.processor.borrow().is_some())
            .field("step", &self.step.borrow().is_some())
            .field("converter", &self.converter.borrow().is_some())
            .field("conversion", &self.conversion.borrow().is_some())
            .finish_non_exhaustive()
    }
}

fn rows(texts: impl IntoIterator<Item = String>, selected: &[usize]) -> ModelRc<TableRow> {
    let rows: Vec<TableRow> = texts
        .into_iter()
        .enumerate()
        .map(|(i, text)| TableRow {
            cells: ModelRc::new(VecModel::from(vec![SharedString::from(text)])),
            selected: selected.contains(&i),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

fn strings(texts: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    let texts: Vec<SharedString> = texts.into_iter().map(Into::into).collect();
    ModelRc::new(VecModel::from(texts))
}

fn int(n: impl TryInto<i32>) -> i32 {
    n.try_into().unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asking {
    Add,
    Delete,
}

struct State {
    editor: ProcessorEditor,
    tab: usize,
    asking: Option<Asking>,
}

fn show(window: &StringProcessorWindow, state: &State) {
    let editor = &state.editor;
    window.set_steps(rows(editor.rows(), &editor.selected()));
    window.set_any_selected(!editor.selected().is_empty());
    window.set_starting(rows(editor.starting_texts().to_vec(), &[]));
    window.set_processed(rows(editor.processed(), &[]));
    window.set_example(editor.example().into());
    let tabs = editor.example_tabs();
    let tab = state.tab.min(tabs.len().saturating_sub(1));
    window.set_tab_index(int(tab));
    window.set_tab_rows(rows(
        tabs.get(tab).map(|t| t.1.clone()).unwrap_or_default(),
        &[],
    ));
    window.set_tabs(strings(tabs.into_iter().map(|t| t.0)));
    window.set_asking(state.asking.is_some());
    match state.asking {
        Some(Asking::Add) => {
            window.set_asking_title(ADD_TITLE.into());
            window.set_asking_message(
                ADD_CHOICES
                    .iter()
                    .map(|(_, label, description)| format!("{label}: {description}"))
                    .collect::<Vec<_>>()
                    .join("\n")
                    .into(),
            );
            window.set_asking_choices(strings(ADD_CHOICES.iter().map(|c| c.1.to_owned())));
        }
        Some(Asking::Delete) => {
            window.set_asking_title("Remove".into());
            window.set_asking_message(editor.delete_question().unwrap_or_default().into());
            window.set_asking_choices(strings(["yes".to_owned()]));
        }
        None => {}
    }
}

/// Edit a string processor, with these starting strings to test it on;
/// "apply" gives it to `applied`.
pub fn open(
    store: &Arc<Store>,
    processor: &StringProcessor,
    texts: Vec<String>,
    slots: &Slots,
    applied: Rc<dyn Fn(StringProcessor)>,
) -> Result<StringProcessorWindow, slint::PlatformError> {
    let window = StringProcessorWindow::new()?;
    let state = Rc::new(RefCell::new(State {
        editor: ProcessorEditor::new(processor, texts),
        tab: 0,
        asking: None,
    }));
    let refresh: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                show(&window, &state.borrow());
            }
        }
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = slots.processor.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    });
    // a step made or edited: in the list (at its place, or the end)
    let put: Rc<dyn Fn(Option<usize>, ProcessingStep)> = Rc::new({
        let state = state.clone();
        let refresh = refresh.clone();
        move |index, step| {
            {
                let mut state = state.borrow_mut();
                match index {
                    Some(i) => state.editor.replace(i, step),
                    None => state.editor.add(step),
                }
            }
            refresh();
        }
    });
    let open_step: Rc<dyn Fn(Option<usize>, ProcessingStep)> = Rc::new({
        let store = store.clone();
        let slots = slots.clone();
        let state = state.clone();
        let put = put.clone();
        move |index, step| {
            let editor = state.borrow().editor.clone();
            open_step(&store, &editor, index, &step, &slots, put.clone());
        }
    });
    window.on_row_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        move |r, ctrl, _| {
            if let Ok(r) = usize::try_from(r) {
                state.borrow_mut().editor.click(r, ctrl);
            }
            refresh();
        }
    });
    let edit: Rc<dyn Fn()> = Rc::new({
        let state = state.clone();
        let open_step = open_step.clone();
        move || {
            let editing = state.borrow().editor.editing().map(|(i, s)| (i, s.clone()));
            if let Some((index, step)) = editing {
                open_step(Some(index), step);
            }
        }
    });
    window.on_row_activated({
        let state = state.clone();
        let edit = edit.clone();
        move |r| {
            if let Ok(r) = usize::try_from(r) {
                state.borrow_mut().editor.click(r, false);
            }
            edit();
        }
    });
    window.on_edit({
        let edit = edit.clone();
        move || edit()
    });
    for (distance, up) in [(-1, true), (1, false)] {
        let state = state.clone();
        let refresh = refresh.clone();
        let moved = move || {
            state.borrow_mut().editor.move_selected(distance);
            refresh();
        };
        if up {
            window.on_up(moved);
        } else {
            window.on_down(moved);
        }
    }
    window.on_delete({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            let mut s = state.borrow_mut();
            if s.editor.delete_question().is_some() {
                s.asking = Some(Asking::Delete);
            }
            drop(s);
            refresh();
        }
    });
    window.on_add({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            state.borrow_mut().asking = Some(Asking::Add);
            refresh();
        }
    });
    window.on_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        let open_step = open_step.clone();
        move |i| {
            let asking = state.borrow_mut().asking.take();
            match asking {
                Some(Asking::Add) => {
                    let kind = usize::try_from(i)
                        .ok()
                        .and_then(|i| ADD_CHOICES.get(i))
                        .map(|c| c.0);
                    refresh();
                    if let Some(kind) = kind {
                        let step = state.borrow().editor.new_step(kind);
                        open_step(None, step);
                    }
                }
                Some(Asking::Delete) => {
                    state.borrow_mut().editor.delete();
                    refresh();
                }
                None => {}
            }
        }
    });
    window.on_cancelled({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            state.borrow_mut().asking = None;
            refresh();
        }
    });
    window.on_text_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        move |r| {
            if let Ok(r) = usize::try_from(r) {
                state.borrow_mut().editor.select_text(r);
            }
            refresh();
        }
    });
    window.on_example_edited({
        let state = state.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            state.editor.set_example(window.get_example().to_string());
            show(&window, &state);
        }
    });
    window.on_tab_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        move |i| {
            state.borrow_mut().tab = usize::try_from(i).unwrap_or(0);
            refresh();
        }
    });
    window.on_apply({
        let state = state.clone();
        let close = close.clone();
        move || {
            let value = state.borrow().editor.value();
            close();
            applied(value);
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    show(&window, &state.borrow());
    window.show()?;
    Ok(window)
}

/// A step's editor in the step window.
#[derive(Debug, Clone)]
enum StepEditor {
    Splitter(SplitterEditor),
    Joiner(JoinerEditor),
    Slicer(SlicerEditor),
    Sorter(SorterEditor),
    Match(MatchEditor),
}

impl StepEditor {
    fn value(&self) -> Result<ProcessingStep, String> {
        match self {
            StepEditor::Splitter(e) => e.value(),
            StepEditor::Joiner(e) => Ok(e.value()),
            StepEditor::Slicer(e) => Ok(e.value()),
            StepEditor::Sorter(e) => Ok(e.value()),
            StepEditor::Match(e) => e.value().map(ProcessingStep::Filter),
        }
    }
}

fn show_step(window: &StringStepWindow, editor: &StepEditor) {
    let none: &[usize] = &[];
    match editor {
        StepEditor::Splitter(e) => {
            window.set_kind(0);
            window.set_separator(e.separator.as_str().into());
            window.set_max_splits_on(e.max_splits.is_some());
            window.set_max_splits(int(e.max_splits.unwrap_or(1)));
            window.set_split_example(e.example.as_str().into());
            window.set_summary(SharedString::new());
            let (results, invalid) = e.results();
            window.set_results(rows(results, none));
            window.set_invalid(invalid);
        }
        StepEditor::Joiner(e) => {
            window.set_kind(1);
            window.set_joiner(e.joiner.as_str().into());
            window.set_tuple_on(e.tuple_size.is_some());
            window.set_tuple_size(int(e.tuple_size.unwrap_or(2)));
            window.set_summary(e.summary().into());
            window.set_texts(rows(e.texts.clone(), none));
            let (results, invalid) = e.results();
            window.set_results(rows(results, none));
            window.set_invalid(invalid);
        }
        StepEditor::Slicer(e) => {
            window.set_kind(2);
            window.set_select_type(i32::from(!e.select_one));
            window.set_single(int(e.single));
            window.set_start_on(e.start.is_some());
            window.set_start(int(e.start.unwrap_or(0)));
            window.set_end_on(e.end.is_some());
            window.set_end(int(e.end.unwrap_or(-1)));
            window.set_summary(e.summary().into());
            window.set_texts(rows(e.texts.clone(), none));
            window.set_results(rows(e.results(), none));
            window.set_invalid(false);
        }
        StepEditor::Sorter(e) => {
            window.set_kind(3);
            let sort = SORT_TYPES.iter().position(|(k, _)| *k == e.kind);
            window.set_sort_type(int(sort.unwrap_or(0)));
            window.set_ascending(e.ascending);
            window.set_regex_on(e.regex.is_some());
            if let Some(regex) = &e.regex {
                window.set_regex(regex.as_str().into());
            }
            window.set_summary(SharedString::new());
            window.set_texts(rows(e.texts.clone(), none));
            window.set_results(rows(e.results(), none));
            window.set_invalid(false);
        }
        StepEditor::Match(e) => {
            window.set_kind(4);
            window.set_match_type(int(e.match_type));
            window.set_fixed(e.fixed.as_str().into());
            window.set_match_regex(e.regex.as_str().into());
            let set = CHARACTER_SETS.iter().position(|(f, _)| *f == e.flexible);
            window.set_character_set(int(set.unwrap_or(0)));
            window.set_min_on(e.min_chars.is_some());
            window.set_min_chars(int(e.min_chars.unwrap_or(16)));
            window.set_max_on(e.max_chars.is_some());
            window.set_max_chars(int(e.max_chars.unwrap_or(64)));
            window.set_match_example(e.example.as_str().into());
            let shown = e.shown();
            window.set_show_fixed(shown.fixed);
            window.set_show_match_regex(shown.regex);
            window.set_show_character_set(shown.flexible);
            window.set_show_limits(shown.limits);
            let (text, ok) = e.test_result().unwrap_or_default();
            window.set_test_result(text.into());
            window.set_test_ok(ok);
            window.set_summary(SharedString::new());
            window.set_invalid(false);
        }
    }
}

/// Take in what was typed or chosen.
fn read_step(window: &StringStepWindow, editor: &mut StepEditor) {
    let count = |on: bool, n: i32| on.then(|| usize::try_from(n).unwrap_or(0));
    match editor {
        StepEditor::Splitter(e) => {
            e.separator = window.get_separator().to_string();
            e.max_splits = count(window.get_max_splits_on(), window.get_max_splits());
            e.example = window.get_split_example().to_string();
        }
        StepEditor::Joiner(e) => {
            e.joiner = window.get_joiner().to_string();
            e.tuple_size = count(window.get_tuple_on(), window.get_tuple_size());
        }
        StepEditor::Slicer(e) => {
            e.select_one = window.get_select_type() == 0;
            e.single = i64::from(window.get_single());
            e.start = window.get_start_on().then(|| i64::from(window.get_start()));
            e.end = window.get_end_on().then(|| i64::from(window.get_end()));
        }
        StepEditor::Sorter(e) => {
            let sort = usize::try_from(window.get_sort_type()).unwrap_or(0);
            e.kind = SORT_TYPES.get(sort).map_or(e.kind, |(k, _)| *k);
            e.ascending = window.get_ascending();
            e.regex = window
                .get_regex_on()
                .then(|| window.get_regex().to_string());
        }
        StepEditor::Match(e) => {
            e.fixed = window.get_fixed().to_string();
            e.regex = window.get_match_regex().to_string();
            let set = usize::try_from(window.get_character_set()).unwrap_or(0);
            e.flexible = CHARACTER_SETS.get(set).map_or(e.flexible, |(f, _)| *f);
            e.min_chars = count(window.get_min_on(), window.get_min_chars());
            e.max_chars = count(window.get_max_on(), window.get_max_chars());
            e.example = window.get_match_example().to_string();
            let match_type = usize::try_from(window.get_match_type()).unwrap_or(0);
            if match_type != e.match_type {
                e.set_type(match_type);
            }
        }
    }
}

/// Open a step's editor: a new step (`index` none) or the one at `index`;
/// "apply" gives it to `put`.
fn open_step(
    store: &Arc<Store>,
    editor: &ProcessorEditor,
    index: Option<usize>,
    step: &ProcessingStep,
    slots: &Slots,
    put: Rc<dyn Fn(Option<usize>, ProcessingStep)>,
) {
    let step_editor = match step {
        ProcessingStep::Split {
            separator,
            max_splits,
        } => StepEditor::Splitter(SplitterEditor::new(
            separator,
            *max_splits,
            editor.example_text_for(step),
        )),
        ProcessingStep::Join { joiner, tuple_size } => StepEditor::Joiner(JoinerEditor::new(
            joiner,
            *tuple_size,
            editor.example_texts_for(step),
        )),
        ProcessingStep::Slice { start, end } => StepEditor::Slicer(SlicerEditor::new(
            *start,
            *end,
            editor.example_texts_for(step),
        )),
        ProcessingStep::Sort {
            kind,
            ascending,
            regex,
        } => StepEditor::Sorter(SorterEditor::new(
            *kind,
            *ascending,
            regex
                .as_ref()
                .map(hydrus_core::url::strings::PyRegex::pattern),
            editor.example_texts_for(step),
        )),
        ProcessingStep::Filter(string_match) => StepEditor::Match(MatchEditor::new(string_match)),
        ProcessingStep::TagFilter(tag_filter) => {
            if slots.tag_filter.borrow().is_some() {
                return;
            }
            let example = tag_filter.example.clone();
            let applied: Rc<dyn Fn(hydrus_core::tag_filter::TagFilter)> = Rc::new(move |filter| {
                put(
                    index,
                    ProcessingStep::TagFilter(TagFilterStep {
                        filter,
                        example: example.clone(),
                    }),
                );
            });
            match crate::tag_filter_window::open(
                store,
                &tag_filter.filter,
                false,
                STEP_TITLE,
                "",
                &slots.tag_filter,
                applied,
            ) {
                Ok(w) => *slots.tag_filter.borrow_mut() = Some(w),
                Err(e) => eprintln!("could not open the tag filter: {e}"),
            }
            return;
        }
        ProcessingStep::Convert(converter) => {
            if slots.converter.borrow().is_some() {
                return;
            }
            let applied: Rc<dyn Fn(StringConverter)> =
                Rc::new(move |converter| put(index, ProcessingStep::Convert(converter)));
            match open_converter(
                converter,
                Some(editor.example_text_for(step)),
                slots,
                applied,
            ) {
                Ok(w) => *slots.converter.borrow_mut() = Some(w),
                Err(e) => eprintln!("could not open the string converter: {e}"),
            }
            return;
        }
        ProcessingStep::Unsupported { .. } => return,
    };
    if slots.step.borrow().is_some() {
        return;
    }
    let Ok(window) = StringStepWindow::new() else {
        return;
    };
    window.set_window_title(STEP_TITLE.into());
    let state = Rc::new(RefCell::new(step_editor));
    show_step(&window, &state.borrow());
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = slots.step.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    });
    window.on_changed({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut editor = state.borrow_mut();
            read_step(&window, &mut editor);
            show_step(&window, &editor);
            window.set_veto(SharedString::new());
        }
    });
    window.on_apply({
        let weak = window.as_weak();
        let state = state.clone();
        let close = close.clone();
        move || {
            let value = state.borrow().value();
            match value {
                Ok(step) => {
                    close();
                    put(index, step);
                }
                Err(veto) => {
                    if let Some(window) = weak.upgrade() {
                        window.set_veto(veto.into());
                    }
                }
            }
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    if window.show().is_ok() {
        *slots.step.borrow_mut() = Some(window);
    }
}

fn show_converter(window: &StringConverterWindow, editor: &ConverterEditor, asking: bool) {
    let selected = editor.selected();
    let rows: Vec<TableRow> = editor
        .rows()
        .into_iter()
        .enumerate()
        .map(|(i, cells)| TableRow {
            cells: ModelRc::new(VecModel::from(
                cells
                    .into_iter()
                    .map(SharedString::from)
                    .collect::<Vec<_>>(),
            )),
            selected: selected.contains(&i),
        })
        .collect();
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    window.set_one_selected(selected.len() == 1);
    window.set_any_selected(!selected.is_empty());
    window.set_can_up(editor.can_move_up());
    window.set_can_down(editor.can_move_down());
    window.set_example(editor.example.as_str().into());
    window.set_asking(asking);
    if asking {
        window.set_asking_title("Delete".into());
        window.set_asking_message(editor.delete_question().unwrap_or_default().into());
        window.set_asking_choices(strings(["yes".to_owned()]));
    }
}

/// Edit a string converter, with an example in place of its own if
/// given; "apply" gives it to `applied`.
pub fn open_converter(
    converter: &StringConverter,
    example: Option<String>,
    slots: &Slots,
    applied: Rc<dyn Fn(StringConverter)>,
) -> Result<StringConverterWindow, slint::PlatformError> {
    let window = StringConverterWindow::new()?;
    let state = Rc::new(RefCell::new((
        ConverterEditor::new(converter, example),
        false,
    )));
    let refresh: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let state = state.borrow();
                show_converter(&window, &state.0, state.1);
            }
        }
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = slots.converter.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    });
    // a conversion window for "add" (`index` none) or "edit"
    let open: Rc<dyn Fn(Option<usize>, ConversionEditor)> = Rc::new({
        let state = state.clone();
        let refresh = refresh.clone();
        let slot = slots.conversion.clone();
        move |index, editor| {
            if slot.borrow().is_some() {
                return;
            }
            let done: Rc<dyn Fn(Conversion)> = Rc::new({
                let state = state.clone();
                let refresh = refresh.clone();
                move |conversion| {
                    LAST_CONVERSION.with(|last| *last.borrow_mut() = Some(conversion.clone()));
                    {
                        let editor = &mut state.borrow_mut().0;
                        match index {
                            Some(i) => editor.replace(i, conversion),
                            None => editor.add(conversion),
                        }
                    }
                    refresh();
                }
            });
            match open_conversion(editor, &slot, done) {
                Ok(w) => *slot.borrow_mut() = Some(w),
                Err(e) => eprintln!("could not open the conversion: {e}"),
            }
        }
    });
    window.on_row_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        move |r, ctrl, _| {
            if let Ok(r) = usize::try_from(r) {
                state.borrow_mut().0.click(r, ctrl);
            }
            refresh();
        }
    });
    let edit: Rc<dyn Fn()> = Rc::new({
        let state = state.clone();
        let open = open.clone();
        move || {
            let editing = state.borrow().0.editing();
            if let Some((index, editor)) = editing {
                open(Some(index), editor);
            }
        }
    });
    window.on_row_activated({
        let state = state.clone();
        let edit = edit.clone();
        move |r| {
            if let Ok(r) = usize::try_from(r) {
                state.borrow_mut().0.click(r, false);
            }
            edit();
        }
    });
    window.on_edit({
        let edit = edit.clone();
        move || edit()
    });
    window.on_add({
        let state = state.clone();
        let open = open.clone();
        move || {
            let editor =
                LAST_CONVERSION.with(|last| state.borrow().0.adding(last.borrow().as_ref()));
            open(None, editor);
        }
    });
    window.on_delete({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            {
                let mut state = state.borrow_mut();
                state.1 = state.0.delete_question().is_some();
            }
            refresh();
        }
    });
    window.on_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        move |_| {
            {
                let mut state = state.borrow_mut();
                state.1 = false;
                state.0.delete();
            }
            refresh();
        }
    });
    window.on_cancelled({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            state.borrow_mut().1 = false;
            refresh();
        }
    });
    for (distance, up) in [(-1, true), (1, false)] {
        let state = state.clone();
        let refresh = refresh.clone();
        let moved = move || {
            state.borrow_mut().0.move_selected(distance);
            refresh();
        };
        if up {
            window.on_up(moved);
        } else {
            window.on_down(moved);
        }
    }
    window.on_example_edited({
        let state = state.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            state.0.example = window.get_example().to_string();
            show_converter(&window, &state.0, state.1);
        }
    });
    window.on_apply({
        let state = state.clone();
        let close = close.clone();
        move || {
            let value = state.borrow().0.value();
            close();
            applied(value);
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    refresh();
    window.show()?;
    Ok(window)
}

fn show_conversion(window: &ConversionWindow, editor: &ConversionEditor, asking: Option<&str>) {
    window.set_types(strings(
        CONVERSION_TYPES.iter().map(|(_, l)| (*l).to_owned()),
    ));
    window.set_kind(int(editor.kind));
    let shown = editor.shown();
    window.set_text_label(shown.text.unwrap_or_default().into());
    window.set_text(editor.text.as_str().into());
    window.set_number_label(shown.number.unwrap_or_default().into());
    let min = match CONVERSION_TYPES[editor.kind].0 {
        15 => 1,
        11 => -65535,
        _ => 0,
    };
    window.set_number_min(min);
    window.set_number(int(editor.number));
    window.set_encodings(strings(ENCODINGS.iter().map(|(_, l)| (*l).to_owned())));
    window.set_show_encoding(shown.encoding);
    window.set_encoding(int(editor.encoding));
    window.set_show_decoding(shown.decoding);
    window.set_decoding(int(editor.decoding));
    window.set_show_regex(shown.regex);
    window.set_pattern(editor.pattern.as_str().into());
    window.set_replacement(editor.replacement.as_str().into());
    window.set_show_date_link(shown.date_link);
    window.set_show_timezone_decode(shown.timezone_decode);
    window.set_timezone_decode(int(editor.timezone_decode));
    window.set_show_timezone_offset(shown.timezone_offset);
    window.set_timezone_offset(int(editor.timezone_offset));
    window.set_show_timezone_encode(shown.timezone_encode);
    window.set_timezone_encode(int(editor.timezone_encode));
    window.set_hashes(strings(HASH_FUNCTIONS.iter().map(|(_, l)| (*l).to_owned())));
    window.set_show_hash(shown.hash);
    window.set_hash(int(editor.hash));
    window.set_dateparser_note(
        if shown.dateparser {
            DATEPARSER_NOTE
        } else {
            ""
        }
        .into(),
    );
    window.set_example(editor.example.as_str().into());
    window.set_result(editor.result().into());
    window.set_asking(asking.is_some());
    if let Some(question) = asking {
        window.set_asking_title("Are you sure?".into());
        window.set_asking_message(question.into());
        window.set_asking_choices(strings(["yes".to_owned()]));
    }
}

/// Take in what was typed or chosen.
fn read_conversion(window: &ConversionWindow, editor: &mut ConversionEditor) {
    let index = |i: i32| usize::try_from(i).unwrap_or(0);
    let kind = index(window.get_kind());
    if kind == editor.kind {
        editor.set_number(i64::from(window.get_number()));
    } else {
        editor.set_kind(kind);
    }
    editor.text = window.get_text().to_string();
    editor.encoding = index(window.get_encoding());
    editor.decoding = index(window.get_decoding());
    editor.pattern = window.get_pattern().to_string();
    editor.replacement = window.get_replacement().to_string();
    editor.timezone_decode = index(window.get_timezone_decode());
    editor.timezone_offset = i64::from(window.get_timezone_offset());
    editor.timezone_encode = index(window.get_timezone_encode());
    editor.hash = index(window.get_hash());
}

/// Edit a conversion; "apply" (after its question, if it asks one) gives
/// it to `done`.
fn open_conversion(
    editor: ConversionEditor,
    slot: &Rc<RefCell<Option<ConversionWindow>>>,
    done: Rc<dyn Fn(Conversion)>,
) -> Result<ConversionWindow, slint::PlatformError> {
    let window = ConversionWindow::new()?;
    window.set_window_title(CONVERSION_TITLE.into());
    let state = Rc::new(RefCell::new((editor, None::<&'static str>)));
    show_conversion(&window, &state.borrow().0, None);
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    });
    let finish: Rc<dyn Fn()> = Rc::new({
        let state = state.clone();
        let close = close.clone();
        move || {
            let value = state.borrow().0.value();
            close();
            done(value);
        }
    });
    window.on_changed({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            read_conversion(&window, &mut state.0);
            show_conversion(&window, &state.0, state.1);
        }
    });
    window.on_apply({
        let weak = window.as_weak();
        let state = state.clone();
        let finish = finish.clone();
        move || {
            let question = state.borrow().0.ok_question();
            match question {
                Some(question) => {
                    state.borrow_mut().1 = Some(question);
                    if let Some(window) = weak.upgrade() {
                        show_conversion(&window, &state.borrow().0, Some(question));
                    }
                }
                None => finish(),
            }
        }
    });
    window.on_chosen({
        let finish = finish.clone();
        move |_| finish()
    });
    window.on_cancelled({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            state.borrow_mut().1 = None;
            if let Some(window) = weak.upgrade() {
                show_conversion(&window, &state.borrow().0, None);
            }
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    window.show()?;
    Ok(window)
}
