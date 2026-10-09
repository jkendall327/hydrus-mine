//! The reference's string processing editors (`ClientGUIStringPanels`),
//! without Slint: the string processor's (`EditStringProcessorPanel`: its
//! steps in a list with up, delete, down, add and edit, and its test
//! panels, the starting strings processed and a single example through
//! each step), and the editors of the steps it opens: splitter, joiner,
//! sorter and selector/slicer.

use hydrus_core::pyjson::PyJson;
use hydrus_core::url::strings::{
    Conversion, DateTimezone, Encoding, FlexibleMatch, HashFunction, MatchKind, ProcessingStep,
    PyRegex, SortKind, StringConverter, StringMatch, StringProcessor, TagFilterStep, join_texts,
    slice_texts, sort_texts, split_text,
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

    /// Selected queue entries, in their execution order, for reference export.
    pub fn export_steps(&self) -> Vec<ProcessingStep> {
        self.selected()
            .into_iter()
            .map(|i| self.steps[i].clone())
            .collect()
    }

    /// Append a fully validated reference package without changing selection.
    pub fn import_text(&mut self, text: &str) -> Result<usize, String> {
        let steps =
            hydrus_downloader_exchange::processing::decode_text(text).map_err(|e| e.to_string())?;
        let count = steps.len();
        for step in steps {
            self.add(step);
        }
        Ok(count)
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

/// A match editor's types, in its order (`STRING_MATCH_*`).
pub const MATCH_TYPES: [&str; 4] = [
    "any characters",
    "fixed characters",
    "character set",
    "regex",
];

/// A match editor's character sets, in its order.
pub const CHARACTER_SETS: [(FlexibleMatch, &str); 7] = [
    (FlexibleMatch::Alpha, "alphabetic characters (a-zA-Z)"),
    (
        FlexibleMatch::Alphanumeric,
        "alphanumeric characters (a-zA-Z0-9)",
    ),
    (FlexibleMatch::Numeric, "numeric characters (0-9)"),
    (FlexibleMatch::Hex, "hexadecimal characters (0-9a-fA-F)"),
    (
        FlexibleMatch::Base64,
        "base64 characters (a-zA-z0-9+/ with = padding)",
    ),
    (
        FlexibleMatch::Base64Url,
        "base64url characters (a-zA-z0-9-_ optionally with = padding)",
    ),
    (
        FlexibleMatch::Base64UrlEncoded,
        "base64 characters (url encoded) (a-zA-z0-9 %2B %2F with %3D padding)",
    ),
];

/// A string match's editor (`EditStringMatchPanel`).
#[derive(Debug, Clone, PartialEq)]
pub struct MatchEditor {
    /// An index into [`MATCH_TYPES`].
    pub match_type: usize,
    pub fixed: String,
    pub regex: String,
    pub flexible: FlexibleMatch,
    /// "no limit" for none.
    pub min_chars: Option<usize>,
    pub max_chars: Option<usize>,
    pub example: String,
}

/// Which of a match editor's rows show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchShown {
    pub fixed: bool,
    pub regex: bool,
    pub flexible: bool,
    /// The limits and the example string.
    pub limits: bool,
}

impl MatchEditor {
    pub fn new(string_match: &StringMatch) -> Self {
        let mut editor = Self {
            match_type: 0,
            fixed: String::new(),
            regex: String::new(),
            flexible: FlexibleMatch::Alpha,
            min_chars: string_match.min_chars,
            max_chars: string_match.max_chars,
            example: string_match.example.clone(),
        };
        editor.match_type = match &string_match.kind {
            MatchKind::Any => 0,
            MatchKind::Fixed(fixed) => {
                editor.fixed.clone_from(fixed);
                1
            }
            MatchKind::Flexible(flexible) => {
                editor.flexible = *flexible;
                2
            }
            MatchKind::Regex(regex) => {
                regex.pattern().clone_into(&mut editor.regex);
                3
            }
        };
        editor.fill();
        editor
    }

    /// A type chosen.
    pub fn set_type(&mut self, match_type: usize) {
        self.match_type = match_type.min(MATCH_TYPES.len() - 1);
        self.fill();
    }

    /// A fixed text or regex left empty takes the example, as the
    /// reference's does when its type is shown.
    fn fill(&mut self) {
        match self.match_type {
            1 if self.fixed.is_empty() => self.fixed.clone_from(&self.example),
            3 if self.regex.is_empty() => self.regex.clone_from(&self.example),
            _ => {}
        }
    }

    pub fn shown(&self) -> MatchShown {
        MatchShown {
            fixed: self.match_type == 1,
            regex: self.match_type == 3,
            flexible: self.match_type == 2,
            limits: self.match_type != 1,
        }
    }

    /// The match as the boxes say (`_GetValue`): a fixed match has no
    /// limits, and its text as its example.
    pub fn current(&self) -> StringMatch {
        let kind = match self.match_type {
            1 => MatchKind::Fixed(self.fixed.clone()),
            2 => MatchKind::Flexible(self.flexible),
            3 => MatchKind::Regex(PyRegex::new(self.regex.clone())),
            _ => MatchKind::Any,
        };
        if self.match_type == 1 {
            StringMatch {
                kind,
                min_chars: None,
                max_chars: None,
                example: self.fixed.clone(),
            }
        } else {
            StringMatch {
                kind,
                min_chars: self.min_chars,
                max_chars: self.max_chars,
                example: self.example.clone(),
            }
        }
    }

    /// Whether the example matches ("Example matches ok!", or "Example
    /// does not match - " and why); nothing for a fixed match.
    pub fn test_result(&self) -> Option<(String, bool)> {
        if self.match_type == 1 {
            return None;
        }
        Some(match self.current().test(&self.example) {
            Ok(()) => ("Example matches ok!".to_owned(), true),
            Err(reason) => (format!("Example does not match - {reason}"), false),
        })
    }

    /// "ok": the match, or why not.
    pub fn value(&self) -> Result<StringMatch, String> {
        let current = self.current();
        current
            .test(&current.example)
            .map(|()| current.clone())
            .map_err(|_| "Please enter an example text that matches the given rules!".to_owned())
    }
}

/// The conversion editor's title (`DialogEdit`'s).
pub const CONVERSION_TITLE: &str = "edit conversion";

/// The conversion types the conversion editor offers, in its order, by
/// the reference's codes, with their names.
pub const CONVERSION_TYPES: [(i64, &str); 16] = [
    (0, "remove text from beginning of string"),
    (1, "remove text from end of string"),
    (6, "take the start of the string"),
    (7, "take the end of the string"),
    (2, "prepend text"),
    (3, "append text"),
    (15, "append random text"),
    (4, "encode"),
    (5, "decode"),
    (8, "reverse text"),
    (9, "regex substitution"),
    (14, "datestring to timestamp (easy)"),
    (10, "datestring to timestamp (advanced)"),
    (12, "timestamp to datestring"),
    (11, "integer addition"),
    (13, "get hash of string"),
];

/// The encodings the conversion editor offers, in its order.
pub const ENCODINGS: [(Encoding, &str); 6] = [
    (Encoding::HexUtf8, "hex (utf-8)"),
    (Encoding::Base64Utf8, "base64 (utf-8)"),
    (Encoding::Base64UrlUtf8, "base64url (utf-8)"),
    (Encoding::UrlPercent, "url percent encoding"),
    (Encoding::UnicodeEscape, "unicode escape characters"),
    (Encoding::HtmlEntities, "html entities"),
];

/// The hash functions it offers, in its order.
pub const HASH_FUNCTIONS: [(HashFunction, &str); 4] = [
    (HashFunction::Md5, "md5"),
    (HashFunction::Sha1, "sha1"),
    (HashFunction::Sha256, "sha256"),
    (HashFunction::Sha512, "sha512"),
];

/// A date decode's timezones ("UTC", "Local", "Offset"); a date encode
/// offers the first two.
pub const TIMEZONES: [&str; 3] = ["UTC", "Local", "Offset"];

/// A string converter's editor (`EditStringConverterPanel`): its
/// conversions numbered, each with the example converted up to it, and
/// the example string.
#[derive(Debug, Clone)]
pub struct ConverterEditor {
    conversions: Vec<Conversion>,
    pub example: String,
    selected: Vec<bool>,
}

impl ConverterEditor {
    /// The converter, with the example the editor is given in place of
    /// its own, if any.
    pub fn new(converter: &StringConverter, example: Option<String>) -> Self {
        Self {
            selected: vec![false; converter.conversions.len()],
            conversions: converter.conversions.clone(),
            example: example.unwrap_or_else(|| converter.example.clone()),
        }
    }

    pub fn value(&self) -> StringConverter {
        StringConverter {
            conversions: self.conversions.clone(),
            example: self.example.clone(),
        }
    }

    /// The list's rows: the number, the conversion, and the example
    /// converted up to and including it (or why not).
    pub fn rows(&self) -> Vec<[String; 3]> {
        let converter = self.value();
        self.conversions
            .iter()
            .enumerate()
            .map(|(i, conversion)| {
                let result = converter
                    .convert_upto(&self.example, Some(i + 1))
                    .unwrap_or_else(|e| e);
                [
                    hydrus_core::numbers::human_int(i as u64 + 1),
                    conversion.describe(),
                    result,
                ]
            })
            .collect()
    }

    pub fn selected(&self) -> Vec<usize> {
        (0..self.conversions.len())
            .filter(|&i| self.selected[i])
            .collect()
    }

    /// A row clicked, with ctrl held to add to the selection or not.
    pub fn click(&mut self, row: usize, ctrl: bool) {
        if row >= self.conversions.len() {
            return;
        }
        if ctrl {
            self.selected[row] = !self.selected[row];
        } else {
            self.selected.fill(false);
            self.selected[row] = true;
        }
    }

    /// The example as a conversion at `index` sees it: through the ones
    /// before it (the end, for a new one), or as it is if they fail.
    pub fn example_at(&self, index: usize) -> String {
        self.value()
            .convert_upto(&self.example, Some(index))
            .unwrap_or_else(|_| self.example.clone())
    }

    /// "add"'s conversion editor: the last conversion used, else "append
    /// extra text", with the example through all of them.
    pub fn adding(&self, last_used: Option<&Conversion>) -> ConversionEditor {
        let conversion = last_used
            .cloned()
            .unwrap_or_else(|| Conversion::Append("extra text".to_owned()));
        ConversionEditor::new(&conversion, self.example_at(self.conversions.len()))
    }

    /// "edit"'s: the first selected, with its index.
    pub fn editing(&self) -> Option<(usize, ConversionEditor)> {
        let index = self.selected.iter().position(|&s| s)?;
        Some((
            index,
            ConversionEditor::new(&self.conversions[index], self.example_at(index)),
        ))
    }

    /// A conversion added at the end, selected.
    pub fn add(&mut self, conversion: Conversion) {
        self.conversions.push(conversion);
        self.selected.fill(false);
        self.selected.push(true);
    }

    pub fn replace(&mut self, index: usize, conversion: Conversion) {
        if let Some(slot) = self.conversions.get_mut(index) {
            *slot = conversion;
        }
    }

    /// "delete"'s question, if anything is selected.
    pub fn delete_question(&self) -> Option<&'static str> {
        self.selected
            .contains(&true)
            .then_some("Delete all selected?")
    }

    /// The selected conversions removed ("yes" to the question).
    pub fn delete(&mut self) {
        let mut kept = self.selected.iter().map(|&s| !s);
        self.conversions.retain(|_| kept.next().unwrap_or(true));
        self.selected.retain(|&s| !s);
    }

    fn single(&self) -> Option<usize> {
        let selected = self.selected();
        (selected.len() == 1).then(|| selected[0])
    }

    pub fn can_move_up(&self) -> bool {
        self.single().is_some_and(|i| i > 0)
    }

    pub fn can_move_down(&self) -> bool {
        self.single()
            .is_some_and(|i| i + 1 < self.conversions.len())
    }

    /// The one selected swapped with the one above (-1) or below (1).
    pub fn move_selected(&mut self, distance: isize) {
        let Some(index) = self.single() else {
            return;
        };
        let Some(other) = index
            .checked_add_signed(distance)
            .filter(|&o| o < self.conversions.len())
        else {
            return;
        };
        self.conversions.swap(index, other);
        self.selected.swap(index, other);
    }
}

