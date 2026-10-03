//! The reference's string processing editors (`ClientGUIStringPanels`),
//! without Slint: the string processor's (`EditStringProcessorPanel`: its
//! steps in a list with up, delete, down, add and edit, and its test
//! panels, the starting strings processed and a single example through
//! each step), and the editors of the steps it opens: splitter, joiner,
//! sorter and selector/slicer.

use hydrus_core::url::strings::{
    ProcessingStep, PyRegex, SortKind, StringConverter, StringMatch, StringProcessor,
    TagFilterStep, join_texts, slice_texts, sort_texts, split_text,
};

/// What a step's results show when there are none (`NO_RESULTS_TEXT`).
pub const NO_RESULTS_TEXT: &str = "no results";

/// "add"'s question (`SelectFromListButtons`).
pub const ADD_TITLE: &str = "Which type of processing step?";

/// The kinds of step "add" offers, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    Match,
    TagFilter,
    Converter,
    Splitter,
    Joiner,
    Sorter,
    Slicer,
}

/// "add"'s choices: the kind, its button and its description.
pub const ADD_CHOICES: [(StepKind, &str, &str); 7] = [
    (
        StepKind::Match,
        "String Match",
        "An object that filters strings.",
    ),
    (
        StepKind::TagFilter,
        "String Tag Filter",
        "An object that filters strings using tag rules.",
    ),
    (
        StepKind::Converter,
        "String Converter",
        "An object that converts strings from one thing to another.",
    ),
    (
        StepKind::Splitter,
        "String Splitter",
        "An object that breaks strings into smaller strings.",
    ),
    (
        StepKind::Joiner,
        "String Joiner",
        "An object that concatenates strings together.",
    ),
    (
        StepKind::Sorter,
        "String Sorter",
        "An object that sorts strings.",
    ),
    (
        StepKind::Slicer,
        "String Selector/Slicer",
        "An object that filter-selects from the list of strings. Either absolute index position or a range.",
    ),
];

/// The editor's title for any step (`DialogEdit`'s).
pub const STEP_TITLE: &str = "edit processing step";

/// The kind of a step.
pub fn kind_of(step: &ProcessingStep) -> Option<StepKind> {
    Some(match step {
        ProcessingStep::Convert(_) => StepKind::Converter,
        ProcessingStep::Filter(_) => StepKind::Match,
        ProcessingStep::Split { .. } => StepKind::Splitter,
        ProcessingStep::Slice { .. } => StepKind::Slicer,
        ProcessingStep::Join { .. } => StepKind::Joiner,
        ProcessingStep::Sort { .. } => StepKind::Sorter,
        ProcessingStep::TagFilter(_) => StepKind::TagFilter,
        ProcessingStep::Unsupported { .. } => return None,
    })
}

/// A string processor being edited, with its test strings.
#[derive(Debug, Clone)]
pub struct ProcessorEditor {
    steps: Vec<ProcessingStep>,
    /// The starting strings (the test data's texts).
    texts: Vec<String>,
    /// The single example string (the first starting string, at first).
    example: String,
    /// Whether each step's row is selected.
    selected: Vec<bool>,
}

impl ProcessorEditor {
    pub fn new(processor: &StringProcessor, texts: Vec<String>) -> Self {
        let example = texts.first().cloned().unwrap_or_default();
        Self {
            selected: vec![false; processor.steps.len()],
            steps: processor.steps.clone(),
            texts,
            example,
        }
    }

    pub fn value(&self) -> StringProcessor {
        StringProcessor {
            steps: self.steps.clone(),
        }
    }

    pub fn steps(&self) -> &[ProcessingStep] {
        &self.steps
    }

    /// The list's rows (`ToString( with_type = True )`).
    pub fn rows(&self) -> Vec<String> {
        self.steps.iter().map(|s| s.describe(false, true)).collect()
    }

    pub fn starting_texts(&self) -> &[String] {
        &self.texts
    }

    /// A starting string clicked: the single example is it.
    pub fn select_text(&mut self, index: usize) {
        if let Some(text) = self.texts.get(index) {
            self.example.clone_from(text);
        }
    }

    pub fn example(&self) -> &str {
        &self.example
    }

