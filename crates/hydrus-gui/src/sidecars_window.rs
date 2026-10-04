//! The sidecar editors, bound (`ui/sidecars.slint`): the routers list,
//! a router's editor and a source's or destination's editor, each over
//! hydrus-gui-model's [`sidecar_editors`](crate::sidecar_editors), its
//! questions asked in its own panel, "apply" handing back what was edited.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::ServiceKey;
use hydrus_core::url::strings::StringProcessor;
use hydrus_parse::sidecar::{Exporter, Importer, Router, TagDisplay};
use hydrus_store::Store;

use crate::list_selection::ListSelection;
use crate::sidecar_editors::{
    self as editors, CANVAS_CHOICES, Context, ExporterEditor, ImporterEditor, Kind, NodeBoxes,
    SEPARATOR_CHOICES, Shown, TAG_DISPLAY_CHOICES, TimestampDetail, change_type_choices,
};
use crate::sidecars::{exporter_text, importer_text, router_text};
use crate::{SidecarNodeWindow, SidecarRouterWindow, SidecarRoutersWindow, TableRow};

/// The windows while they are open.
#[derive(Clone, Default)]
pub struct Slots {
    pub routers: Rc<RefCell<Option<SidecarRoutersWindow>>>,
    /// Scoped router clipboard and PNG children.
    pub exchange: crate::downloader_interchange_window::Slots,
    pub router: Rc<RefCell<Option<SidecarRouterWindow>>>,
    pub node: Rc<RefCell<Option<SidecarNodeWindow>>>,
    /// The string processor editor, from a router or source.
    pub strings: crate::string_processor_window::Slots,
    /// Reusable JSON formula editor for sidecar sources.
    pub formula: crate::formula_window::Slots,
    /// Owner-supplied file paths or media results for the reusable test panel.
    pub test_objects: Rc<RefCell<Vec<editors::TestObject>>>,
}
impl Slots {
    /// Force-close a router owner and invalidate every retained descendant.
    pub fn cancel(&self) {
        let owner = self
            .routers
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(owner) = owner {
            owner.invoke_cancel();
        } else {
            let router = self
                .router
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            if let Some(router) = router {
                router.invoke_cancel();
            }
            let node = self
                .node
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            if let Some(node) = node {
                node.invoke_cancel();
            }
            self.strings.cancel_all();
            self.formula.cancel();
            self.exchange.cancel();
        }
    }
    /// Supply the same bounded examples to the router and its source children.
    pub fn set_test_objects(&self, mut objects: Vec<editors::TestObject>) {
        objects.truncate(editors::TEST_OBJECT_LIMIT);
        *self.test_objects.borrow_mut() = objects;
    }
}

impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slots")
            .field("routers", &self.routers.borrow().is_some())
            .field("router", &self.router.borrow().is_some())
            .field("node", &self.node.borrow().is_some())
            .field("strings", &self.strings)
            .field("formula", &self.formula)
            .finish()
    }
}

fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = items.into_iter().map(Into::into).collect();
    ModelRc::new(VecModel::from(items))
}

