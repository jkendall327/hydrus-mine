//! Manage local services with detached edits and transaction-time deletion checks.
use crate::{
    EditServiceWindow, ServiceColourRow, ServiceRatingExampleRow, ServicesEditorWindow,
    TableColumn, TableRow,
};
use crate::import_options_window::advanced_mode;
use hydrus_core::{ServiceId, ServiceKey};
use hydrus_gui_model::services_editor::{self, Editor};
use hydrus_store::{
    Store,
    services::{PenBrush, RatingDisplay, Rgb, Service, ServiceKind, StarAppearance, StarShape},
};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

/// Windows retained by the main window and exposed to interaction tests.
#[derive(Clone, Default)]
pub struct Slots {
    pub manage: Rc<RefCell<Option<ServicesEditorWindow>>>,
    pub edit: Rc<RefCell<Option<EditServiceWindow>>>,
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slots").finish_non_exhaustive()
    }
}

fn show(window: &ServicesEditorWindow, editor: &Editor) {
    window.set_sort_column(i32::try_from(editor.sort_column).unwrap_or(1));
    window.set_ascending(editor.ascending);
    window.set_rows(ModelRc::new(VecModel::from(
        editor
            .rows
            .iter()
            .map(|r| TableRow {
                cells: ModelRc::new(VecModel::from(r.cells().map(SharedString::from).to_vec())),
                selected: editor.selection.is_selected(r.token),
            })
            .collect::<Vec<_>>(),
    )));
    window.set_can_edit(editor.selected().is_some_and(|r| {
        !matches!(
            r.service.kind,
            ServiceKind::TagRepository(_)
                | ServiceKind::FileRepository(_)
                | ServiceKind::Ipfs(_)
                | ServiceKind::Unsupported { .. }
        )
    }));
    window.set_can_delete(editor.rows.iter().any(|r| {
        editor.selection.is_selected(r.token)
            && hydrus_store::services_management::editable(&r.service.kind)
    }));
}
fn display(kind: &ServiceKind) -> Option<&RatingDisplay> {
    match kind {
        ServiceKind::RatingLike(c) => Some(&c.display),
        ServiceKind::RatingNumerical(c) => Some(&c.display),
        ServiceKind::RatingIncDec(c) => Some(c),
        _ => None,
    }
}
fn appearance(kind: &ServiceKind) -> Option<&StarAppearance> {
    match kind {
        ServiceKind::RatingLike(c) => Some(&c.appearance),
        ServiceKind::RatingNumerical(c) => Some(&c.appearance),
        _ => None,
    }
}
const SHAPES: [u8; 22] = [
    0, 1, 2, 3, 4, 5, 6, 7, 30, 31, 32, 33, 40, 42, 43, 44, 50, 60, 61, 101, 102, 103,
];
fn rgb(text: &str) -> Result<Rgb, String> {
    serde_json::from_value(serde_json::Value::String(text.into()))
        .map_err(|_| format!("Invalid colour: {text}. Enter #RRGGBB."))
}