/// Which of a conversion editor's rows show, and their labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionShown {
    /// The text row's label ("text to append: "), if it shows.
    pub text: Option<&'static str>,
    /// The number row's label ("characters to remove: "), if it shows.
    pub number: Option<&'static str>,
    pub encoding: bool,
    pub decoding: bool,
    pub regex: bool,
    /// The link to the date phrases.
    pub date_link: bool,
    pub timezone_decode: bool,
    pub timezone_offset: bool,
    pub timezone_encode: bool,
    pub hash: bool,
    /// The easy date parser's explanation.
    pub dateparser: bool,
}

/// The easy date parser's explanation.
pub const DATEPARSER_NOTE: &str = "This will parse pretty much any normal looking date into a timestamp that hydrus understands, timezone conversions included, with zero setup! It can even do \"2 hours ago\"!";

/// A conversion's editor (`EditStringConverterPanel._ConversionPanel`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionEditor {
    /// An index into [`CONVERSION_TYPES`].
    pub kind: usize,
    pub text: String,
    pub number: i64,
    /// The number box's least value: as the type last shown set it.
    number_min: i64,
    /// Indices into [`ENCODINGS`].
    pub encoding: usize,
    pub decoding: usize,
    pub pattern: String,
    pub replacement: String,
    /// Indices into [`TIMEZONES`].
    pub timezone_decode: usize,
    pub timezone_offset: i64,
    pub timezone_encode: usize,
    /// An index into [`HASH_FUNCTIONS`].
    pub hash: usize,
    /// The example it converts (read only).
    pub example: String,
}

