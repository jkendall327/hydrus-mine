//! The options window, bound to its editor ([`Editor`]): the page chosen's
//! rows shown, edits kept until "apply" writes them (and what couldn't be
//! set is said in a popup, as the reference says it), "cancel" forgetting
//! them.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use slint::{
    ComponentHandle as _, Model as _, ModelRc, SharedString, StandardListViewItem, VecModel,
};

use hydrus_store::Store;

use crate::options::{
    Editor, Kind, Row, SEARCH_PLACEHOLDER, SEARCH_SHOWN, Settings, TAG_SORT_GROUPS, TAG_SORT_TYPES,
    Unit, Value, duration_fields, tag_sort_orders,
};
use crate::{CheckerOptionsWindow, DurationField, OptionRow, OptionsWindow};

/// A time's fields as the window shows them.
fn fields(seconds: f64, units: &[Unit]) -> ModelRc<DurationField> {
    let fields: Vec<DurationField> = duration_fields(seconds, units)
        .into_iter()
        .zip(units)
        .map(|(value, unit)| DurationField {
            value: int(value),
            maximum: int(unit.max()),
            label: unit.label().into(),
        })
        .collect();
    ModelRc::new(VecModel::from(fields))
}

fn int(n: i64) -> i32 {
    i32::try_from(n).unwrap_or(if n < 0 { i32::MIN } else { i32::MAX })
}

/// A collect's choices for the files that match none of it, as the
/// reference's cog menu has them ("unmatched files").
type SortCogTarget = (usize, usize, Vec<hydrus_gui_model::sort_cog::Entry>);

const UNMATCHED: [&str; 2] = ["collect into one group", "leave separate"];

