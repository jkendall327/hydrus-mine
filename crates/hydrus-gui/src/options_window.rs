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
                (Kind::ImportOptions, Value::ImportOptions(_)) => {
                    out.kind = 18;
                    out.text = "edit import options".into();
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
pub(crate) fn open(
    store: &Arc<Store>,
    slot: &Rc<RefCell<Option<OptionsWindow>>>,
    checker_slot: &Rc<RefCell<Option<CheckerOptionsWindow>>>,
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
    let active = Rc::new(Cell::new(true));
    let names: Vec<StandardListViewItem> = editor
        .borrow()
        .page_names()
        .into_iter()
        .map(StandardListViewItem::from)
        .collect();
    window.set_pages(ModelRc::new(VecModel::from(names)));
    // (the rows are made anew only as the page changes: an edit leaves its
    // control as the user left it)
    let show_page = {
        let session_choices = session_choices.clone();
        let editor = editor.clone();
        let store = store.clone();
        let weak = window.as_weak();
        move || {
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
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        let import_slot = import_slot.clone();
        let regex_slot = regex_slot.clone();
        let gallery_slot = gallery_slot.clone();
        let location_slot = location_slot.clone();
        let tag_slot = tag_slot.clone();
        let active = active.clone();
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
            crate::import_options_panel_window::cancel(&import_slot);
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
        move |i, checked| {
            editor.borrow_mut().check(at(i), checked);
            if let Some(window) = weak.upgrade() {
                for (index, row) in editor.borrow().rows().iter().enumerate() {
                    if let Row::Opt {
                        option, enabled, ..
                    } = row
                        && matches!(option.kind, Kind::TagService { .. })
                        && let Some(mut shown) = window.get_rows().row_data(index)
                    {
                        shown.enabled = *enabled;
                        window.get_rows().set_row_data(index, shown);
                    }
                }
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
    window.on_order_chosen(move |i, index| {
        sort_edited(i, &|sort, _| sort.ascending = index == 0);
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
        let import_slot = import_slot.clone();
        let active = active.clone();
        let tag_slot = tag_slot.clone();
        let editor = editor.clone();
        let store = store.clone();
        let close = close.clone();
        move || {
            if !active.get() || tag_slot.borrow().is_some() {
                return;
            }
            let (after, before, problems) = {
                let editor = editor.borrow();
                let (after, before, problems) = editor.applied();
                (after, before.clone(), problems)
            };
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
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
