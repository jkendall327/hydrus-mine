//! The string processor editor and its splitter, joiner, sorter and
//! selector/slicer editors, replayed step by step against the reference's
//! (`oracle/fixtures/string_processor_editor.json`, from
//! `oracle/record_string_processor_editor.py`): the list's rows and
//! selection, the processed strings, the single example and its tabs,
//! what each step's editor is given and shows, and what "ok" gives.

use hydrus_core::url::strings::{ProcessingStep, SortKind, StringProcessor};
use hydrus_gui_model::string_editors::{
    ADD_CHOICES, ADD_TITLE, JoinerEditor, ProcessorEditor, SORT_TYPES, STEP_TITLE, SlicerEditor,
    SorterEditor, SplitterEditor, StepKind, kind_of,
};
use hydrus_legacy::objects::domain::string_processor;
use hydrus_legacy::serialisable::SerialisableObject;
use serde_json::{Value, json};

fn processor(value: &Value) -> StringProcessor {
    string_processor(&SerialisableObject::from_tuple_str(&value.to_string()).unwrap()).unwrap()
}

fn strings(value: &Value) -> Vec<String> {
    serde_json::from_value(value.clone()).unwrap()
}

/// A step's editor, as the model has it.
enum Editor {
    Splitter(SplitterEditor),
    Joiner(JoinerEditor),
    Slicer(SlicerEditor),
    Sorter(SorterEditor),
}

impl Editor {
    fn open(editor: &ProcessorEditor, step: &ProcessingStep) -> Option<Self> {
        Some(match step {
            ProcessingStep::Split {
                separator,
                max_splits,
            } => Editor::Splitter(SplitterEditor::new(
                separator,
                *max_splits,
                editor.example_text_for(step),
            )),
            ProcessingStep::Join { joiner, tuple_size } => Editor::Joiner(JoinerEditor::new(
                joiner,
                *tuple_size,
                editor.example_texts_for(step),
            )),
            ProcessingStep::Slice { start, end } => Editor::Slicer(SlicerEditor::new(
                *start,
                *end,
                editor.example_texts_for(step),
            )),
            ProcessingStep::Sort {
                kind,
                ascending,
                regex,
            } => Editor::Sorter(SorterEditor::new(
                *kind,
                *ascending,
                regex
                    .as_ref()
                    .map(hydrus_core::url::strings::PyRegex::pattern),
                editor.example_texts_for(step),
            )),
            _ => return None,
        })
    }

    fn state(&self) -> Value {
        match self {
            Editor::Splitter(e) => {
                let (results, invalid) = e.results();
                json!({"separator": e.separator, "max_splits": e.max_splits, "example": e.example,
                    "results": results, "invalid": invalid})
            }
            Editor::Joiner(e) => {
                let (results, invalid) = e.results();
                json!({"joiner": e.joiner, "tuple": e.tuple_size, "summary": e.summary(),
                    "texts": e.texts, "results": results, "invalid": invalid})
            }
            Editor::Slicer(e) => json!({
                "select": if e.select_one { "single" } else { "range" },
                "single": e.single, "start": e.start, "end": e.end, "summary": e.summary(),
                "texts": e.texts, "results": e.results(),
            }),
            Editor::Sorter(e) => {
                let name = SORT_TYPES.iter().find(|(k, _)| *k == e.kind).unwrap().1;
                json!({"sort_type": name, "asc": e.ascending, "regex": e.regex,
                    "texts": e.texts, "results": e.results()})
            }
        }
    }