/// A row as the window shows it (a sort's types are the store's).
fn option_row(row: &Row<'_>, store: &Store, sessions: &[(Option<String>, String)]) -> OptionRow {
    let mut out = OptionRow::default();
    match row {
        Row::Title { title, depth } => {
            out.kind = 0;
            out.label = (*title).into();
            out.depth = int(*depth as i64);
        }
        Row::Opt {
            option,
            depth,
            value,
            number,
            enabled,
        } => {
            out.enabled = *enabled;
            out.label = option.label.into();
            out.depth = int(*depth as i64);
            match (&option.kind, value) {
                (Kind::Check, Value::Check(b)) => {
                    out.kind = 1;
                    out.checked = *b;
                }
                (Kind::Int { min, max }, Value::Int(n)) => {
                    out.kind = 2;
                    out.number = int(*n);
                    out.minimum = int(*min);
                    out.maximum = int(*max);
                }
                (
                    Kind::Noneable {
                        none_phrase,
                        min,
                        max,
                        unit,
                        ..
                    },
                    Value::Noneable(n),
                ) => {
                    out.kind = 3;
                    out.number = int(n.unwrap_or(*number));
                    out.minimum = int(*min);
                    out.maximum = int(*max);
                    out.is_none = n.is_none();
                    out.none_phrase = (*none_phrase).into();
                    out.unit = unit.unwrap_or_default().into();
                }
                (Kind::Float { .. }, Value::Float(text)) => {
                    out.kind = 4;
                    out.text = text.as_str().into();
                }
                (Kind::TagService { combined }, Value::TagService(service)) => {
                    let choices = crate::options::tag_service_choices(store, *combined);
                    out.kind = 15;
                    out.items = ModelRc::new(VecModel::from(
                        choices
                            .iter()
                            .map(|(_, name)| SharedString::from(name.as_str()))
                            .collect::<Vec<_>>(),
                    ));
                    out.index = choices
                        .iter()
                        .position(|(key, _)| key == service)
                        .map_or(-1, |index| int(index as i64));
                }
                (Kind::Choice(items), Value::Choice(i)) => {
                    out.kind = 5;
                    let items: Vec<SharedString> = items.iter().map(|&s| s.into()).collect();
                    out.items = ModelRc::new(VecModel::from(items));
                    out.index = int(*i as i64);
                }
                (Kind::SavedSession, Value::SavedSession(name)) => {
                    out.kind = 5;
                    out.items = ModelRc::new(VecModel::from(
                        sessions
                            .iter()
                            .map(|(_, label)| SharedString::from(label.as_str()))
                            .collect::<Vec<_>>(),
                    ));
                    out.index = sessions
                        .iter()
                        .position(|(value, _)| value == name)
                        .map_or(-1, |index| int(index as i64));
                }
                (Kind::Directory, Value::Text(text)) => {
                    out.kind = 20;
                    out.text = text.as_str().into();
                }
                (Kind::Text, Value::Text(text)) => {
                    out.kind = 6;
                    out.text = text.as_str().into();
                }
                (Kind::NoneableText { none_phrase }, Value::NoneableText { none, text }) => {
                    out.kind = 7;
                    out.text = text.as_str().into();
                    out.is_none = *none;
                    out.none_phrase = (*none_phrase).into();
                }
                (Kind::CanvasTicks, Value::Canvases(canvases)) => {
                    use hydrus_core::CanvasType;
                    out.kind = 23;
                    out.items = ModelRc::new(VecModel::from(vec![
                        "media views".into(),
                        "preview views".into(),
                        "client api views".into(),
                    ]));
                    out.checks = ModelRc::new(VecModel::from(
                        [
                            CanvasType::MediaViewer,
                            CanvasType::Preview,
                            CanvasType::ClientApi,
                        ]
                        .iter()
                        .map(|c| canvases.contains(c))
                        .collect::<Vec<_>>(),
                    ));
                }
                (
                    Kind::NoneableDuration {
                        units,
                        min,
                        none_phrase,
                        ..
                    },
                    Value::NoneableDuration { none, seconds },
                ) => {
                    out.kind = 24;
                    out.is_none = *none;
                    out.none_phrase = (*none_phrase).into();
                    out.minimum = int((min * 1000.0) as i64);
                    out.fields = fields(*seconds, units);
                }
                (Kind::Duration { units, .. }, Value::Duration(seconds)) => {
                    out.kind = 8;
                    out.fields = fields(*seconds, units);
                }
                (
                    Kind::Velocity {
                        number: (min, max),
                        per,
                        units,
                        ..
                    },
                    Value::Velocity(n, seconds),
                ) => {
                    out.kind = 9;
                    out.number = int(*n);
                    out.minimum = int(*min);
                    out.maximum = int(*max);
                    out.per = (*per).into();
                    out.fields = fields(*seconds, units);
                }
                (Kind::Sort, Value::Sort(sort)) => {
                    out.kind = 10;
                    out.sort_cog = !hydrus_gui_model::sort_cog::groups(sort).is_empty();
                    let choices = crate::sort::page_choices(store, &sort.by);
                    let names: Vec<SharedString> =
                        choices.iter().map(|c| c.name.as_str().into()).collect();
                    out.items = ModelRc::new(VecModel::from(names));
                    if let Some(i) = choices.iter().position(|c| c.by == sort.by) {
                        out.index = int(i as i64);
                        let orders: Vec<SharedString> =
                            choices[i].orders.iter().map(|&o| o.into()).collect();
                        out.orders = ModelRc::new(VecModel::from(orders));
                        out.order_index = i32::from(!sort.ascending);
                    }
                }
                (Kind::TagSort, Value::TagSort(sort)) => {
                    out.kind = 12;
                    let strings = |items: &[&str]| {
                        ModelRc::new(VecModel::from(
                            items
                                .iter()
                                .map(|&s| s.into())
                                .collect::<Vec<SharedString>>(),
                        ))
                    };
                    let types: Vec<&str> = TAG_SORT_TYPES.iter().map(|(name, _)| *name).collect();
                    out.items = strings(&types);
                    out.index = int(TAG_SORT_TYPES
                        .iter()
                        .position(|(_, t)| *t == sort.sort_type)
                        .unwrap_or(0) as i64);
                    let (orders, order) = tag_sort_orders(sort);
                    out.orders = strings(&orders);
                    out.order_index = int(order as i64);
                    let groups: Vec<&str> = TAG_SORT_GROUPS.iter().map(|(name, _)| *name).collect();
                    out.groups = strings(&groups);
                    out.group_index = int(TAG_SORT_GROUPS
                        .iter()
                        .position(|(_, g)| *g == sort.group_by)
                        .unwrap_or(0) as i64);
                    // (as the reference's, a subtag sort doesn't group)
                    out.grouped = sort.sort_type != hydrus_core::tag_sort::TagSortType::Subtag;
                }
                (Kind::LocalLocation, Value::Location(location)) => {
                    out.kind = 16;
                    out.text =
                        crate::domains::location_label(&store.snapshot().services, location).into();
                }
                (Kind::FavouriteTags, Value::FavouriteTags(_)) => {
                    out.kind = 17;
                    out.text = "edit favourite tags".into();
                }
                (Kind::RelatedWeights, Value::RelatedWeights(_)) => {
                    out.kind = 29;
                }
                (Kind::MostUsedTags, Value::MostUsedTags(_)) => {
                    out.kind = 28;
                }
                (Kind::GallerySource, Value::GallerySource(current)) => {
                    out.kind = 19;
                    let downloaders: hydrus_parse::Downloaders =
                        store.read(hydrus_store::settings::get).unwrap_or_default();
                    out.text = hydrus_gui_model::gallery_source::caption(
                        &downloaders.gugs,
                        current.as_ref(),
                    )
                    .into();
                }
                (Kind::ProviderOrder, Value::ProviderOrder(_)) => {
                    out.kind = 22;
                }
                (Kind::TagBanner(_), Value::TagBanner(value)) => {
                    out.kind = 27;
                    let presentation = store.read(hydrus_store::settings::get).unwrap_or_default();
                    out.text = hydrus_gui_model::tag_banner::Editor::new(value, presentation)
                        .preview()
                        .into();
                }
                (Kind::NamespaceSorts, Value::NamespaceSorts(_)) => {
                    out.kind = 21;
                    out.text = "edit namespace sorting schemes".into();
                }
                (Kind::ImportOptions, Value::ImportOptions(_)) => {
                    out.kind = 18;
                    out.text = "edit import options".into();
                }
                (Kind::DeletionReasons, Value::DeletionReasons(_)) => {
                    out.kind = 25;
                }
                (Kind::FrameLocations, Value::FrameLocations(_)) => {
                    out.kind = 26;
                }
                (Kind::RegexFavourites, Value::RegexFavourites(_)) => {
                    out.kind = 14;
                    out.text = "edit regex favourites".into();
                }
                (Kind::Checker, Value::Checker(_)) => {
                    out.kind = 13;
                    out.text = "checker options".into();
                }
                (Kind::Collect, Value::Collect(collect)) => {
                    out.kind = 11;
                    let choices = crate::collect::choices(store);
                    out.text = crate::collect::label(&choices, collect).into();
                    let names: Vec<SharedString> =
                        choices.iter().map(|c| c.name.as_str().into()).collect();
                    out.items = ModelRc::new(VecModel::from(names));
                    let checks: Vec<bool> = choices.iter().map(|c| c.checked(collect)).collect();
                    out.checks = ModelRc::new(VecModel::from(checks));
                    out.orders = ModelRc::new(VecModel::from(
                        UNMATCHED
                            .iter()
                            .map(|&s| s.into())
                            .collect::<Vec<SharedString>>(),
                    ));
                    out.order_index = i32::from(!collect.collect_unmatched);
                }
                _ => {}
            }
        }
    }
    out
}