    /// The single example typed.
    pub fn set_example(&mut self, text: String) {
        self.example = text;
    }

    /// The starting strings processed ("processed strings").
    pub fn processed(&self) -> Vec<String> {
        self.value()
            .process(self.texts.clone())
            .unwrap_or_else(|e| vec![format!("error in processing: {e}")])
    }

    /// The single example through each step but the slicers ("results
    /// for each step"): each tab's label ("splitter (3)") and its rows,
    /// stopping after one with no results or an error.
    pub fn example_tabs(&self) -> Vec<(String, Vec<String>)> {
        let processor = self.value();
        let mut tabs = Vec::new();
        for (i, step) in self.steps.iter().enumerate() {
            if matches!(step, ProcessingStep::Slice { .. }) {
                continue;
            }
            let (results, mut stop) =
                match processor.process_limited(vec![self.example.clone()], Some(i + 1), true) {
                    Ok(results) => (results, false),
                    Err(e) => (vec![format!("error: {e}")], true),
                };
            let label = format!(
                "{} ({})",
                step.describe(true, false),
                hydrus_core::numbers::human_int(results.len() as u64)
            );
            let rows = if results.is_empty() {
                stop = true;
                vec![NO_RESULTS_TEXT.to_owned()]
            } else {
                results
            };
            tabs.push((label, rows));
            if stop {
                break;
            }
        }
        tabs
    }

    /// Where a step's example comes from: its place, if the list has one
    /// equal to it, else the end (`_GetExampleTextForStringProcessingStep`).
    fn example_index(&self, step: &ProcessingStep) -> usize {
        self.steps
            .iter()
            .position(|s| s == step)
            .unwrap_or(self.steps.len())
    }

    /// The single example a step's editor is given: the first row of the
    /// example tab before its place (counting tabs, which skip slicers, as
    /// the reference does), or the example itself.
    pub fn example_text_for(&self, step: &ProcessingStep) -> String {
        let index = self.example_index(step);
        let tabs = self.example_tabs();
        if index > 0
            && index < tabs.len() + 1
            && let Some(first) = tabs[index - 1].1.first()
            && first != NO_RESULTS_TEXT
        {
            return first.clone();
        }
        self.example.clone()
    }

    /// The strings a sorter, slicer or joiner's editor is given: the
    /// starting strings through the steps up to and including its place
    /// (`GetResultTexts( index )` processes `index + 1` steps).
    pub fn example_texts_for(&self, step: &ProcessingStep) -> Vec<String> {
        let index = self.example_index(step);
        self.value()
            .process_limited(self.texts.clone(), Some(index + 1), false)
            .unwrap_or_default()
    }

    /// A new step of a kind, as "add" makes it before its editor opens: a
    /// match or tag filter with the example its place gives.
    pub fn new_step(&self, kind: StepKind) -> ProcessingStep {
        match kind {
            StepKind::Match | StepKind::TagFilter => {
                let with_example = |example: String| match kind {
                    StepKind::Match => ProcessingStep::Filter(StringMatch {
                        example,
                        ..StringMatch::any()
                    }),
                    _ => ProcessingStep::TagFilter(TagFilterStep {
                        example,
                        ..TagFilterStep::new(hydrus_core::tag_filter::TagFilter::default())
                    }),
                };
                let first = with_example(self.example.clone());
                with_example(self.example_text_for(&first))
            }
            StepKind::Converter => ProcessingStep::Convert(StringConverter::default()),
            StepKind::Splitter => ProcessingStep::Split {
                separator: ",".to_owned(),
                max_splits: None,
            },
            StepKind::Joiner => ProcessingStep::Join {
                joiner: String::new(),
                tuple_size: None,
            },
            StepKind::Sorter => ProcessingStep::Sort {
                kind: SortKind::Human,
                ascending: false,
                regex: None,
            },
            StepKind::Slicer => ProcessingStep::Slice {
                start: None,
                end: None,
            },
        }
    }

    pub fn selected(&self) -> Vec<usize> {
        (0..self.steps.len())
            .filter(|&i| self.selected[i])
            .collect()
    }