    fn edit(&mut self, edits: &serde_json::Map<String, Value>) {
        for (key, v) in edits {
            match (&mut *self, key.as_str()) {
                (Editor::Splitter(e), "separator") => e.separator = v.as_str().unwrap().into(),
                (Editor::Splitter(e), "max_splits") => {
                    e.max_splits = v.as_u64().map(|n| usize::try_from(n).unwrap());
                }
                (Editor::Joiner(e), "joiner") => e.joiner = v.as_str().unwrap().into(),
                (Editor::Joiner(e), "tuple") => {
                    e.tuple_size = v.as_u64().map(|n| usize::try_from(n).unwrap());
                }
                (Editor::Slicer(e), "select") => e.select_one = v == "single",
                (Editor::Slicer(e), "single") => e.single = v.as_i64().unwrap(),
                (Editor::Slicer(e), "start") => e.start = v.as_i64(),
                (Editor::Slicer(e), "end") => e.end = v.as_i64(),
                (Editor::Sorter(e), "sort_type") => {
                    e.kind = SORT_TYPES.iter().find(|(_, n)| n == v).unwrap().0;
                }
                (Editor::Sorter(e), "asc") => e.ascending = v.as_bool().unwrap(),
                (Editor::Sorter(e), "regex") => e.regex = v.as_str().map(str::to_owned),
                (_, key) => panic!("no field {key}"),
            }
        }
    }

    fn value(&self) -> Result<ProcessingStep, String> {
        match self {
            Editor::Splitter(e) => e.value(),
            Editor::Joiner(e) => Ok(e.value()),
            Editor::Slicer(e) => Ok(e.value()),
            Editor::Sorter(e) => Ok(e.value()),
        }
    }
}

/// The reference's regex errors are Python's own words; ours say why too.
fn same_state(ours: &Value, recorded: &Value, context: &str) {
    let mut ours = ours.clone();
    if let (Some(results), Some(recorded)) = (
        ours.get_mut("results").and_then(Value::as_array_mut),
        recorded["results"].as_array(),
    ) && let ([ours_error], [Value::String(theirs)]) =
        (results.as_mut_slice(), recorded.as_slice())
        && theirs.starts_with("Error: missing")
    {
        assert!(
            ours_error.as_str().unwrap().starts_with("Error: "),
            "{context}"
        );
        *ours_error = Value::String(theirs.clone());
    }
    assert_eq!(&ours, recorded, "{context}");
}

/// A step's editor opened, edited (or cancelled) and "ok"ed, against the
/// recording; the step it gives, if any.
fn edit_step(
    editor: &ProcessorEditor,
    step: &ProcessingStep,
    edits: &Value,
    opened: &[Value],
    context: &str,
) -> Option<ProcessingStep> {
    let Some(mut step_editor) = Editor::open(editor, step) else {
        // (a match, tag filter or converter: stood in for, given back)
        let kind = match kind_of(step).unwrap() {
            StepKind::Match => "match",
            StepKind::TagFilter => "tag filter",
            _ => "converter",
        };
        let given = editor.example_text_for(step);
        let given = if kind == "converter" {
            json!(given)
        } else {
            json!([given])
        };
        assert_eq!(opened, [json!({"kind": kind, "given": given})], "{context}");
        return (!edits.is_null()).then(|| step.clone());
    };
    let record = &opened[0];
    same_state(&step_editor.state(), &record["opened"], context);
    let edits = edits.as_object()?;
    step_editor.edit(edits);
    same_state(&step_editor.state(), &record["edited"], context);
    match step_editor.value() {
        Ok(value) => {
            assert_eq!(value, processor(&record["value"]).steps[0], "{context}");
            Some(value)
        }
        Err(veto) => {
            assert_eq!(veto, record["veto"], "{context}");
            None
        }
    }
}

fn check(editor: &ProcessorEditor, state: &Value, context: &str) {
    assert_eq!(editor.rows(), strings(&state["rows"]), "{context}");
    assert_eq!(json!(editor.selected()), state["selected"], "{context}");
    assert_eq!(
        editor.processed(),
        strings(&state["processed"]),
        "{context}"
    );
    assert_eq!(editor.example(), state["example"], "{context}");
    assert_eq!(json!(editor.example_tabs()), state["tabs"], "{context}");
    assert_eq!(editor.value(), processor(&state["value"]), "{context}");
}