fn code_index(code: i64) -> usize {
    CONVERSION_TYPES
        .iter()
        .position(|(c, _)| *c == code)
        .unwrap_or(0)
}

fn encoding_index(encoding: Encoding) -> usize {
    ENCODINGS
        .iter()
        .position(|(e, _)| *e == encoding)
        .unwrap_or(0)
}

fn count_of(n: i64) -> usize {
    usize::try_from(n).unwrap_or(0)
}

impl ConversionEditor {
    pub fn new(conversion: &Conversion, example: String) -> Self {
        let mut editor = Self {
            kind: 0,
            text: String::new(),
            // (its box starts at 1, with a least value of 0)
            number: 1,
            number_min: 0,
            encoding: 0,
            decoding: 0,
            pattern: String::new(),
            replacement: String::new(),
            timezone_decode: 0,
            timezone_offset: 0,
            timezone_encode: 0,
            hash: 0,
            example,
        };
        // (numbers are set while the box's least value is still 0)
        let number = |n: i64| n.max(0);
        editor.kind = match conversion {
            Conversion::RemoveFromStart(n) => {
                editor.number = number(i64::try_from(*n).unwrap_or(i64::MAX));
                code_index(0)
            }
            Conversion::RemoveFromEnd(n) => {
                editor.number = number(i64::try_from(*n).unwrap_or(i64::MAX));
                code_index(1)
            }
            Conversion::KeepStart(n) => {
                editor.number = number(i64::try_from(*n).unwrap_or(i64::MAX));
                code_index(6)
            }
            Conversion::KeepEnd(n) => {
                editor.number = number(i64::try_from(*n).unwrap_or(i64::MAX));
                code_index(7)
            }
            Conversion::Prepend(t) => {
                editor.text.clone_from(t);
                code_index(2)
            }
            Conversion::Append(t) => {
                editor.text.clone_from(t);
                code_index(3)
            }
            Conversion::AppendRandom { population, count } => {
                editor.text.clone_from(population);
                editor.number = number(i64::try_from(*count).unwrap_or(i64::MAX));
                code_index(15)
            }
            Conversion::Encode(e) => {
                editor.encoding = encoding_index(*e);
                code_index(4)
            }
            Conversion::Decode(e) => {
                editor.decoding = encoding_index(*e);
                code_index(5)
            }
            Conversion::Reverse => code_index(8),
            Conversion::RegexSub {
                pattern,
                replacement,
            } => {
                pattern.pattern().clone_into(&mut editor.pattern);
                editor.replacement.clone_from(replacement);
                code_index(9)
            }
            Conversion::IntegerAddition(n) => {
                editor.number = number(*n);
                code_index(11)
            }
            Conversion::Hash(h) => {
                editor.hash = HASH_FUNCTIONS.iter().position(|(f, _)| f == h).unwrap_or(0);
                code_index(13)
            }
            Conversion::DateDecode {
                phrase,
                timezone,
                offset,
            } => {
                editor.text.clone_from(phrase);
                editor.timezone_decode = count_of(timezone.code());
                editor.timezone_offset = *offset;
                code_index(10)
            }
            Conversion::DateEncode { phrase, timezone } => {
                editor.text.clone_from(phrase);
                editor.timezone_encode = count_of(timezone.code());
                code_index(12)
            }
            Conversion::DateParse => code_index(14),
            Conversion::Unsupported { code, data } => {
                let data = hydrus_core::pyjson::PyJson::parse(data).ok();
                let item = |i: usize| {
                    data.as_ref()
                        .and_then(|d| d.as_list())
                        .and_then(|l| l.get(i))
                };
                let text = |i: usize| item(i).and_then(PyJson::as_str).unwrap_or("").to_owned();
                let int = |i: usize| item(i).and_then(PyJson::as_i64).unwrap_or(0);
                match code {
                    10 => {
                        editor.text = text(0);
                        editor.timezone_decode = count_of(int(1));
                        editor.timezone_offset = int(2);
                    }
                    12 => {
                        editor.text = text(0);
                        editor.timezone_encode = count_of(int(1));
                    }
                    _ => {}
                }
                code_index(*code)
            }
        };
        editor.update_number_min();
        editor
    }