    /// A row clicked, with ctrl held to add to the selection or not.
    pub fn click(&mut self, row: usize, ctrl: bool) {
        if row >= self.steps.len() {
            return;
        }
        if ctrl {
            self.selected[row] = !self.selected[row];
        } else {
            self.selected.fill(false);
            self.selected[row] = true;
        }
    }

    /// A step added at the end.
    pub fn add(&mut self, step: ProcessingStep) {
        self.steps.push(step);
        self.selected.push(false);
    }

    /// The step "edit" edits: the first selected.
    pub fn editing(&self) -> Option<(usize, &ProcessingStep)> {
        let index = self.selected.iter().position(|&s| s)?;
        Some((index, &self.steps[index]))
    }

    pub fn replace(&mut self, index: usize, step: ProcessingStep) {
        if let Some(slot) = self.steps.get_mut(index) {
            *slot = step;
        }
    }

    /// "X"'s question, if anything is selected ("Remove 2 selected?").
    pub fn delete_question(&self) -> Option<String> {
        let count = self.selected().len();
        (count > 0).then(|| {
            format!(
                "Remove {} selected?",
                hydrus_core::numbers::human_int(count as u64)
            )
        })
    }

    /// The selected steps removed ("yes" to the question).
    pub fn delete(&mut self) {
        let mut kept = self.selected.iter().map(|&s| !s);
        self.steps.retain(|_| kept.next().unwrap_or(true));
        self.selected.retain(|&s| !s);
    }

    /// The selected steps moved one up (-1) or down (1), each in turn
    /// from the end they move to, as the reference's list moves its rows
    /// (each keeping whether it is selected).
    pub fn move_selected(&mut self, distance: isize) {
        let mut indices = self.selected();
        if distance > 0 {
            indices.reverse();
        }
        let last = self.steps.len().saturating_sub(1);
        for index in indices {
            let new = index.saturating_add_signed(distance).min(last);
            if new != index {
                let step = self.steps.remove(index);
                self.steps.insert(new, step);
                let selected = self.selected.remove(index);
                self.selected.insert(new, selected);
            }
        }
    }
}

/// A splitter's editor (`EditStringSplitterPanel`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitterEditor {
    pub separator: String,
    pub max_splits: Option<usize>,
    pub example: String,
}

impl SplitterEditor {
    pub fn new(separator: &str, max_splits: Option<usize>, example: String) -> Self {
        Self {
            separator: separator.to_owned(),
            max_splits,
            example,
        }
    }

    /// The example split ("result:"), and whether the separator is shown
    /// as invalid.
    pub fn results(&self) -> (Vec<String>, bool) {
        if self.separator.is_empty() {
            return (Vec::new(), true);
        }
        match split_text(&self.separator, self.max_splits, &self.example) {
            Ok(results) => (results, false),
            Err(e) => (vec![format!("Error: {e}")], true),
        }
    }

    /// "ok": the step, or why not.
    pub fn value(&self) -> Result<ProcessingStep, String> {
        if self.separator.is_empty() {
            return Err("Sorry, you have to have a value in the separator field!".to_owned());
        }
        Ok(ProcessingStep::Split {
            separator: self.separator.clone(),
            max_splits: self.max_splits,
        })
    }
}

/// A joiner's editor (`EditStringJoinerPanel`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinerEditor {
    pub joiner: String,
    /// "join groups of size" (at least 2), or "merge all into one string".
    pub tuple_size: Option<usize>,
    pub texts: Vec<String>,
}

impl JoinerEditor {
    pub fn new(joiner: &str, tuple_size: Option<usize>, texts: Vec<String>) -> Self {
        Self {
            joiner: joiner.to_owned(),
            tuple_size,
            texts,
        }
    }

    pub fn value(&self) -> ProcessingStep {
        ProcessingStep::Join {
            joiner: self.joiner.clone(),
            tuple_size: self.tuple_size,
        }
    }

    /// The summary under the boxes (`ToString`).
    pub fn summary(&self) -> String {
        self.value().describe(false, false)
    }

    /// The test strings joined, and whether the joiner is shown as
    /// invalid.
    pub fn results(&self) -> (Vec<String>, bool) {
        match join_texts(&self.joiner, self.tuple_size, &self.texts) {
            Ok(results) => (results, false),
            Err(e) => (vec![format!("Error: {e}")], true),
        }
    }
}