fn rows(labels: Vec<String>, selection: &ListSelection<usize>) -> ModelRc<TableRow> {
    let rows: Vec<TableRow> = labels
        .into_iter()
        .enumerate()
        .map(|(i, label)| TableRow {
            cells: strings([label]),
            selected: selection.is_selected(i),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

fn index(i: usize) -> i32 {
    i32::try_from(i).unwrap_or(0)
}

fn usize_of(i: i32) -> usize {
    usize::try_from(i).unwrap_or(0)
}

/// A service's name by its key in hex.
pub(crate) fn namer(store: &Store) -> impl Fn(&str) -> Option<String> {
    let snapshot = store.snapshot();
    move |key: &str| {
        hex::decode(key)
            .ok()
            .and_then(|k| snapshot.services.by_key(&ServiceKey::new(k)).ok())
            .map(|s| s.name.clone())
    }
}

/// What a panel asks, with what its answers do.
struct Question {
    title: String,
    message: String,
    choices: Vec<String>,
}

impl Question {
    fn info(message: &str) -> Self {
        Self {
            title: "information".into(),
            message: message.into(),
            choices: vec!["ok".into()],
        }
    }

    fn yes_no(message: &str) -> Self {
        Self {
            title: "Are you sure?".into(),
            message: message.into(),
            choices: vec!["yes".into(), "no".into()],
        }
    }
}

macro_rules! show_question {
    ($window:expr, $question:expr) => {{
        let window = &$window;
        match $question {
            Some(q) => {
                window.set_asking(true);
                window.set_asking_title(q.title.as_str().into());
                window.set_asking_message(q.message.as_str().into());
                window.set_asking_choices(strings(q.choices.iter().cloned()));
            }
            None => window.set_asking(false),
        }
    }};
}

/// "Which type?" for these kinds: the question, and the kinds by answer.
fn type_question(
    allowed: &[Kind],
    current: Option<Kind>,
    destination: bool,
) -> Option<(Question, Vec<Kind>)> {
    let choices = match current {
        Some(current) => change_type_choices(allowed, current, destination)?,
        None => allowed
            .iter()
            .map(|k| (k.label(), k.description(destination), *k))
            .collect(),
    };
    let message = if destination {
        editors::DESTINATION_TYPE_QUESTION
    } else {
        editors::SOURCE_TYPE_QUESTION
    };
    let described: Vec<String> = choices
        .iter()
        .map(|(label, description, _)| format!("{label}: {description}"))
        .collect();
    Some((
        Question {
            title: editors::TYPE_TITLE.into(),
            message: format!("{message}\n\n{}", described.join("\n")),
            choices: choices.iter().map(|(l, _, _)| (*l).to_owned()).collect(),
        },
        choices.into_iter().map(|(_, _, k)| k).collect(),
    ))
}

// ---- a source's or destination's editor ----

/// What a node window edits.
#[derive(Debug, Clone)]
pub enum Node {
    Source(Importer),
    Destination(Exporter),
}

enum Editing {
    Source(ImporterEditor),
    Destination(ExporterEditor),
}

impl Editing {
    fn boxes(&self) -> &NodeBoxes {
        match self {
            Editing::Source(e) => &e.boxes,
            Editing::Destination(e) => &e.boxes,
        }
    }

    fn boxes_mut(&mut self) -> &mut NodeBoxes {
        match self {
            Editing::Source(e) => &mut e.boxes,
            Editing::Destination(e) => &mut e.boxes,
        }
    }

    fn shown(&self) -> Shown {
        match self {
            Editing::Source(e) => e.shown(),
            Editing::Destination(e) => e.shown(),
        }
    }

    fn destination(&self) -> bool {
        matches!(self, Editing::Destination(_))
    }
}

enum NodeAsking {
    Type(Vec<Kind>),
    Service(Vec<(String, ServiceKey)>),
    Info,
}

struct NodeState {
    editing: Editing,
    context: Context,
    asking: Option<(NodeAsking, Question)>,
}

fn show_node(window: &SidecarNodeWindow, state: &NodeState, store: &Store) {
    let editing = &state.editing;
    let boxes = editing.boxes();
    let shown = editing.shown();
    window.set_type_label(boxes.kind.label().into());
    window.set_show_naming(shown.naming);
    window.set_show_separator(shown.separator);
    window.set_show_timestamp(shown.timestamp);
    window.set_show_tags(shown.tags);
    window.set_show_display(!editing.destination());
    window.set_show_json(shown.json_formula);
    window.set_show_forced(shown.forced_name);
    window.set_show_nested(shown.nested);
    window.set_show_processing(!editing.destination());
    window.set_show_sidecar_help(editing.destination() && shown.naming);
    let naming = &boxes.naming;
    window.set_remove_ext(naming.naming.remove_actual_filename_ext);
    window.set_suffix(naming.naming.suffix.as_str().into());
    window.set_example(naming.example.as_str().into());
    window.set_result(naming.result().into());
    window.set_converter_label(naming.converter_label().into());
    window.set_separators(strings(SEPARATOR_CHOICES.iter().map(|&s| s.to_owned())));
    window.set_separator(index(boxes.separator.choice));
    window.set_custom_separator(boxes.separator.custom.as_str().into());
    let name = namer(store)(&boxes.service_key).unwrap_or_else(|| "unknown service".into());
    window.set_service_label(name.into());
    window.set_displays(strings(TAG_DISPLAY_CHOICES.iter().map(|&s| s.to_owned())));
    let t = &boxes.timestamp;
    window.set_timestamp_kinds(strings(t.kind_labels()));
    window.set_timestamp_kind(index(t.kind));
    window.set_detail(match t.detail() {
        TimestampDetail::None => 0,
        TimestampDetail::FileService => 1,
        TimestampDetail::DeletedService => 2,
        TimestampDetail::Canvas => 3,
        TimestampDetail::Domain => 4,
    });
    window.set_file_services(strings(t.file_services.iter().map(|(_, n)| n.clone())));
    window.set_file_service(index(t.file_service));
    window.set_deleted_services(strings(t.deleted_services.iter().map(|(_, n)| n.clone())));
    window.set_deleted_service(index(t.deleted_service));
    window.set_canvases(strings(CANVAS_CHOICES.iter().map(|&s| s.to_owned())));
    window.set_canvas(index(t.canvas));
    window.set_domain(t.domain.as_str().into());
    match editing {
        Editing::Source(e) => {
            window.set_display(i32::from(e.display == TagDisplay::DisplayActual));
            window.set_processing_note(editors::SOURCE_PROCESSING_NOTE.into());
            window.set_processing(e.processor.button_label().into());
        }
        Editing::Destination(e) => {
            window.set_forced_on(e.forced_name.is_some());
            window.set_forced_name(e.forced_name.clone().unwrap_or_default().into());
            window.set_nested(e.nested.join("\n").into());
        }
    }
    show_question!(window, state.asking.as_ref().map(|(_, q)| q));
}

/// Take in what was typed or chosen.
fn read_node(window: &SidecarNodeWindow, state: &mut NodeState) {
    let boxes = state.editing.boxes_mut();
    boxes.naming.naming.remove_actual_filename_ext = window.get_remove_ext();
    boxes.naming.naming.suffix = window.get_suffix().to_string();
    boxes.naming.example = window.get_example().to_string();
    boxes.separator.choice = usize_of(window.get_separator());
    boxes.separator.custom = window.get_custom_separator().to_string();
    let t = &mut boxes.timestamp;
    t.kind = usize_of(window.get_timestamp_kind());
    t.file_service = usize_of(window.get_file_service());
    t.deleted_service = usize_of(window.get_deleted_service());
    t.canvas = usize_of(window.get_canvas());
    t.domain = window.get_domain().to_string();
    match &mut state.editing {
        Editing::Source(e) => {
            e.display = if window.get_display() == 1 {
                TagDisplay::DisplayActual
            } else {
                TagDisplay::Storage
            };
        }
        Editing::Destination(e) => {
            e.forced_name = window
                .get_forced_on()
                .then(|| window.get_forced_name().to_string());
            e.nested = window
                .get_nested()
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect();
        }
    }
}

/// Edit a source or destination; "apply" gives it to `done`.
pub fn open_node(
    store: &Arc<Store>,
    context: Context,
    node: &Node,
    slots: &Slots,
    done: Rc<dyn Fn(Node)>,
) -> Result<SidecarNodeWindow, slint::PlatformError> {
    slots.strings.set_store(store);
    let slot = &slots.node;
    let window = SidecarNodeWindow::new()?;
    let active = Rc::new(Cell::new(true));
    let snapshot = store.snapshot();
    let services = &snapshot.services;
    let editing = match node {
        Node::Source(importer) => Editing::Source(ImporterEditor::new(importer, services)),
        Node::Destination(exporter) => {
            window.set_window_title("edit metadata migration destination".into());
            Editing::Destination(ExporterEditor::new(exporter, services))
        }
    };
    if !editing.destination() {
        window.set_window_title(editors::SOURCES_TITLE.into());
    }
    // (the reference warns when a destination's tag service is gone)
    let warning = match &editing {
        Editing::Destination(e) if e.shown().tags && !e.service_exists(services) => Some((
            NodeAsking::Info,
            Question::info(
                "Hey, the tag service for your exporter does not seem to exist! Maybe it was deleted. Please select a new one that does.",
            ),
        )),
        _ => None,
    };
    let state = Rc::new(RefCell::new(NodeState {
        editing,
        context,
        asking: warning,
    }));
    let refresh: Rc<dyn Fn()> = {
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        Rc::new(move || {
            if let Some(window) = weak.upgrade() {
                show_node(&window, &state.borrow(), &store);
            }
        })
    };
    refresh();
    window.on_changed({
        let weak = window.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                read_node(&window, &mut state.borrow_mut());
            }
            refresh();
        }
    });
    window.on_edit_formula({
        let state = state.clone();
        let refresh = refresh.clone();
        let store = store.clone();
        let objects = slots.test_objects.clone();
        let slots = slots.formula.clone();
        let active = active.clone();
        move || {
            if !active.get() {
                return;
            }
            if slots.formula.borrow().is_some() {
                return;
            }
            let (formula, examples) = match &state.borrow().editing {
                Editing::Source(e) => (
                    e.formula.clone(),
                    objects.borrow().first().map_or_else(Vec::new, |object| {
                        e.value().map_or_else(
                            |error| vec![error.into()],
                            |importer| {
                                editors::test_importer_strings(&store, &importer, object, true)
                            },
                        )
                    }),
                ),
                Editing::Destination(_) => return,
            };
            let applied = Rc::new({
                let state = state.clone();
                let refresh = refresh.clone();
                let active = active.clone();
                move |formula| {
                    if !active.get() {
                        return;
                    }
                    if let Editing::Source(e) = &mut state.borrow_mut().editing {
                        e.formula = formula;
                    }
                    refresh();
                }
            });
            match crate::formula_window::open(
                &store,
                &formula,
                crate::formula_window::FormulaTestData {
                    collapse_newlines: false,
                    text: examples.first().cloned().unwrap_or_default(),
                    examples,
                    ..crate::formula_window::FormulaTestData::default()
                },
                &slots,
                applied,
            ) {
                Ok(w) => {
                    w.set_allow_type_change(false);
                    *slots.formula.borrow_mut() = Some(w);
                }
                Err(e) => eprintln!("could not open JSON formula: {e}"),
            }
        }
    });
    // the sidecar filename's conversion, in the string converter editor,
    // with the sidecar path it would convert
    window.on_edit_converter({
        let state = state.clone();
        let refresh = refresh.clone();
        let strings = slots.strings.clone();
        move || {
            if strings.converter.borrow().is_some() {
                return;
            }
            let (converter, example) = {
                let state = state.borrow();
                let naming = &state.editing.boxes().naming;
                let unconverted = hydrus_parse::sidecar::SidecarNaming {
                    filename_converter: hydrus_core::url::strings::StringConverter::default(),
                    ..naming.naming.clone()
                };
                (
                    naming.naming.filename_converter.clone(),
                    unconverted.path(&naming.example, naming.extension),
                )
            };
            let applied: Rc<dyn Fn(hydrus_core::url::strings::StringConverter)> = Rc::new({
                let state = state.clone();
                let refresh = refresh.clone();
                move |converter| {
                    state
                        .borrow_mut()
                        .editing
                        .boxes_mut()
                        .naming
                        .naming
                        .filename_converter = converter;
                    refresh();
                }
            });
            match crate::string_processor_window::open_converter(
                &converter,
                Some(example),
                &strings,
                applied,
            ) {
                Ok(w) => *strings.converter.borrow_mut() = Some(w),
                Err(e) => eprintln!("could not open the string converter: {e}"),
            }
        }
    });
    // a source's processing, in the string processor editor
    window.on_edit_processing({
        let state = state.clone();
        let refresh = refresh.clone();
        let store = store.clone();
        let strings = slots.strings.clone();
        let objects = slots.test_objects.clone();
        let active = active.clone();
        move || {
            if !active.get() {
                return;
            }
            let Editing::Source(e) = &state.borrow().editing else {
                return;
            };
            if strings.processor.borrow().is_some() {
                return;
            }
            let applied: Rc<dyn Fn(StringProcessor)> = Rc::new({
                let state = state.clone();
                let refresh = refresh.clone();
                let active = active.clone();
                move |processor| {
                    if !active.get() {
                        return;
                    }
                    if let Editing::Source(e) = &mut state.borrow_mut().editing {
                        e.processor = processor;
                    }
                    refresh();
                }
            });
            match crate::string_processor_window::open(
                &store,
                &e.processor,
                objects.borrow().first().map_or_else(Vec::new, |object| {
                    e.value().map_or_else(
                        |error| vec![error.into()],
                        |importer| editors::test_importer_strings(&store, &importer, object, true),
                    )
                }),
                &strings,
                applied,
            ) {
                Ok(w) => *strings.processor.borrow_mut() = Some(w),
                Err(e) => eprintln!("could not open the string processor: {e}"),
            }
        }
    });
    window.on_change_type({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            {
                let mut s = state.borrow_mut();
                let destination = s.editing.destination();
                let allowed = if destination {
                    s.context.exporters()
                } else {
                    s.context.importers()
                };
                s.asking = Some(
                    match type_question(allowed, Some(s.editing.boxes().kind), destination) {
                        Some((q, kinds)) => (NodeAsking::Type(kinds), q),
                        None => (NodeAsking::Info, Question::info(editors::ONLY_ONE)),
                    },
                );
            }
            refresh();
        }
    });
    window.on_choose_service({
        let state = state.clone();
        let refresh = refresh.clone();
        let store = store.clone();
        move || {
            {
                let snapshot = store.snapshot();
                let mut s = state.borrow_mut();
                let choices = match &s.editing {
                    Editing::Source(e) => e.service_choices(&snapshot.services),
                    Editing::Destination(e) => e.service_choices(&snapshot.services),
                };
                match choices.as_slice() {
                    [] => {}
                    [(_, only)] => s.editing.boxes_mut().service_key = hex::encode(only.as_bytes()),
                    _ => {
                        let q = Question {
                            title: editors::SELECT_SERVICE.into(),
                            message: String::new(),
                            choices: choices.iter().map(|(n, _)| n.clone()).collect(),
                        };
                        s.asking = Some((NodeAsking::Service(choices), q));
                    }
                }
            }
            refresh();
        }
    });
    window.on_sidecar_help({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            state.borrow_mut().asking =
                Some((NodeAsking::Info, Question::info(editors::SIDECAR_HELP)));
            refresh();
        }
    });
    let answer = {
        let state = state.clone();
        let refresh = refresh.clone();
        let store = store.clone();
        move |chosen: Option<usize>| {
            {
                let mut s = state.borrow_mut();
                let asking = s.asking.take();
                match (asking, chosen) {
                    (Some((NodeAsking::Type(kinds), _)), Some(i)) => {
                        if let Some(&kind) = kinds.get(i) {
                            let snapshot = store.snapshot();
                            match &mut s.editing {
                                Editing::Source(e) => e.change_type(kind, &snapshot.services),
                                Editing::Destination(e) => e.change_type(kind, &snapshot.services),
                            }
                        }
                    }
                    (Some((NodeAsking::Service(choices), _)), Some(i)) => {
                        if let Some((_, key)) = choices.get(i) {
                            s.editing.boxes_mut().service_key = hex::encode(key.as_bytes());
                        }
                    }
                    _ => {}
                }
            }
            refresh();
        }
    };
    window.on_chosen({
        let answer = answer.clone();
        move |i| answer(usize::try_from(i).ok())
    });
    window.on_cancelled(move || answer(None));
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        let formula = slots.formula.clone();
        let strings = slots.strings.clone();
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            formula.cancel();
            strings.cancel_all();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
            if let Some(window) = weak.upgrade() {
                window.invoke_closed();
            }
        }
    };
    window.on_apply({
        let weak = window.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        let close = close.clone();
        let active = active.clone();
        move || {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            let value = {
                let mut s = state.borrow_mut();
                read_node(&window, &mut s);
                match &s.editing {
                    Editing::Source(e) => e.value().map(Node::Source),
                    Editing::Destination(e) => e.value().map(Node::Destination),
                }
            };
            match value {
                Ok(node) => {
                    close();
                    done(node);
                }
                Err(error) => {
                    state.borrow_mut().asking = Some((NodeAsking::Info, Question::info(error)));
                    refresh();
                }
            }
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show()?;
    Ok(window)
}

// ---- a router's editor ----

enum RouterAsking {
    /// The kind of a source to add.
    NewSource(Vec<Kind>),
    Remove(Vec<usize>),
    /// "ok"'s questions still to answer.
    Ok(Vec<&'static str>),
}

struct RouterState {
    router: Router,
    test_objects: Vec<editors::TestObject>,
    test_source: usize,
    selection: ListSelection<usize>,
    asking: Option<(RouterAsking, Question)>,
}

fn show_router(window: &SidecarRouterWindow, state: &mut RouterState, store: &Store) {
    state.test_source = state
        .test_source
        .min(state.router.importers.len().saturating_sub(1));
    let namer = namer(store);
    let labels: Vec<String> = state
        .router
        .importers
        .iter()
        .map(|i| importer_text(i, &namer))
        .collect();
    let order: Vec<usize> = (0..labels.len()).collect();
    let selected = state.selection.in_order(&order).len();
    window.set_sources(rows(labels, &state.selection));
    window.set_one_selected(selected == 1);
    window.set_any_selected(selected > 0);
    window.set_processing_note(editors::ROUTER_PROCESSING_NOTE.into());
    window.set_processing(state.router.processor.button_label().into());
    window.set_destination(exporter_text(&state.router.exporter, &namer).into());
    window.set_test_sources(strings(
        (0..state.router.importers.len()).map(|i| format!("source {}", i + 1)),
    ));
    window.set_test_source(index(state.test_source));
    window.set_test_rows(ModelRc::new(VecModel::from(
        editors::router_test_rows(store, &state.router, state.test_source, &state.test_objects)
            .into_iter()
            .map(|row| TableRow {
                cells: strings(row),
                selected: false,
            })
            .collect::<Vec<_>>(),
    )));
    window.set_test_note(
        if state.router.importers.is_empty() {
            "Add a source and this will show test data."
        } else if state.test_objects.is_empty() {
            "No example files were supplied by the owner."
        } else {
            ""
        }
        .into(),
    );
    show_question!(window, state.asking.as_ref().map(|(_, q)| q));
}

/// Edit a router; "apply" (once "ok"'s questions are answered yes) gives
/// it to `done`.
pub fn open_router(
    store: &Arc<Store>,
    context: Context,
    router: Router,
    slots: &Slots,
    done: Rc<dyn Fn(Router)>,
) -> Result<SidecarRouterWindow, slint::PlatformError> {
    let window = SidecarRouterWindow::new()?;
    let active = Rc::new(Cell::new(true));
    window.set_window_title(editors::ROUTER_TITLE.into());
    let state = Rc::new(RefCell::new(RouterState {
        router,
        test_objects: slots.test_objects.borrow().clone(),
        test_source: 0,
        selection: ListSelection::default(),
        asking: None,
    }));
    let refresh: Rc<dyn Fn()> = {
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let owned = slots.clone();
        Rc::new(move || {
            if let Some(window) = weak.upgrade() {
                window.set_child_open(
                    owned.node.borrow().is_some()
                        || owned.strings.has_open()
                        || owned.formula.formula.borrow().is_some()
                        || owned.formula.strings.has_open(),
                );
                show_router(&window, &mut state.borrow_mut(), &store);
            }
        })
    };
    refresh();
    window.on_test_source_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        let weak = window.as_weak();
        let active = active.clone();
        move || {
            if !active.get() {
                return;
            }
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().test_source = usize_of(window.get_test_source());
            }
            refresh();
        }
    });
    // a source edited in its own window: `at` replaces one, else it is added
    let edit_source = {
        let state = state.clone();
        let refresh = refresh.clone();
        let store = store.clone();
        let slots = slots.clone();
        let active = active.clone();
        Rc::new(move |importer: Importer, at: Option<usize>| {
            if !active.get() {
                return;
            }
            let done: Rc<dyn Fn(Node)> = {
                let state = state.clone();
                let refresh = refresh.clone();
                let active = active.clone();
                Rc::new(move |node| {
                    if !active.get() {
                        return;
                    }
                    if let Node::Source(importer) = node {
                        let mut s = state.borrow_mut();
                        match at {
                            Some(i) if i < s.router.importers.len() => {
                                s.router.importers[i] = importer;
                            }
                            _ => s.router.importers.push(importer),
                        }
                    }
                    refresh();
                })
            };
            match open_node(&store, context, &Node::Source(importer), &slots, done) {
                Ok(w) => {
                    let updated = refresh.clone();
                    w.on_closed(move || updated());
                    *slots.node.borrow_mut() = Some(w);
                    refresh();
                }
                Err(e) => eprintln!("could not open the source: {e}"),
            }
        })
    };
    window.on_row_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        move |row, ctrl, shift| {
            if let Ok(row) = usize::try_from(row) {
                let mut s = state.borrow_mut();
                let order: Vec<usize> = (0..s.router.importers.len()).collect();
                s.selection.click(&order, row, ctrl, shift);
            }
            refresh();
        }
    });
    let edit_selected = {
        let state = state.clone();
        let edit_source = edit_source.clone();
        Rc::new(move |row: Option<usize>| {
            let picked = {
                let s = state.borrow();
                let order: Vec<usize> = (0..s.router.importers.len()).collect();
                row.or_else(|| s.selection.in_order(&order).first().copied())
                    .and_then(|i| s.router.importers.get(i).cloned().map(|imp| (i, imp)))
            };
            if let Some((i, importer)) = picked {
                edit_source(importer, Some(i));
            }
        })
    };
    window.on_row_activated({
        let edit_selected = edit_selected.clone();
        move |row| edit_selected(usize::try_from(row).ok())
    });
    window.on_edit(move || edit_selected(None));
    window.on_add({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            if let Some((q, kinds)) = type_question(context.importers(), None, false) {
                state.borrow_mut().asking = Some((RouterAsking::NewSource(kinds), q));
            }
            refresh();
        }
    });
    window.on_delete({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            {
                let mut s = state.borrow_mut();
                let order: Vec<usize> = (0..s.router.importers.len()).collect();
                let selected = s.selection.in_order(&order);
                if !selected.is_empty() {
                    let q = Question::yes_no(&editors::remove_question(selected.len()));
                    s.asking = Some((RouterAsking::Remove(selected), q));
                }
            }
            refresh();
        }
    });
    window.on_edit_destination({
        let state = state.clone();
        let refresh = refresh.clone();
        let store = store.clone();
        let slots = slots.clone();
        let active = active.clone();
        move || {
            if !active.get() {
                return;
            }
            let exporter = state.borrow().router.exporter.clone();
            let done: Rc<dyn Fn(Node)> = {
                let state = state.clone();
                let refresh = refresh.clone();
                let active = active.clone();
                Rc::new(move |node| {
                    if !active.get() {
                        return;
                    }
                    if let Node::Destination(exporter) = node {
                        state.borrow_mut().router.exporter = exporter;
                    }
                    refresh();
                })
            };
            match open_node(&store, context, &Node::Destination(exporter), &slots, done) {
                Ok(w) => {
                    let updated = refresh.clone();
                    w.on_closed(move || updated());
                    *slots.node.borrow_mut() = Some(w);
                    refresh();
                }
                Err(e) => eprintln!("could not open the destination: {e}"),
            }
        }
    });
    // the router's processing, in the string processor editor
    window.on_edit_processing({
        let state = state.clone();
        let refresh = refresh.clone();
        let store = store.clone();
        let strings = slots.strings.clone();
        let active = active.clone();
        move || {
            if !active.get() || strings.processor.borrow().is_some() {
                return;
            }
            let processor = state.borrow().router.processor.clone();
            let applied: Rc<dyn Fn(StringProcessor)> = Rc::new({
                let state = state.clone();
                let refresh = refresh.clone();
                let active = active.clone();
                move |processor| {
                    if !active.get() {
                        return;
                    }
                    state.borrow_mut().router.processor = processor;
                    refresh();
                }
            });
            let examples = {
                let state = state.borrow();
                editors::router_test_strings(&store, &state.router, &state.test_objects)
            };
            match crate::string_processor_window::open(
                &store, &processor, examples, &strings, applied,
            ) {
                Ok(w) => {
                    let updated = refresh.clone();
                    w.on_closed(move || updated());
                    *strings.processor.borrow_mut() = Some(w);
                    refresh();
                }
                Err(e) => eprintln!("could not open the string processor: {e}"),
            }
        }
    });
    let close = {
        let weak = window.as_weak();
        let slot = slots.router.clone();
        let node = slots.node.clone();
        let active = active.clone();
        let strings = slots.strings.clone();
        let formula = slots.formula.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            let child = node.borrow_mut().take();
            if let Some(child) = child {
                child.invoke_cancel();
            }
            strings.cancel_all();
            formula.cancel();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
            if let Some(window) = weak.upgrade() {
                window.invoke_closed();
            }
        }
    };
    // "ok"'s next question, or done
    let ok = {
        let state = state.clone();
        let refresh = refresh.clone();
        let close = close.clone();
        let active = active.clone();
        Rc::new(move |mut left: Vec<&'static str>| {
            if !active.get() {
                return;
            }
            if left.is_empty() {
                let router = state.borrow().router.clone();
                close();
                done(router);
                return;
            }
            let q = Question::yes_no(left.remove(0));
            state.borrow_mut().asking = Some((RouterAsking::Ok(left), q));
            refresh();
        })
    };
    window.on_apply({
        let state = state.clone();
        let ok = ok.clone();
        let slots = slots.clone();
        move || {
            if slots.node.borrow().is_some()
                || slots.strings.has_open()
                || slots.formula.formula.borrow().is_some()
                || slots.formula.strings.has_open()
            {
                return;
            }
            let questions = editors::ok_questions(&state.borrow().router);
            ok(questions);
        }
    });
    let answer = {
        let state = state.clone();
        let refresh = refresh.clone();
        let edit_source = edit_source.clone();
        move |chosen: Option<usize>| {
            let asking = state.borrow_mut().asking.take();
            match (asking, chosen) {
                (Some((RouterAsking::NewSource(kinds), _)), Some(i)) => {
                    if let Some(&kind) = kinds.get(i) {
                        refresh();
                        edit_source(
                            editors::new_importer(
                                kind,
                                hydrus_core::url::strings::StringProcessor::default(),
                            ),
                            None,
                        );
                        return;
                    }
                }
                (Some((RouterAsking::Remove(rows), _)), Some(0)) => {
                    let mut s = state.borrow_mut();
                    let mut rows = rows;
                    rows.sort_unstable();
                    for i in rows.into_iter().rev() {
                        if i < s.router.importers.len() {
                            s.router.importers.remove(i);
                        }
                    }
                    s.selection = ListSelection::default();
                }
                (Some((RouterAsking::Ok(left), _)), Some(0)) => {
                    ok(left);
                    return;
                }
                _ => {}
            }
            refresh();
        }
    };
    window.on_chosen({
        let answer = answer.clone();
        move |i| answer(usize::try_from(i).ok())
    });
    window.on_cancelled(move || answer(None));
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show()?;
    Ok(window)
}