    /// A type chosen.
    pub fn set_kind(&mut self, kind: usize) {
        self.kind = kind.min(CONVERSION_TYPES.len() - 1);
        self.update_number_min();
    }

    fn code(&self) -> i64 {
        CONVERSION_TYPES[self.kind].0
    }

    /// The number box's least value as the type sets it (append random
    /// text 1, integer addition -65535, the counts 0, others leave it),
    /// the number raised to it.
    fn update_number_min(&mut self) {
        match self.code() {
            15 => self.number_min = 1,
            11 => self.number_min = -65535,
            0 | 1 | 6 | 7 => self.number_min = 0,
            _ => {}
        }
        self.number = self.number.max(self.number_min);
    }

    /// A number typed (held to the box's least value).
    pub fn set_number(&mut self, number: i64) {
        self.number = number.clamp(self.number_min, 65535);
    }

    pub fn shown(&self) -> ConversionShown {
        let code = self.code();
        let mut shown = ConversionShown {
            text: None,
            number: None,
            encoding: code == 4,
            decoding: code == 5,
            regex: code == 9,
            date_link: matches!(code, 10 | 12),
            timezone_decode: code == 10,
            timezone_offset: code == 10 && self.timezone_decode == 2,
            timezone_encode: code == 12,
            hash: code == 13,
            dateparser: code == 14,
        };
        match code {
            15 => {
                shown.text = Some("population");
                shown.number = Some("number of characters");
            }
            2 => shown.text = Some("text to prepend: "),
            3 => shown.text = Some("text to append: "),
            10 => shown.text = Some("date decode phrase: "),
            12 => shown.text = Some("date encode phrase: "),
            0 | 1 => shown.number = Some("characters to remove: "),
            6 | 7 => shown.number = Some("characters to take: "),
            11 => shown.number = Some("number to add: "),
            _ => {}
        }
        shown
    }