fn edited_kind(
    window: &EditServiceWindow,
    original: &ServiceKind,
    colours: &[(String, String)],
) -> Result<ServiceKind, String> {
    if let ServiceKind::ClientApi(config) = original {
        use hydrus_gui_model::client_api_admin::{Binding, ListenerEdit, server_config};
        return server_config(
            config,
            &ListenerEdit {
                port: window.get_api_running().then(|| window.get_api_port()),
                binding: if window.get_api_non_local() {
                    Binding::Network
                } else {
                    Binding::LocalOnly
                },
                cors: window.get_api_cors(),
                logs: window.get_api_logs(),
                use_https: window.get_api_https(),
                normie_eris: window.get_api_normie(),
                external_scheme: (!window.get_api_scheme_none())
                    .then(|| window.get_api_scheme().to_string()),
                external_host: (!window.get_api_host_none())
                    .then(|| window.get_api_host().to_string()),
                external_port: (!window.get_api_external_port_none())
                    .then(|| window.get_api_external_port().to_string()),
            },
        )
        .map(ServiceKind::ClientApi);
    }
    let mut kind = original.clone();
    let display = match &mut kind {
        ServiceKind::RatingLike(c) => Some(&mut c.display),
        ServiceKind::RatingNumerical(c) => Some(&mut c.display),
        ServiceKind::RatingIncDec(c) => Some(c),
        _ => None,
    };
    if let Some(d) = display {
        d.show_in_thumbnail = window.get_show_thumbnail();
        d.show_in_thumbnail_even_when_null = window.get_show_null();
        let incdec = matches!(original, ServiceKind::RatingIncDec(_));
        for (i, (border, fill)) in colours.iter().enumerate() {
            let colour = PenBrush {
                pen: rgb(border)?,
                brush: rgb(fill)?,
            };
            match i {
                0 => d.colours.like = colour,
                1 if incdec => d.colours.mixed = colour,
                1 => d.colours.dislike = colour,
                2 => d.colours.null = colour,
                _ => d.colours.mixed = colour,
            }
        }
    }
    let appearance = if usize::try_from(window.get_appearance()).ok() == Some(SHAPES.len()) {
        if window.get_svg().is_empty() {
            return Err("Enter an SVG name.".into());
        }
        StarAppearance::Svg(window.get_svg().to_string())
    } else {
        StarAppearance::Shape(StarShape(
            *usize::try_from(window.get_appearance())
                .ok()
                .and_then(|i| SHAPES.get(i))
                .ok_or("Unknown rating shape.")?,
        ))
    };
    match &mut kind {
        ServiceKind::RatingLike(c) => c.appearance = appearance,
        ServiceKind::RatingNumerical(c) => {
            c.appearance = appearance;
            c.num_stars = u32::try_from(window.get_stars()).map_err(|_| "Invalid star count.")?;
            c.allow_zero = window.get_allow_zero();
            services_editor::normalize_numerical(c);
            c.custom_pad = window.get_icon_padding();
            c.show_fraction_beside_stars =
                u8::try_from(window.get_fraction()).map_err(|_| "Invalid fraction placement.")?;
        }
        _ => {}
    }
    hydrus_store::services_management::validate(&kind).map_err(|e| e.to_string())?;
    Ok(kind)
}
fn edit(
    service: Service,
    slots: &Slots,
    done: Rc<dyn Fn(String, ServiceKind)>,
    parent_active: Rc<Cell<bool>>,
    sizes: [(f64, f64); 4],
    advanced: bool,
) -> Result<(), String> {
    let window = EditServiceWindow::new().map_err(|e| e.to_string())?;
    window.set_service_name(service.name.as_str().into());
    window.set_service_type(service.service_type().name().into());
    window.set_rating(display(&service.kind).is_some());
    window.set_star_rating(appearance(&service.kind).is_some());
    window.set_numerical(matches!(service.kind, ServiceKind::RatingNumerical(_)));
    window.set_client_api(matches!(service.kind, ServiceKind::ClientApi(_)));
    if let ServiceKind::ClientApi(c) = &service.kind {
        window.set_api_running(c.port.is_some());
        window.set_api_port(i32::from(c.port.unwrap_or(45869)));
        window.set_api_non_local(c.allow_non_local_connections);
        window.set_api_cors(c.support_cors);
        window.set_api_logs(c.log_requests);
        window.set_api_https(c.use_https);
        window.set_api_normie(c.use_normie_eris);
        window.set_api_advanced(advanced);
        let (scheme, host, port) = (
            &c.external_scheme_override,
            &c.external_host_override,
            &c.external_port_override,
        );
        window.set_api_scheme_none(scheme.is_none());
        window.set_api_scheme(scheme.clone().unwrap_or_default().into());
        window.set_api_host_none(host.is_none());
        window.set_api_host(host.clone().unwrap_or_default().into());
        window.set_api_external_port_none(port.is_none());
        window.set_api_external_port(port.clone().unwrap_or_default().into());
        {
            use hydrus_gui_model::client_api_admin::tooltips;
            window.set_api_tooltip_non_local(tooltips::NON_LOCAL.into());
            window.set_api_tooltip_https(tooltips::HTTPS.into());
            window.set_api_tooltip_cors(tooltips::CORS.into());
            window.set_api_tooltip_logs(tooltips::LOGS.into());
            window.set_api_tooltip_normie(tooltips::NORMIE.into());
            window.set_api_tooltip_external_port(tooltips::EXTERNAL_PORT.into());
        }
    }
    let mut colours = Vec::new();
    let mut colour_rows = Vec::new();
    if let Some(d) = display(&service.kind) {
        window.set_show_thumbnail(d.show_in_thumbnail);
        window.set_show_null(d.show_in_thumbnail_even_when_null);
        let rows = if matches!(service.kind, ServiceKind::RatingIncDec(_)) {
            vec![
                ("normal rating", d.colours.like),
                ("a mixture of ratings", d.colours.mixed),
            ]
        } else {
            vec![
                ("liked", d.colours.like),
                ("disliked", d.colours.dislike),
                ("not rated", d.colours.null),
                ("a mixture of ratings", d.colours.mixed),
            ]
        };
        for (label, c) in rows {
            colours.push((c.pen.to_string(), c.brush.to_string()));
            colour_rows.push(ServiceColourRow {
                label: label.into(),
                border: c.pen.to_string().into(),
                fill: c.brush.to_string().into(),
            });
        }
    }
    window.set_colours(ModelRc::new(VecModel::from(colour_rows)));
    let mut appearances = SHAPES
        .iter()
        .map(|code| SharedString::from(StarShape(*code).name().unwrap_or("circle")))
        .collect::<Vec<_>>();
    appearances.push("svg".into());
    window.set_appearances(ModelRc::new(VecModel::from(appearances)));
    if let Some(a) = appearance(&service.kind) {
        match a {
            StarAppearance::Shape(s) => window.set_appearance(
                i32::try_from(SHAPES.iter().position(|c| *c == s.0).unwrap_or(0)).unwrap_or(0),
            ),
            StarAppearance::Svg(name) => {
                window.set_appearance(i32::try_from(SHAPES.len()).unwrap_or(0));
                window.set_svg(name.as_str().into());
            }
        }
    }
    if let ServiceKind::RatingNumerical(c) = &service.kind {
        window.set_stars(i32::try_from(c.num_stars).unwrap_or(20));
        window.set_allow_zero(c.allow_zero);
        window.set_icon_padding(c.custom_pad);
        window.set_fraction(i32::from(c.show_fraction_beside_stars));
    }
    let colours = Rc::new(RefCell::new(colours));
    let active = Rc::new(Cell::new(true));
    let samples = Rc::new(RefCell::new(
        hydrus_gui_model::rating_example::Example::new(&service.kind),
    ));
    let pending_counter = Rc::new(Cell::new(None::<usize>));
    let refresh = Rc::new({
        let weak = window.as_weak();
        let colours = colours.clone();
        let original = service.kind.clone();
        let samples = samples.clone();
        let active = active.clone();
        let parent_active = parent_active.clone();
        move || {
            if !active.get() || !parent_active.get() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            let Some(example) = samples.borrow().as_ref().cloned() else {
                return;
            };
            let mut kind = match edited_kind(&w, &original, &colours.borrow()) {
                Ok(kind) => kind,
                Err(error) => {
                    w.set_error(error.into());
                    return;
                }
            };
            // This checkbox does not update Qt's example service. It remains a
            // staged persistence setting, independent of the opening sample conversion.
            if let (ServiceKind::RatingNumerical(c), ServiceKind::RatingNumerical(initial)) =
                (&mut kind, &original)
            {
                c.allow_zero = initial.allow_zero;
            }
            let rows = (0..4)
                .filter_map(|i| {
                    let control = example.control(i, &kind)?;
                    let mut graphic = crate::rating_row(&control);
                    let fraction_placement = if let ServiceKind::RatingNumerical(c) = &kind {
                        graphic.pad = c.custom_pad as f32;
                        i32::from(c.show_fraction_beside_stars)
                    } else {
                        0
                    };
                    let (icon_size, incdec_height) = sizes[i];
                    let counter_width =
                        if let hydrus_gui_model::rating_example::Sample::IncDec(value) =
                            example.samples()[i]
                        {
                            hydrus_gui_model::rating_example::counter_width(incdec_height, value)
                        } else {
                            0.0
                        };
                    Some(ServiceRatingExampleRow {
                        label: hydrus_gui_model::rating_example::LABELS[i].into(),
                        graphic,
                        icon_size: icon_size as f32,
                        incdec_height: incdec_height as f32,
                        outline: hydrus_gui_model::ratings::outline_width(icon_size) as f32,
                        counter_width: counter_width as f32,
                        fraction: example.fraction(i, &kind).into(),
                        fraction_placement,
                    })
                })
                .collect::<Vec<_>>();
            w.set_examples(ModelRc::new(VecModel::from(rows)));
            w.set_error(SharedString::new());
        }
    });
    window.on_preview_edited({
        let refresh = refresh.clone();
        move || refresh()
    });
    window.on_colour_edited({
        let colours = colours.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let parent_active = parent_active.clone();
        move |i, border, text| {
            if !active.get() || !parent_active.get() {
                return;
            }
            if let Ok(i) = usize::try_from(i)
                && let Some(c) = colours.borrow_mut().get_mut(i)
            {
                if border {
                    c.0 = text.to_string();
                } else {
                    c.1 = text.to_string();
                }
            }
            refresh();
        }
    });
    window.on_preview_clicked({
        let weak = window.as_weak();
        let samples = samples.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let parent_active = parent_active.clone();
        let pending_counter = pending_counter.clone();
        move |index, right, proportion| {
            if !active.get() || !parent_active.get() || pending_counter.get().is_some() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            if !w.window().is_visible() {
                return;
            }
            if let Ok(index) = usize::try_from(index)
                && let Some(example) = samples.borrow_mut().as_mut()
            {
                example.click(index, right, f64::from(proportion));
            }
            refresh();
        }
    });
    window.on_preview_pointer({
        let weak = window.as_weak();
        let samples = samples.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let parent_active = parent_active.clone();
        let pending_counter = pending_counter.clone();
        move |index, right, x, width, icon, drag| {
            if !active.get() || !parent_active.get() || pending_counter.get().is_some() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            if !w.window().is_visible() {
                return;
            }
            if let Ok(index) = usize::try_from(index)
                && let Some(example) = samples.borrow_mut().as_mut()
            {
                example.pointer(
                    index,
                    right,
                    f64::from(x),
                    f64::from(width),
                    f64::from(icon),
                    drag,
                );
            }
            refresh();
        }
    });
    window.on_counter_edit({
        let weak = window.as_weak();
        let samples = samples.clone();
        let active = active.clone();
        let parent_active = parent_active.clone();
        let pending_counter = pending_counter.clone();
        move |index| {
            if !active.get() || !parent_active.get() || pending_counter.get().is_some() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            if !w.window().is_visible() {
                return;
            }
            if let Ok(index) = usize::try_from(index)
                && let Some(example) = samples.borrow().as_ref()
                && let Some(hydrus_gui_model::rating_example::Sample::IncDec(value)) =
                    example.samples().get(index)
            {
                pending_counter.set(Some(index));
                w.set_counter_value(i32::try_from(*value).unwrap_or(1_000_000));
                w.set_counter_editing(true);
            }
        }
    });
    window.on_counter_answered({
        let weak = window.as_weak();
        let samples = samples.clone();
        let active = active.clone();
        let parent_active = parent_active.clone();
        let pending_counter = pending_counter.clone();
        let refresh = refresh.clone();
        move |yes| {
            if !active.get() || !parent_active.get() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            if !w.window().is_visible() {
                return;
            }
            let Some(index) = pending_counter.take() else {
                return;
            };
            if yes && let Some(example) = samples.borrow_mut().as_mut() {
                example.set_counter(index, u32::try_from(w.get_counter_value()).unwrap_or(0));
            }
            w.set_counter_editing(false);
            refresh();
        }
    });
    refresh();
    let close = Rc::new({
        let active = active.clone();
        let slot = slots.edit.clone();
        let weak = window.as_weak();
        move || {
            if !active.replace(false) {
                return;
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            slot.borrow_mut().take();
        }
    });
    window.on_apply_clicked({
        let weak = window.as_weak();
        let close = close.clone();
        move || {
            if !active.get() || !parent_active.get() || pending_counter.get().is_some() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            if !w.window().is_visible() {
                return;
            }
            if w.get_service_name().is_empty() {
                w.set_error("Please enter a name!".into());
                return;
            }
            match edited_kind(&w, &service.kind, &colours.borrow()) {
                Ok(kind) => {
                    done(w.get_service_name().to_string(), kind);
                    close();
                }
                Err(e) => w.set_error(e.into()),
            }
        }
    });
    window.on_cancel_clicked({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show().map_err(|e| e.to_string())?;
    *slots.edit.borrow_mut() = Some(window);
    Ok(())
}

/// Open a detached manage-services list. Apply refreshes snapshot and visible media.
pub fn open(
    store: &Arc<Store>,
    slots: &Slots,
    changed: Rc<dyn Fn()>,
) -> Result<ServicesEditorWindow, String> {
    let window = ServicesEditorWindow::new().map_err(|e| e.to_string())?;
    let active = Rc::new(Cell::new(true));
    let editor = Rc::new(RefCell::new(Editor::new(store).map_err(|e| e.to_string())?));
    let sizes = store
        .read(|conn| {
            let thumbnails = hydrus_store::settings::get::<
                hydrus_core::thumbnail::ThumbnailRatingSettings,
            >(conn)?;
            let viewer = hydrus_store::settings::get::<
                hydrus_core::media_viewer::MediaViewerSettings,
            >(conn)?;
            let contexts =
                hydrus_store::settings::get::<hydrus_store::settings::RatingContextSizes>(conn)?;
            Ok([
                (
                    thumbnails.icon_size.trunc(),
                    thumbnails.incdec_height.trunc(),
                ),
                (
                    viewer.rating_icon_size.trunc(),
                    viewer.rating_incdec_height.trunc(),
                ),
                (
                    contexts.preview_icon_size.trunc(),
                    contexts.preview_incdec_height.trunc(),
                ),
                (
                    contexts.dialog_icon_size.trunc(),
                    contexts.dialog_incdec_height.trunc(),
                ),
            ])
        })
        .map_err(|e| e.to_string())?;
    window.set_columns(ModelRc::new(VecModel::from(
        services_editor::COLUMNS
            .iter()
            .enumerate()
            .map(|(i, title)| TableColumn {
                title: (*title).into(),
                width: if i == 0 { 170.0 } else { 220.0 },
                stretch: i == 0,
            })
            .collect::<Vec<_>>(),
    )));
    window.set_add_types(ModelRc::new(VecModel::from(
        services_editor::ADD_TYPES
            .iter()
            .map(|t| SharedString::from(t.name()))
            .collect::<Vec<_>>(),
    )));
    show(&window, &editor.borrow());
    let close = Rc::new({
        let weak = window.as_weak();
        let slots = slots.clone();
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            let edit = slots
                .edit
                .borrow()
                .as_ref()
                .map(ComponentHandle::clone_strong);
            if let Some(edit) = edit {
                edit.invoke_cancel_clicked();
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            slots.manage.borrow_mut().take();
        }
    });
    window.on_row_clicked({
        let editor = editor.clone();
        let weak = window.as_weak();
        move |i, c, s| {
            if let (Some(w), Ok(i)) = (weak.upgrade(), usize::try_from(i)) {
                let order = editor.borrow().order();
                editor.borrow_mut().selection.click(&order, i, c, s);
                show(&w, &editor.borrow());
            }
        }
    });
    window.on_sort({
        let editor = editor.clone();
        let weak = window.as_weak();
        move |i, a| {
            if let (Some(w), Ok(i)) = (weak.upgrade(), usize::try_from(i)) {
                editor.borrow_mut().sort(i, a);
                show(&w, &editor.borrow());
            }
        }
    });
    window.on_add_clicked({
        let editor = editor.clone();
        let store = store.clone();
        let weak = window.as_weak();
        let slots = slots.clone();
        let active = active.clone();
        move |i| {
            if !active.get() || slots.edit.borrow().is_some() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            let Some(kind) = usize::try_from(i)
                .ok()
                .and_then(|i| services_editor::ADD_TYPES.get(i))
                .and_then(|t| services_editor::default_kind(*t))
            else {
                return;
            };
            let key = ServiceKey::new(rand::random::<[u8; 32]>().to_vec());
            let service = Service {
                id: ServiceId(0),
                key: key.clone(),
                name: "new service".into(),
                kind,
            };
            let done = Rc::new({
                let editor = editor.clone();
                let weak = weak.clone();
                move |name: String, kind: ServiceKind| {
                    if let Some(w) = weak.upgrade() {
                        let added = editor.borrow_mut().add(key.clone(), kind);
                        let result =
                            added.and_then(|token| editor.borrow_mut().rename(token, &name));
                        if let Err(e) = result {
                            w.set_error(e.into());
                        }
                        show(&w, &editor.borrow());
                    }
                }
            });
            if let Err(e) = edit(service, &slots, done, active.clone(), sizes, advanced_mode(&store)) {
                w.set_error(e.into());
            }
        }
    });
    window.on_edit_clicked({
        let editor = editor.clone();
        let store = store.clone();
        let weak = window.as_weak();
        let slots = slots.clone();
        let active = active.clone();
        move || {
            if !active.get() || slots.edit.borrow().is_some() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            if !w.get_can_edit() {
                w.set_error(
                    "Remote repository, IPFS and account configuration is not available here yet."
                        .into(),
                );
                return;
            }
            let Some(row) = editor.borrow().selected().cloned() else {
                return;
            };
            let token = row.token;
            let done = Rc::new({
                let editor = editor.clone();
                let weak = weak.clone();
                move |name: String, kind: ServiceKind| {
                    if let Some(w) = weak.upgrade() {
                        if let Some(row) = editor
                            .borrow_mut()
                            .rows
                            .iter_mut()
                            .find(|r| r.token == token)
                        {
                            row.service.kind = kind;
                        }
                        if let Err(e) = editor.borrow_mut().rename(token, &name) {
                            w.set_error(e.into());
                        }
                        show(&w, &editor.borrow());
                    }
                }
            });
            if let Err(e) = edit(row.service, &slots, done, active.clone(), sizes, advanced_mode(&store)) {
                w.set_error(e.into());
            }
        }
    });
    let deleting = Rc::new(RefCell::new(false));
    window.on_delete_clicked({
        let editor = editor.clone();
        let store = store.clone();
        let weak = window.as_weak();
        let deleting = deleting.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                match editor.borrow().delete_question(&store) {
                    Ok(Some(q)) => {
                        *deleting.borrow_mut() = true;
                        w.set_question(q.into());
                    }
                    Ok(None) => {}
                    Err(e) => w.set_error(e.into()),
                }
            }
        }
    });
    let apply = Rc::new({
        let editor = editor.clone();
        let store = store.clone();
        let close = close.clone();
        let weak = window.as_weak();
        let active = active.clone();
        let slots = slots.clone();
        move || {
            if !active.get() || slots.edit.borrow().is_some() {
                return;
            }
            if let Some(w) = weak.upgrade() {
                match editor.borrow().apply(&store) {
                    Ok(()) => {
                        changed();
                        close();
                    }
                    Err(e) => w.set_error(e.to_string().into()),
                }
            }
        }
    });
    window.on_apply_clicked({
        let editor = editor.clone();
        let weak = window.as_weak();
        let apply = apply.clone();
        let deleting = deleting.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                *deleting.borrow_mut() = false;
                if let Some(q) = editor.borrow().apply_question() {
                    w.set_question(q.into());
                } else {
                    apply();
                }
            }
        }
    });
    window.on_answered({
        let editor = editor.clone();
        let weak = window.as_weak();
        move |yes| {
            if let Some(w) = weak.upgrade() {
                w.set_question(SharedString::new());
                if yes {
                    if *deleting.borrow() {
                        editor.borrow_mut().delete_selected();
                        show(&w, &editor.borrow());
                    } else {
                        apply();
                    }
                }
            }
        }
    });
    window.on_cancel_clicked({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