/// Open the window on the store's settings; it forgets itself from `slot`
/// when closed, and calls `applied` once changes are written.
#[allow(clippy::too_many_arguments)] // Explicit parent-owned child slots preserve cancellation and inspection.
pub(crate) fn open(
    store: &Arc<Store>,
    slot: &Rc<RefCell<Option<OptionsWindow>>>,
    checker_slot: &Rc<RefCell<Option<CheckerOptionsWindow>>>,
    reason_slot: &crate::options_deletion::Slot,
    frame_slot: &crate::options_frames::Slot,
    banner_slot: &crate::tag_banner_window::Slot,
    suggested_slot: &crate::tag_suggestions_window::Slots,
    applied: Rc<dyn Fn()>,
) -> Result<OptionsWindow, String> {
    let settings = store
        .read(Settings::load)
        .map_err(|e| format!("could not read the options: {e}"))?;
    let window = OptionsWindow::new().map_err(|e| e.to_string())?;
    let session_choices = Rc::new(crate::options::session_choices(store));
    window.set_search_at_top(settings.options_preferences.search_at_top);
    let resolved = settings
        .search_defaults
        .resolved_local_location(&store.snapshot().services);
    let mut editor = Editor::new(settings);
    editor.set_local_location(resolved);
    editor.resolve_tag_services(store);
    let downloaders: hydrus_parse::Downloaders =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    let current = hydrus_gui_model::gallery_source::resolve(
        &downloaders.gugs,
        editor.edited_gallery_source(),
    );
    editor.set_gallery_source(current);
    let editor = Rc::new(RefCell::new(editor));
    let regex_slot: crate::regex_favourites_window::Slot = Rc::default();
    let gallery_slot: crate::gallery_source_window::Slot = Rc::default();
    let location_slot: Rc<RefCell<Option<crate::LocationsWindow>>> = Rc::default();
    let tag_slot: crate::write_tag_window::Slot = Rc::default();
    let import_slot: crate::import_options_panel_window::Slot = Rc::default();
    let namespace_slot: crate::namespace_sorts_window::Slot = Rc::default();
    let active = Rc::new(Cell::new(true));
    let cog_target: Rc<RefCell<Option<SortCogTarget>>> = Rc::default();
    let names: Vec<StandardListViewItem> = editor
        .borrow()
        .page_names()
        .into_iter()
        .map(StandardListViewItem::from)
        .collect();
    window.set_pages(ModelRc::new(VecModel::from(names)));
    let show_providers = crate::options_palette::bind(&window, &editor, &active);
    let reason_queue = crate::options_deletion::bind(&window, &editor, &active, reason_slot);
    let frame_table = crate::options_frames::bind(&window, &editor, &active, frame_slot);
    // (the rows are made anew only as the page changes: an edit leaves its
    // control as the user left it)
    let show_page = {
        let cog_target = cog_target.clone();
        let session_choices = session_choices.clone();
        let editor = editor.clone();
        let store = store.clone();
        let weak = window.as_weak();
        move || {
            cog_target.borrow_mut().take();
            let Some(window) = weak.upgrade() else { return };
            let editor = editor.borrow();
            let rows: Vec<OptionRow> = editor
                .rows()
                .iter()
                .enumerate()
                .map(|(i, row)| OptionRow {
                    found: editor.found(i),
                    ..option_row(row, &store, &session_choices)
                })
                .collect();
            window.set_page(int(editor.page() as i64));
            window.set_rows(ModelRc::new(VecModel::from(rows)));
            show_providers();
            if let Some(name) = editor.remembered_panel()
                && let Err(error) = store.write(move |ctx| {
                    let mut preferences: hydrus_store::settings::OptionsPreferences =
                        hydrus_store::settings::get(ctx.conn())?;
                    if preferences.last_panel != name {
                        name.clone_into(&mut preferences.last_panel);
                        hydrus_store::settings::set(ctx.conn(), &preferences)?;
                    }
                    Ok(())
                })
            {
                eprintln!("could not remember the options page: {error}");
            }
        }
    };
    show_page();
    crate::sidebar_context_cog::bind_options(&window, &editor, &store, &active, show_page.clone());
    (reason_queue.show)();
    (frame_table.show)();
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        let import_slot = import_slot.clone();
        let namespace_slot = namespace_slot.clone();
        let banner_slot = banner_slot.clone();
        let suggested_slot = suggested_slot.clone();
        let regex_slot = regex_slot.clone();
        let gallery_slot = gallery_slot.clone();
        let location_slot = location_slot.clone();
        let tag_slot = tag_slot.clone();
        let active = active.clone();
        let cancel_reasons = reason_queue.cancel.clone();
        let cancel_frames = frame_table.cancel.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            let child = tag_slot
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            if let Some(child) = child {
                child.invoke_cancel();
            }
            cancel_reasons();
            cancel_frames();
            crate::import_options_panel_window::cancel(&import_slot);
            crate::namespace_sorts_window::cancel(&namespace_slot);
            crate::tag_banner_window::cancel(&banner_slot);
            crate::tag_suggestions_window::cancel(&suggested_slot);
            crate::locations_window::cancel(&location_slot);
            crate::regex_favourites_window::cancel(&regex_slot);
            crate::gallery_source_window::cancel(&gallery_slot);
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    };
    let at = |i: i32| usize::try_from(i).unwrap_or(usize::MAX);
    // the search: suggestions as it is typed in; one chosen shows its page,
    // its row highlighted, and the search is cleared (as the reference's)
    window.set_search_placeholder(SEARCH_PLACEHOLDER.into());
    window.set_shown(int(SEARCH_SHOWN as i64));
    let matches: Rc<RefCell<Vec<crate::options::Suggestion>>> = Rc::default();
    window.on_search_edited({
        let editor = editor.clone();
        let matches = matches.clone();
        let weak = window.as_weak();
        move |text| {
            let Some(window) = weak.upgrade() else { return };
            let found: Vec<crate::options::Suggestion> =
                editor.borrow().search(&text).into_iter().cloned().collect();
            let texts: Vec<SharedString> = found.iter().map(|s| s.text.as_str().into()).collect();
            *matches.borrow_mut() = found;
            window.set_matches(ModelRc::new(VecModel::from(texts)));
            window.set_match_highlighted(-1);
        }
    });
    window.on_move_match({
        let matches = matches.clone();
        let weak = window.as_weak();
        move |by| {
            let Some(window) = weak.upgrade() else { return };
            let count = int(matches.borrow().len() as i64);
            if count == 0 {
                return;
            }
            let at = (window.get_match_highlighted() + by).clamp(0, count - 1);
            window.set_match_highlighted(at);
        }
    });
    window.on_search_chosen({
        let editor = editor.clone();
        let matches = matches.clone();
        let show_page = show_page.clone();
        let weak = window.as_weak();
        move |i| {
            let chosen = usize::try_from(i)
                .ok()
                .and_then(|i| matches.borrow().get(i).cloned());
            let Some(chosen) = chosen else {
                return;
            };
            editor.borrow_mut().go_to(&chosen);
            show_page();
            matches.borrow_mut().clear();
            if let Some(window) = weak.upgrade() {
                window.set_search_text(SharedString::new());
                window.set_matches(ModelRc::default());
                window.set_match_highlighted(-1);
            }
        }
    });
    window.on_local_location_clicked({
        let editor = editor.clone();
        let store = store.clone();
        let location_slot = location_slot.clone();
        let show_page = show_page.clone();
        move |row| {
            let current = match editor.borrow().rows().get(at(row)) {
                Some(Row::Opt {
                    value: Value::Location(location),
                    ..
                }) => location.clone(),
                _ => return,
            };
            let chosen = Rc::new({
                let editor = editor.clone();
                let show_page = show_page.clone();
                move |location| {
                    editor.borrow_mut().set_local_location(location);
                    show_page();
                }
            });
            if let Err(error) = crate::locations_window::open_importable(
                &location_slot,
                store.clone(),
                &current,
                chosen,
            ) {
                eprintln!("could not open default local location: {error}");
            }
        }
    });
    window.on_favourite_tags_clicked({
        let editor = editor.clone();
        let store = store.clone();
        let tag_slot = tag_slot.clone();
        let import_slot = import_slot.clone();
        let import_slot = import_slot.clone();
        let active = active.clone();
        let show_page = show_page.clone();
        move || {
            if !active.get() || tag_slot.borrow().is_some() || import_slot.borrow().is_some() {
                return;
            }
            let initial = editor.borrow().edited_favourite_tags();
            let accepted = Rc::new({
                let editor = editor.clone();
                let active = active.clone();
                let show_page = show_page.clone();
                move |tags: Vec<String>| {
                    if active.get() {
                        editor.borrow_mut().set_favourite_tags(&tags);
                        show_page();
                    }
                }
            });
            if let Err(error) =
                crate::write_tag_window::open_favourites(&store, &initial.0, &tag_slot, accepted)
            {
                eprintln!("could not open favourite tags: {error}");
            }
        }
    });
    window.on_related_weights_clicked({
        let slots = suggested_slot.clone();
        let editor = editor.clone();
        let active = active.clone();
        let show_page = show_page.clone();
        move || {
            if !active.get() || slots.editor.borrow().is_some() || slots.weights.borrow().is_some()
            {
                return;
            }
            let initial = editor.borrow().edited_related_weights();
            let accepted = Rc::new({
                let editor = editor.clone();
                let active = active.clone();
                let show_page = show_page.clone();
                move |weights| {
                    if active.get() {
                        editor.borrow_mut().set_related_weights(weights);
                        show_page();
                    }
                }
            });
            if let Err(error) = crate::related_weights_window::open(
                &slots.weights,
                &initial,
                active.clone(),
                accepted,
            ) {
                eprintln!("could not edit related weights: {error}");
            }
        }
    });
    window.on_most_used_tags_clicked({
        let store = store.clone();
        let suggested_slot = suggested_slot.clone();
        let editor = editor.clone();
        let active = active.clone();
        let show_page = show_page.clone();
        move || {
            if !active.get()
                || suggested_slot.editor.borrow().is_some()
                || suggested_slot.weights.borrow().is_some()
            {
                return;
            }
            let initial = editor.borrow().edited_most_used_tags();
            let applied = Rc::new({
                let active = active.clone();
                let editor = editor.clone();
                let show_page = show_page.clone();
                move |tags| {
                    if active.get() {
                        editor.borrow_mut().set_most_used_tags(tags);
                        show_page();
                    }
                }
            });
            if let Err(error) = crate::tag_suggestions_window::open(
                &store,
                &suggested_slot,
                initial,
                active.clone(),
                applied,
            ) {
                eprintln!("could not edit most used tags: {error}");
            }
        }
    });
    window.on_gallery_source_clicked({
        let editor = editor.clone();
        let gallery_slot = gallery_slot.clone();
        let store = store.clone();
        let active = active.clone();
        let show_page = show_page.clone();
        move || {
            if !active.get() {
                return;
            }
            let current = editor.borrow().edited_gallery_source();
            let applied: crate::gallery_source_window::Applied = Rc::new({
                let editor = editor.clone();
                let active = active.clone();
                let show_page = show_page.clone();
                move |value| {
                    if !active.get() {
                        return Err("The options window has closed.".into());
                    }
                    editor.borrow_mut().set_gallery_source(Some(value));
                    show_page();
                    Ok(())
                }
            });
            if let Err(error) =
                crate::gallery_source_window::open(&store, current, false, &gallery_slot, applied)
            {
                eprintln!("could not open gallery source: {error}");
            }
        }
    });
    window.on_regex_favourites_clicked({
        let editor = editor.clone();
        let regex_slot = regex_slot.clone();
        let show_page = show_page.clone();
        move || {
            if crate::regex_favourites_window::has_open(&regex_slot) {
                return;
            }
            let favourites = editor.borrow().edited_regex_favourites();
            let applied = Rc::new({
                let editor = editor.clone();
                let show_page = show_page.clone();
                move |favourites| {
                    editor.borrow_mut().set_regex_favourites(favourites);
                    show_page();
                    Ok(())
                }
            });
            if let Err(error) =
                crate::regex_favourites_window::open(&favourites, &regex_slot, applied)
            {
                eprintln!("could not open regex favourites: {error}");
            }
        }
    });
    window.on_banner_clicked({
        let editor = editor.clone();
        let slot = banner_slot.clone();
        let active = active.clone();
        let show_page = show_page.clone();
        let store = store.clone();
        move |row| {
            if !active.get() || slot.borrow().is_some() {
                return;
            }
            let Some((target, value)) = usize::try_from(row)
                .ok()
                .and_then(|row| editor.borrow().edited_banner(row))
            else {
                return;
            };
            let presentation = store.read(hydrus_store::settings::get).unwrap_or_default();
            let applied = Rc::new({
                let editor = editor.clone();
                let active = active.clone();
                let show_page = show_page.clone();
                move |value| {
                    if active.get() {
                        editor.borrow_mut().set_banner(target, value);
                        show_page();
                    }
                }
            });
            if let Err(error) = crate::tag_banner_window::open(&value, presentation, &slot, applied)
            {
                eprintln!("could not open tag banner: {error}");
            }
        }
    });
    window.on_namespace_sorts_clicked({
        let editor = editor.clone();
        let slot = namespace_slot.clone();
        let active = active.clone();
        let show_page = show_page.clone();
        move || {
            if !active.get() || slot.borrow().is_some() {
                return;
            }
            let sorts = editor.borrow().edited_namespace_sorts();
            let advanced = editor.borrow().applied().0.advanced.0;
            let applied = Rc::new({
                let editor = editor.clone();
                let active = active.clone();
                let show_page = show_page.clone();
                move |sorts| {
                    if !active.get() {
                        return Err("The options window has closed.".into());
                    }
                    editor.borrow_mut().set_namespace_sorts(sorts);
                    show_page();
                    Ok(())
                }
            });
            if let Err(error) =
                crate::namespace_sorts_window::open(&sorts, advanced, &slot, applied)
            {
                eprintln!("could not open namespace sorting schemes: {error}");
            }
        }
    });
    window.on_import_options_clicked({
        let editor = editor.clone();
        let store = store.clone();
        let slot = import_slot.clone();
        let active = active.clone();
        let show_page = show_page.clone();
        move || {
            if !active.get() || slot.borrow().is_some() {
                return;
            }
            let value = editor.borrow().edited_import_options();
            let applied = Rc::new({
                let editor = editor.clone();
                let active = active.clone();
                let show_page = show_page.clone();
                move |value| {
                    if active.get() {
                        editor.borrow_mut().set_import_options(value);
                        show_page();
                    }
                    Ok(())
                }
            });
            if let Err(error) =
                crate::import_options_panel_window::open(&store, &value, &slot, applied)
            {
                eprintln!("could not open import options: {error}");
            }
        }
    });
    window.on_page_chosen({
        let editor = editor.clone();
        let show_page = show_page.clone();
        move |i| {
            editor.borrow_mut().show_page(at(i));
            show_page();
        }
    });
    window.on_check_toggled({
        let editor = editor.clone();
        let weak = window.as_weak();
        let active = active.clone();
        let reasons_open = reason_queue.has_open.clone();
        let frames_open = frame_table.has_open.clone();
        move |i, checked| {
            if !active.get()
                || reasons_open()
                || frames_open()
                || !matches!(
                    editor.borrow().rows().get(at(i)),
                    Some(Row::Opt { enabled: true, .. })
                )
            {
                return;
            }
            editor.borrow_mut().check(at(i), checked);
            if let Some(window) = weak.upgrade() {
                for (index, row) in editor.borrow().rows().iter().enumerate() {
                    if let Row::Opt { enabled, .. } = row
                        && let Some(mut shown) = window.get_rows().row_data(index)
                    {
                        shown.enabled = *enabled;
                        window.get_rows().set_row_data(index, shown);
                    }
                }
            }
        }
    });
    window.on_canvas_toggled({
        let editor = editor.clone();
        let active = active.clone();
        move |row, canvas, checked| {
            if active.get() {
                editor.borrow_mut().canvas(at(row), at(canvas), checked);
            }
        }
    });
    window.on_number_edited({
        let editor = editor.clone();
        move |i, n| editor.borrow_mut().number(at(i), i64::from(n))
    });
    window.on_none_toggled({
        let editor = editor.clone();
        move |i, none| editor.borrow_mut().none(at(i), none)
    });
    window.on_text_edited({
        let editor = editor.clone();
        move |i, text| editor.borrow_mut().text(at(i), &text)
    });
    window.on_directory_browse({
        let editor = editor.clone();
        let active = active.clone();
        let show_page = show_page.clone();
        move |i| {
            if !active.get() || !matches!(editor.borrow().rows().get(at(i)), Some(Row::Opt { option, .. }) if option.kind == Kind::Directory) {
                return;
            }
            if let Some(path) = crate::pick(crate::Pick::Folder, "Select directory").first()
                && active.get()
            {
                editor.borrow_mut().text(at(i), &path.to_string_lossy());
                show_page();
            }
        }
    });
    window.on_field_edited({
        let editor = editor.clone();
        move |i, field, n| editor.borrow_mut().field(at(i), at(field), i64::from(n))
    });
    // checker options' button: their editor (with advanced mode's tiny
    // least times if the options have it on, as the reference's reads
    // them), what it applies kept for "apply"
    window.on_checker_clicked({
        let session_choices = session_choices.clone();
        let editor = editor.clone();
        let store = store.clone();
        let checker_slot = checker_slot.clone();
        let weak = window.as_weak();
        move |i| {
            if checker_slot.borrow().is_some() {
                return;
            }
            let row = at(i);
            let current = match editor.borrow().rows().get(row) {
                Some(Row::Opt {
                    value: Value::Checker(options),
                    ..
                }) => options.clone(),
                _ => return,
            };
            let advanced = store
                .read(hydrus_store::settings::get::<hydrus_store::settings::AdvancedMode>)
                .is_ok_and(|a| a.0);
            let done: Rc<dyn Fn(hydrus_core::subscriptions::CheckerOptions)> = Rc::new({
                let session_choices = session_choices.clone();
                let editor = editor.clone();
                let store = store.clone();
                let weak = weak.clone();
                move |options| {
                    editor.borrow_mut().checker(row, options);
                    let Some(window) = weak.upgrade() else { return };
                    let editor = editor.borrow();
                    if let Some(shown) = editor.rows().get(row) {
                        window.get_rows().set_row_data(
                            row,
                            OptionRow {
                                found: editor.found(row),
                                ..option_row(shown, &store, &session_choices)
                            },
                        );
                    }
                }
            });
            match crate::checker_options_window::open(&current, advanced, &checker_slot, &done) {
                Ok(window) => *checker_slot.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open the checker options: {e}"),
            }
        }
    });
    window.on_choice_chosen({
        let session_choices=session_choices.clone();
        let editor = editor.clone();
        let store = store.clone();
        move |i, index| {
            let mut editor = editor.borrow_mut();
            if matches!(editor.rows().get(at(i)),Some(Row::Opt {option,..}) if matches!(option.kind,Kind::SavedSession)) {
                if let Some((name,_))=session_choices.get(at(index)) {editor.saved_session(at(i),name.clone());}
                return;
            }
            let combined = match editor.rows().get(at(i)) {
                Some(Row::Opt { option, .. }) => match option.kind {
                    Kind::TagService { combined } => Some(combined),
                    _ => None,
                },
                _ => None,
            };
            if let Some(combined) = combined {
                if let Some((service, _)) =
                    crate::options::tag_service_choices(&store, combined).get(at(index))
                {
                    editor.tag_service(at(i), service.clone());
                }
            } else {
                editor.choose(at(i), at(index));
            }
        }
    });
    // a sort's type (in its default order, as the reference's control
    // sets it), or its order; the row shows the type's orders
    let sort_edited = {
        let session_choices = session_choices.clone();
        let editor = editor.clone();
        let store = store.clone();
        let weak = window.as_weak();
        move |i: i32,
              edit: &dyn Fn(&mut hydrus_core::pages::PageSort, &[crate::sort::PageChoice])| {
            let Some(window) = weak.upgrade() else { return };
            let mut editor = editor.borrow_mut();
            let rows = editor.rows();
            let Some(Row::Opt {
                value: Value::Sort(sort),
                ..
            }) = rows.get(at(i))
            else {
                return;
            };
            let mut sort = sort.clone();
            drop(rows);
            let choices = crate::sort::page_choices(&store, &sort.by);
            edit(&mut sort, &choices);
            editor.sort(at(i), sort);
            if let Some(row) = editor.rows().get(at(i)) {
                window.get_rows().set_row_data(
                    at(i),
                    OptionRow {
                        found: editor.found(at(i)),
                        ..option_row(row, &store, &session_choices)
                    },
                );
            }
        }
    };
    window.on_sort_chosen({
        let sort_edited = sort_edited.clone();
        move |i, index| {
            sort_edited(i, &|sort, choices| {
                if let Some(choice) = choices.get(at(index)) {
                    sort.by = choice.by.clone();
                    sort.ascending = choice.default_ascending;
                }
            });
        }
    });
    window.on_order_chosen({
        let sort_edited = sort_edited.clone();
        move |i, index| {
            sort_edited(i, &|sort, _| sort.ascending = index == 0);
        }
    });
    // Each cog freezes its owned sort row and page; it never edits a search or collect context.
    window.on_sort_cog_open({
        let target = cog_target.clone();
        let editor = editor.clone();
        let weak = window.as_weak();
        let active = active.clone();
        move |row| {
            target.borrow_mut().take();
            let Some(window) = weak.upgrade().filter(|_| active.get()) else {
                return;
            };
            let editor = editor.borrow();
            let rows = editor.rows();
            let Some(Row::Opt {
                value: Value::Sort(sort),
                ..
            }) = rows.get(at(row))
            else {
                return;
            };
            let groups = hydrus_gui_model::sort_cog::groups(sort);
            if groups.is_empty() {
                return;
            }
            window.set_sort_cog_group(-1);
            window.set_sort_cog_groups(ModelRc::new(VecModel::from(
                groups
                    .into_iter()
                    .map(SharedString::from)
                    .collect::<Vec<_>>(),
            )));
            *target.borrow_mut() = Some((editor.page(), at(row), Vec::new()));
        }
    });
    window.on_sort_cog_group_chosen({
        let target = cog_target.clone();
        let editor = editor.clone();
        let store = store.clone();
        let weak = window.as_weak();
        let active = active.clone();
        move |row, group| {
            let Some(window) = weak.upgrade().filter(|_| active.get()) else {
                return;
            };
            let editor = editor.borrow();
            let mut target = target.borrow_mut();
            let Some((_, _, entries)) = target
                .as_mut()
                .filter(|(page, owner, _)| *page == editor.page() && *owner == at(row))
            else {
                return;
            };
            let rows = editor.rows();
            let Some(Row::Opt {
                value: Value::Sort(sort),
                ..
            }) = rows.get(at(row))
            else {
                return;
            };
            let new = hydrus_gui_model::sort_cog::entries(&store, sort, at(group));
            if new.is_empty() {
                return;
            }
            window.set_sort_cog_items(ModelRc::new(VecModel::from(
                new.iter()
                    .map(|entry| crate::SortCogItem {
                        label: entry.label.as_str().into(),
                        checked: entry.checked,
                        separator: entry.action.is_none(),
                    })
                    .collect::<Vec<_>>(),
            )));
            *entries = new;
            window.set_sort_cog_group(group);
        }
    });
    window.on_sort_cog_choice({
        let target = cog_target.clone();
        let editor = editor.clone();
        let active = active.clone();
        let sort_edited = sort_edited.clone();
        move |row, index| {
            if !active.get() {
                return;
            }
            let action = target
                .borrow()
                .as_ref()
                .filter(|(page, owner, _)| *page == editor.borrow().page() && *owner == at(row))
                .and_then(|(_, _, entries)| entries.get(at(index)))
                .and_then(|entry| entry.action.clone());
            let Some(action) = action else { return };
            target.borrow_mut().take();
            sort_edited(row, &|sort, _| {
                hydrus_gui_model::sort_cog::choose(sort, &action);
            });
        }
    });
    window.on_sort_cog_cancel(move || {
        cog_target.borrow_mut().take();
    });
    // a tag sort's type, order or grouping; the row shows the type's
    // orders, and grouping only where the type groups
    window.on_tag_sort_chosen({
        let session_choices = session_choices.clone();
        let editor = editor.clone();
        let store = store.clone();
        let weak = window.as_weak();
        move |i, part, index| {
            let Some(window) = weak.upgrade() else { return };
            let mut editor = editor.borrow_mut();
            editor.tag_sort(at(i), at(part), at(index));
            if let Some(row) = editor.rows().get(at(i)) {
                window.get_rows().set_row_data(
                    at(i),
                    OptionRow {
                        found: editor.found(at(i)),
                        ..option_row(row, &store, &session_choices)
                    },
                );
            }
        }
    });
    // a collect's choice checked or not, or its unmatched files' choice
    let collect_edited = {
        let session_choices = session_choices.clone();
        let editor = editor.clone();
        let store = store.clone();
        let weak = window.as_weak();
        move |i: i32,
              edit: &dyn Fn(
            &hydrus_core::pages::PageCollect,
            &[crate::collect::CollectChoice],
        ) -> hydrus_core::pages::PageCollect| {
            let Some(window) = weak.upgrade() else { return };
            let mut editor = editor.borrow_mut();
            let rows = editor.rows();
            let Some(Row::Opt {
                value: Value::Collect(collect),
                ..
            }) = rows.get(at(i))
            else {
                return;
            };
            let collect = collect.clone();
            drop(rows);
            let choices = crate::collect::choices(&store);
            editor.collect(at(i), edit(&collect, &choices));
            if let Some(row) = editor.rows().get(at(i)) {
                window.get_rows().set_row_data(
                    at(i),
                    OptionRow {
                        found: editor.found(at(i)),
                        ..option_row(row, &store, &session_choices)
                    },
                );
            }
        }
    };
    window.on_collect_toggled({
        let collect_edited = collect_edited.clone();
        move |i, choice, on| {
            collect_edited(i, &|collect, choices| {
                crate::collect::toggled(choices, collect, at(choice), on)
            });
        }
    });
    window.on_unmatched_chosen(move |i, index| {
        collect_edited(i, &|collect, _| hydrus_core::pages::PageCollect {
            collect_unmatched: index == 0,
            ..collect.clone()
        });
    });
    window.on_apply({
        let suggested_slot = suggested_slot.clone();
        let reasons_open = reason_queue.has_open.clone();
        let frames_open = frame_table.has_open.clone();
        let import_slot = import_slot.clone();
        let namespace_slot = namespace_slot.clone();
        let banner_slot = banner_slot.clone();
        let active = active.clone();
        let tag_slot = tag_slot.clone();
        let editor = editor.clone();
        let store = store.clone();
        let close = close.clone();
        move || {
            if !active.get()
                || reasons_open()
                || frames_open()
                || tag_slot.borrow().is_some()
                || import_slot.borrow().is_some()
                || namespace_slot.borrow().is_some()
                || banner_slot.borrow().is_some()
                || suggested_slot.editor.borrow().is_some()
                || suggested_slot.weights.borrow().is_some()
            {
                return;
            }
            let (mut after, before, problems) = {
                let editor = editor.borrow();
                let (after, before, problems) = editor.applied();
                (after, before.clone(), problems)
            };
            if after
                .export
                .default_directory
                .as_deref()
                .is_some_and(|path| std::path::Path::new(path).is_relative())
            {
                after.export.default_directory = Some(
                    hydrus_gui_model::export_files::default_directory(&store, &after.export),
                );
            }
            let saved = store.write_and_refresh(move |ctx| {
                after.save(ctx.conn(), &before)?;
                let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
                for problem in &problems {
                    let job = hydrus_store::popups::Job::text(problem.clone(), now as f64);
                    hydrus_store::popups::add(ctx.conn(), &job, now)?;
                }
                Ok(())
            });
            if let Err(e) = saved {
                eprintln!("could not save the options: {e}");
                return;
            }
            applied();
            close();
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
    crate::windows::place_named(window.window(), store, "manage_options_dialog");
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