    /// The conversion as the boxes say (`GetValue`).
    pub fn value(&self) -> Conversion {
        let count = count_of(self.number);
        let unsupported = |code: i64, data: serde_json::Value| Conversion::Unsupported {
            code,
            data: hydrus_core::pyjson::PyJson::parse(&data.to_string())
                .map_or_else(|_| data.to_string(), |d| d.to_python_string()),
        };
        match self.code() {
            0 => Conversion::RemoveFromStart(count),
            1 => Conversion::RemoveFromEnd(count),
            6 => Conversion::KeepStart(count),
            7 => Conversion::KeepEnd(count),
            2 => Conversion::Prepend(self.text.clone()),
            3 => Conversion::Append(self.text.clone()),
            15 => Conversion::AppendRandom {
                population: self.text.clone(),
                count,
            },
            4 => Conversion::Encode(ENCODINGS[self.encoding].0),
            5 => Conversion::Decode(ENCODINGS[self.decoding].0),
            8 => Conversion::Reverse,
            9 => Conversion::RegexSub {
                pattern: PyRegex::new(self.pattern.clone()),
                replacement: self.replacement.clone(),
            },
            11 => Conversion::IntegerAddition(self.number),
            13 => Conversion::Hash(HASH_FUNCTIONS[self.hash].0),
            10 => Conversion::DateDecode {
                phrase: self.text.clone(),
                timezone: DateTimezone::from_code(i64::try_from(self.timezone_decode).unwrap_or(0))
                    .unwrap_or(DateTimezone::Utc),
                offset: self.timezone_offset,
            },
            12 => Conversion::DateEncode {
                phrase: self.text.clone(),
                timezone: DateTimezone::from_code(i64::try_from(self.timezone_encode).unwrap_or(0))
                    .unwrap_or(DateTimezone::Utc),
            },
            14 => Conversion::DateParse,
            code => unsupported(code, serde_json::Value::Null),
        }
    }