/// A selector/slicer's editor (`EditStringSlicerPanel`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlicerEditor {
    /// "select one item", or "select range".
    pub select_one: bool,
    /// "index to select".
    pub single: i64,
    /// "starting index" ("start at the beginning" for none).
    pub start: Option<i64>,
    /// "ending index" ("finish at the end" for none).
    pub end: Option<i64>,
    pub texts: Vec<String>,
}

impl SlicerEditor {
    pub fn new(start: Option<i64>, end: Option<i64>, texts: Vec<String>) -> Self {
        Self {
            select_one: selects_one(start, end),
            single: start.unwrap_or(0),
            start,
            end,
            texts,
        }
    }

    pub fn value(&self) -> ProcessingStep {
        if self.select_one {
            let end = (self.single != -1).then_some(self.single + 1);
            ProcessingStep::Slice {
                start: Some(self.single),
                end,
            }
        } else {
            ProcessingStep::Slice {
                start: self.start,
                end: self.end,
            }
        }
    }

    pub fn summary(&self) -> String {
        self.value().describe(false, false)
    }

    /// The test strings sliced.
    pub fn results(&self) -> Vec<String> {
        let ProcessingStep::Slice { start, end } = self.value() else {
            unreachable!("a slice")
        };
        slice_texts(start, end, &self.texts)
    }
}

/// `StringSlicer.SelectsNothingEver`.
fn selects_nothing(start: Option<i64>, end: Option<i64>) -> bool {
    if end == Some(0) {
        return true;
    }
    let (Some(start), Some(end)) = (start, end) else {
        return false;
    };
    let same_side = (start >= 0 && end >= 0) || (start < 0 && end < 0);
    same_side && start >= end
}

/// `StringSlicer.SelectsOne`.
fn selects_one(start: Option<i64>, end: Option<i64>) -> bool {
    if selects_nothing(start, end) {
        return false;
    }
    if start == Some(-1) && end.is_none() {
        return true;
    }
    let (Some(start), Some(end)) = (start, end) else {
        return false;
    };
    let same_side = (start >= 0 && end >= 0) || (start < 0 && end < 0);
    same_side && start == end - 1
}

/// The sort types a sorter's editor offers, with their names.
pub const SORT_TYPES: [(SortKind, &str); 3] = [
    (SortKind::Human, "human sort"),
    (SortKind::Lexicographic, "strict lexicographic"),
    (SortKind::Reverse, "reverse"),
];

/// A sorter's editor (`EditStringSorterPanel`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SorterEditor {
    pub kind: SortKind,
    pub ascending: bool,
    /// "regex for substring sorting" ("use whole string" for none).
    pub regex: Option<String>,
    pub texts: Vec<String>,
}

impl SorterEditor {
    pub fn new(kind: SortKind, ascending: bool, regex: Option<&str>, texts: Vec<String>) -> Self {
        Self {
            kind,
            ascending,
            regex: regex.map(str::to_owned),
            texts,
        }
    }

    pub fn value(&self) -> ProcessingStep {
        ProcessingStep::Sort {
            kind: self.kind,
            ascending: self.ascending,
            regex: self.regex.as_deref().map(PyRegex::new),
        }
    }

    /// The test strings sorted, each with what the regex found in it ("a2
    /// (regex: 2)", "b (no regex match)"); an error's text where the
    /// regex won't compile.
    pub fn results(&self) -> Vec<String> {
        let regex = self.regex.as_deref().map(PyRegex::new);
        let sorted = match sort_texts(self.kind, self.ascending, regex.as_ref(), &self.texts) {
            Ok(sorted) => sorted,
            Err(e) => vec![format!("Error: {e}")],
        };
        let Some(Ok(regex)) = regex.as_ref().map(PyRegex::regex) else {
            return sorted;
        };
        sorted
            .into_iter()
            .map(|s| match regex.find(&s) {
                Ok(Some(m)) => format!("{s} (regex: {})", m.as_str()),
                Ok(None) => format!("{s} (no regex match)"),
                Err(_) => s,
            })
            .collect()
    }
}