// leaf: audit-network-processor-order
// leaf: audit-network-processor-results
// leaf: string-joiner
// leaf: string-slicer
// leaf: string-sorter
// leaf: string-splitter
// leaf: string-conversion-tests
#[test]
fn the_string_processor_editor_works_as_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("string_processor_editor.json");
    let choices: Vec<Value> = ADD_CHOICES
        .iter()
        .map(|(_, label, description)| json!([label, description]))
        .collect();
    for (c, case) in recorded["cases"].as_array().unwrap().iter().enumerate() {
        let mut editor =
            ProcessorEditor::new(&processor(&case["processor"]), strings(&case["texts"]));
        let steps = case["steps"].as_array().unwrap();
        check(&editor, &steps[0]["state"], &format!("case {c} start"));
        for (s, step) in steps.iter().enumerate().skip(1) {
            let context = format!("case {c} step {s}: {}", step["do"]);
            let action = step["do"].as_array().unwrap();
            let said = step["said"].as_array().unwrap();
            let opened = step["opened"].as_array().unwrap();
            match action[0].as_str().unwrap() {
                "click" => editor.click(
                    usize::try_from(action[1].as_u64().unwrap()).unwrap(),
                    action[2].as_bool().unwrap(),
                ),
                "text" => editor.select_text(usize::try_from(action[1].as_u64().unwrap()).unwrap()),
                "example" => editor.set_example(action[1].as_str().unwrap().to_owned()),
                "up" => editor.move_selected(-1),
                "down" => editor.move_selected(1),
                "delete" => match editor.delete_question() {
                    Some(question) => {
                        assert_eq!(said, &[json!({"asked": question})], "{context}");
                        editor.delete();
                    }
                    None => assert!(said.is_empty(), "{context}"),
                },
                "add" => {
                    assert_eq!(said[0], json!({"select": ADD_TITLE, "choices": choices}));
                    if let Some(label) = action[1].as_str() {
                        assert_eq!(said[1], json!({"dialog": STEP_TITLE}), "{context}");
                        let kind = ADD_CHOICES.iter().find(|(_, l, _)| *l == label).unwrap().0;
                        let new = editor.new_step(kind);
                        if let Some(step) = edit_step(&editor, &new, &action[2], opened, &context) {
                            editor.add(step);
                        }
                    }
                }
                "edit" => {
                    assert_eq!(said, &[json!({"dialog": STEP_TITLE})], "{context}");
                    let (index, current) = editor.editing().unwrap();
                    let current = current.clone();
                    if let Some(step) = edit_step(&editor, &current, &action[1], opened, &context) {
                        editor.replace(index, step);
                    }
                }
                other => panic!("{other}"),
            }
            check(&editor, &step["state"], &context);
        }
    }
    // (the sorter's types are the reference's three)
    assert_eq!(
        SORT_TYPES.map(|(k, _)| k),
        [SortKind::Human, SortKind::Lexicographic, SortKind::Reverse]
    );
}

#[test]
fn processor_exchange_appends_reference_steps_and_rejects_invalid_data_atomically() {
    use hydrus_downloader_exchange::processing;
    let fixture = hydrus_testkit::fixture_json("processing_exchange.json");
    let mut editor = ProcessorEditor::new(
        &processor(&fixture["start"]["processor"]),
        vec!["a,b".into()],
    );
    editor.click(1, false);
    let single: Value =
        serde_json::from_str(&processing::encode_text(&editor.export_steps()).unwrap()).unwrap();
    assert_eq!(single, fixture["single"]);
    editor.click(0, true);
    let multiple: Value =
        serde_json::from_str(&processing::encode_text(&editor.export_steps()).unwrap()).unwrap();
    assert_eq!(multiple, fixture["multiple"]);
    assert_eq!(editor.import_text(&multiple.to_string()).unwrap(), 2);
    assert_eq!(editor.value(), processor(&fixture["imported"]["processor"]));
    assert_eq!(
        editor.value().process(vec!["a,b".into()]).unwrap(),
        fixture["imported"]["processed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    assert_eq!(json!(editor.selected()), fixture["imported"]["selected"]);
    let before = editor.clone();
    assert!(editor.import_text("not JSON").is_err());
    assert_eq!(editor.value(), before.value());
    assert_eq!(editor.selected(), before.selected());
}