    /// The example converted ("converted string"), or why not.
    pub fn result(&self) -> String {
        StringConverter {
            conversions: vec![self.value()],
            example: self.example.clone(),
        }
        .convert_upto(&self.example, None)
        .unwrap_or_else(|e| e)
    }

    /// What "ok" asks first: a regex that looks like it captures a group,
    /// with nothing to replace it with.
    pub fn ok_question(&self) -> Option<&'static str> {
        if self.code() != 9 || !self.replacement.is_empty() {
            return None;
        }
        // (`(?<!\\)\((?!\?:|[=!>])`, matched at the start)
        let rest = self.pattern.strip_prefix('(')?;
        let other = rest.starts_with("?:") || rest.starts_with(['=', '!', '>']);
        (!other).then_some(
            "Are you sure you want this? It looks like you are matching a group, but the regex \"replacement\" input is empty.",
        )
    }
}

/// A tag filter step's explanation, over its filter button.
pub const TAG_FILTER_MESSAGE: &str = "This works the same as any tag filter elsewhere in the program. Note that it converts your texts to valid hydrus tags, so everything is coming out lowercase with trimmed whitespace, and invalid tags will never pass.";

/// `StringTagFilter.Test`: why `text` doesn't pass ("\"x\" was not a valid
/// tag!", "\"x\" did not pass the tag filter!").
pub fn tag_filter_test(
    filter: &hydrus_core::tag_filter::TagFilter,
    text: &str,
) -> Result<(), String> {
    let Some(tag) = hydrus_core::tag::clean_tag_checked(text) else {
        return Err(format!("\"{text}\" was not a valid tag!"));
    };
    if filter.tag_ok(&tag, true) {
        Ok(())
    } else {
        Err(format!("\"{text}\" did not pass the tag filter!"))
    }
}

/// A tag filter step's editor (`EditStringTagFilterPanel`): the filter,
/// and an example (the first test string, if any) and whether it passes.
#[derive(Debug, Clone, PartialEq)]
pub struct TagFilterStepEditor {
    pub filter: hydrus_core::tag_filter::TagFilter,
    pub example: String,
}

impl TagFilterStepEditor {
    pub fn new(step: &TagFilterStep, example: Option<String>) -> Self {
        Self {
            filter: step.filter.clone(),
            example: example.unwrap_or_default(),
        }
    }

    /// The filter button's label.
    pub fn filter_label(&self) -> String {
        crate::tag_filter_editor::button_label(&self.filter, false, "", false).0
    }

    /// The tag filter button's tooltip: the whole text its label may elide.
    pub fn filter_tooltip(&self) -> String {
        crate::tag_filter_editor::button_label(&self.filter, false, "", false).1
    }

    /// "Example matches ok!", or "Example does not match - " and why.
    pub fn test_result(&self) -> (String, bool) {
        match tag_filter_test(&self.filter, &self.example) {
            Ok(()) => ("Example matches ok!".to_owned(), true),
            Err(reason) => (format!("Example does not match - {reason}"), false),
        }
    }

    /// "ok": the step, or why not.
    pub fn value(&self) -> Result<TagFilterStep, String> {
        tag_filter_test(&self.filter, &self.example)
            .map(|()| TagFilterStep {
                filter: self.filter.clone(),
                example: self.example.clone(),
            })
            .map_err(|_| "Please enter an example text that matches the given rules!".to_owned())
    }
}