// ---- the routers list ----

struct RoutersState {
    routers: Vec<Router>,
    selection: ListSelection<usize>,
    asking: Option<(Vec<usize>, Question)>,
    templates: Vec<(&'static str, &'static str, Vec<Router>)>,
}

fn show_routers(window: &SidecarRoutersWindow, state: &RoutersState, store: &Store) {
    let namer = namer(store);
    let labels: Vec<String> = state
        .routers
        .iter()
        .map(|r| router_text(r, true, &namer))
        .collect();
    let order: Vec<usize> = (0..labels.len()).collect();
    let selected = state.selection.in_order(&order).len();
    window.set_rows(rows(labels, &state.selection));
    window.set_one_selected(selected == 1);
    window.set_any_selected(selected > 0);
    window.set_templates(strings(
        state.templates.iter().map(|(l, _, _)| (*l).to_owned()),
    ));
    show_question!(window, state.asking.as_ref().map(|(_, q)| q));
}

/// Edit a list of routers (an import's or an export's); "apply" gives them
/// to `applied`.
pub fn open_routers(
    store: &Arc<Store>,
    context: Context,
    routers: Vec<Router>,
    slots: &Slots,
    applied: Rc<dyn Fn(Vec<Router>)>,
) -> Result<SidecarRoutersWindow, slint::PlatformError> {
    if let Some(window) = slots.routers.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let window = SidecarRoutersWindow::new()?;
    window.set_window_title(editors::ROUTERS_TITLE.into());
    let active = Rc::new(std::cell::Cell::new(true));
    let state = Rc::new(RefCell::new(RoutersState {
        routers,
        selection: ListSelection::default(),
        asking: None,
        templates: editors::templates(context, &store.snapshot().services),
    }));
    let refresh: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let slots = slots.clone();
        let active = active.clone();
        move || {
            if active.get()
                && let Some(window) = weak.upgrade()
            {
                show_routers(&window, &state.borrow(), &store);
                window.set_child_open(
                    slots.router.borrow().is_some()
                        || slots.node.borrow().is_some()
                        || slots.exchange.has_open(),
                );
            }
        }
    });
    let blocked: Rc<dyn Fn() -> bool> = Rc::new({
        let active = active.clone();
        let state = state.clone();
        let slots = slots.clone();
        move || {
            !active.get()
                || state.borrow().asking.is_some()
                || slots.router.borrow().is_some()
                || slots.node.borrow().is_some()
                || slots.exchange.has_open()
        }
    });
    let edit_router = Rc::new({
        let state = state.clone();
        let refresh = refresh.clone();
        let store = store.clone();
        let slots = slots.clone();
        let active = active.clone();
        let blocked = blocked.clone();
        let weak = window.as_weak();
        move |router: Router, at: Option<usize>| {
            if blocked() {
                return;
            }
            let original = router.clone();
            let done: Rc<dyn Fn(Router)> = Rc::new({
                let state = state.clone();
                let refresh = refresh.clone();
                let active = active.clone();
                let weak = weak.clone();
                move |router| {
                    if !active.get() {
                        return;
                    }
                    let mut s = state.borrow_mut();
                    if let Some(i) = at {
                        if s.routers.get(i) != Some(&original) {
                            if let Some(window) = weak.upgrade() {
                                window.set_error(
                                    "The router changed while its editor was open.".into(),
                                );
                            }
                            return;
                        }
                        s.routers[i] = router;
                    } else {
                        s.routers.push(router);
                    }
                    drop(s);
                    refresh();
                }
            });
            match open_router(&store, context, router, &slots, done) {
                Ok(w) => {
                    let refresh = refresh.clone();
                    w.on_closed(move || refresh());
                    *slots.router.borrow_mut() = Some(w);
                }
                Err(e) => {
                    if let Some(window) = weak.upgrade() {
                        window.set_error(e.to_string().into());
                    }
                }
            }
            refresh();
        }
    });
    window.on_row_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |row, ctrl, shift| {
            if blocked() {
                return;
            }
            if let Ok(row) = usize::try_from(row) {
                let mut s = state.borrow_mut();
                let order = (0..s.routers.len()).collect::<Vec<_>>();
                s.selection.click(&order, row, ctrl, shift);
            }
            refresh();
        }
    });
    let edit_selected = Rc::new({
        let state = state.clone();
        let edit_router = edit_router.clone();
        let blocked = blocked.clone();
        move |row: Option<usize>| {
            if blocked() {
                return;
            }
            let picked = {
                let s = state.borrow();
                let order = (0..s.routers.len()).collect::<Vec<_>>();
                row.or_else(|| {
                    s.selection
                        .one()
                        .or_else(|| s.selection.in_order(&order).first().copied())
                })
                .and_then(|i| s.routers.get(i).cloned().map(|router| (i, router)))
            };
            if let Some((i, router)) = picked {
                edit_router(router, Some(i));
            }
        }
    });
    window.on_row_activated({
        let edit_selected = edit_selected.clone();
        move |row| edit_selected(usize::try_from(row).ok())
    });
    window.on_edit(move || edit_selected(None));
    window.on_add(move || edit_router(editors::new_router(context), None));
    window.on_delete({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                return;
            }
            let mut s = state.borrow_mut();
            let order = (0..s.routers.len()).collect::<Vec<_>>();
            let selected = s.selection.in_order(&order);
            if !selected.is_empty() {
                s.asking = Some((
                    selected.clone(),
                    Question::yes_no(&editors::remove_question(selected.len())),
                ));
            }
            drop(s);
            refresh();
        }
    });
    window.on_template({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |i| {
            if blocked() {
                return;
            }
            let mut s = state.borrow_mut();
            let added = usize::try_from(i)
                .ok()
                .and_then(|i| s.templates.get(i))
                .map(|(_, _, routers)| routers.clone())
                .unwrap_or_default();
            s.routers.extend(added);
            drop(s);
            refresh();
        }
    });
    window.on_duplicate({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                return;
            }
            let mut s = state.borrow_mut();
            let order = (0..s.routers.len()).collect::<Vec<_>>();
            let mut selected = s.selection.in_order(&order);
            let copies = selected
                .iter()
                .map(|i| s.routers[*i].clone())
                .collect::<Vec<_>>();
            let first = s.routers.len();
            s.routers.extend(copies);
            selected.extend(first..s.routers.len());
            s.selection.select_many(&selected);
            drop(s);
            refresh();
        }
    });
    window.on_exchange({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let slots = slots.clone();
        let active = active.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |importing| {
            if blocked() {
                return;
            }
            let routers = {
                let s = state.borrow();
                let order = (0..s.routers.len()).collect::<Vec<_>>();
                s.selection.in_order(&order).into_iter()
                    .map(|i| s.routers[i].clone()).collect::<Vec<_>>()
            };
            if !importing && routers.is_empty() {
                return;
            }
            let preview = Rc::new({
                let store = store.clone();
                move |routers: Vec<Router>| {
                    editors::validate_router_import(context, &routers).map_err(|error| {
                        format!("The imported objects were wrong for this control:\n\n{error}")
                    })?;
                    let descriptions = routers.iter()
                        .map(|router| router_text(router, true, &namer(&store)))
                        .collect::<Vec<_>>().join("\n");
                    Ok(format!("Add {} metadata routers:\n{descriptions}\nChanges are saved only when you apply the owning editor.", routers.len()))
                }
            });
            let applied = Rc::new({
                let state = state.clone();
                let active = active.clone();
                let refresh = refresh.clone();
                move |routers: Vec<Router>| {
                    if !active.get() {
                        return Ok(());
                    }
                    editors::validate_router_import(context, &routers).map_err(|error| {
                        format!("The imported objects were wrong for this control:\n\n{error}")
                    })?;
                    let mut s = state.borrow_mut();
                    let order = (0..s.routers.len()).collect::<Vec<_>>();
                    let mut selected = s.selection.in_order(&order);
                    let first = s.routers.len();
                    s.routers.extend(routers);
                    selected.extend(first..s.routers.len());
                    s.selection.select_many(&selected);
                    drop(s);
                    refresh();
                    Ok(())
                }
            });
            match crate::downloader_interchange_window::open_routers(
                &slots.exchange, importing, routers, preview, applied,
            ) {
                Ok(child) => {
                    let refresh = refresh.clone();
                    child.on_closed(move || refresh());
                }
                Err(error) => {
                    if let Some(window) = weak.upgrade() {
                        window.set_error(error.into());
                    }
                }
            }
            refresh();
        }
    });
    let answer = Rc::new({
        let state = state.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        move |chosen: Option<usize>| {
            if !active.get() {
                return;
            }
            let mut s = state.borrow_mut();
            if let (Some((mut rows, _)), Some(0)) = (s.asking.take(), chosen) {
                rows.sort_unstable();
                for i in rows.into_iter().rev() {
                    if i < s.routers.len() {
                        s.routers.remove(i);
                    }
                }
                s.selection = ListSelection::default();
            }
            drop(s);
            refresh();
        }
    });
    window.on_chosen({
        let answer = answer.clone();
        move |i| answer(usize::try_from(i).ok())
    });
    window.on_cancelled(move || answer(None));
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slots = slots.clone();
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            let node = slots.node.borrow_mut().take();
            if let Some(node) = node {
                node.invoke_cancel();
            }
            let router = slots.router.borrow_mut().take();
            if let Some(router) = router {
                router.invoke_cancel();
            }
            slots.exchange.cancel();
            slots.strings.cancel_all();
            slots.formula.cancel();
            slots.routers.borrow_mut().take();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
        }
    });
    window.on_apply({
        let state = state.clone();
        let close = close.clone();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                return;
            }
            let routers = state.borrow().routers.clone();
            close();
            applied(routers);
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    refresh();
    window.show()?;
    Ok(window)
}
