//! Native notebook settings, independent measured tab fits and hierarchy rows.
use crate::{MainWindow, PageTreeRow, Pages, TabFit, TabNames};
use hydrus_core::pages::PageKey;
use hydrus_store::settings::{self, TabPresentationSettings};
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{cell::RefCell, rc::Rc};

fn begin(text: &str) -> TabFit {
    let count = i32::try_from(text.chars().count()).unwrap_or(i32::MAX);
    TabFit {
        low: -1,
        high: count,
        candidate: count,
        display: "".into(),
        done: false,
    }
}
fn step(text: &str, mut fit: TabFit, measured: f32, room: f32) -> TabFit {
    if fit.done {
        return fit;
    }
    if measured.is_finite() && room.is_finite() && measured <= room.max(0.0) {
        fit.low = fit.candidate;
        fit.display = hydrus_gui_model::tab_presentation::middle_name(
            text,
            usize::try_from(fit.candidate).unwrap_or(0),
        )
        .into();
    } else {
        fit.high = fit.candidate - 1;
    }
    if fit.low >= fit.high {
        fit.done = true;
    } else {
        fit.candidate = fit.low + (fit.high - fit.low + 1) / 2;
    }
    fit
}

fn vertical_label(
    text: &str,
    clockwise: bool,
    selected: bool,
    colour: slint::Color,
    span: f32,
    font_size: f32,
) -> slint::Image {
    if !span.is_finite()
        || span <= 0.0
        || !font_size.is_finite()
        || font_size <= 0.0
        || text.is_empty()
    {
        return slint::Image::default();
    }
    // These are the same native sans-serif/default-size glyphs that the tab's
    // Text uses. SVG text is resolved by Slint's font context, not a second
    // font engine or a separate window. Escape page names as text, never SVG.
    let text = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let transform = if clockwise {
        "translate(26 0) rotate(90)".to_owned()
    } else {
        format!("translate(0 {span}) rotate(-90)")
    };
    let weight = if selected { 600 } else { 400 };
    let red = colour.red();
    let green = colour.green();
    let blue = colour.blue();
    let opacity = f32::from(colour.alpha()) / 255.0;
    let centre = span / 2.0;
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="26" height="{span}"><g transform="{transform}"><text x="{centre}" y="13" dominant-baseline="central" text-anchor="middle" font-family="sans-serif" font-size="{font_size}" font-weight="{weight}" fill="rgb({red},{green},{blue})" fill-opacity="{opacity}">{text}</text></g></svg>"#
    );
    slint::Image::load_from_svg_data(svg.as_bytes()).unwrap_or_default()
}

pub(crate) fn bind_names(window: &MainWindow) {
    let names = window.global::<TabNames<'_>>();
    names.on_begin(|text| begin(&text));
    names.on_middle(|text, count| {
        hydrus_gui_model::tab_presentation::middle_name(&text, usize::try_from(count).unwrap_or(0))
            .into()
    });
    names.on_step(|text, fit, measured, room| step(&text, fit, measured, room));
    names.on_vertical_label(|text, clockwise, selected, colour, span, font_size| {
        vertical_label(&text, clockwise, selected, colour, span, font_size)
    });
}

pub(crate) fn show(window: &MainWindow, pages: &Pages) {
    let settings: TabPresentationSettings = pages.store().read(settings::get).unwrap_or_default();
    let drag: hydrus_store::settings::TabDragSettings =
        pages.store().read(settings::get).unwrap_or_default();
    window.set_tab_wheel_scroll(drag.wheel_scroll);
    window.set_tab_alignment(settings.alignment.code());
    window.set_page_tree_side(settings.tree_side());
    window.set_navigation_tabs_hidden(settings.tabs_hidden());
    window.set_elide_tab_names(settings.elide_names);
    let mut state = pages.page_tree.borrow_mut();
    state.sync(&pages.session().pages, pages.shown().key);
    let rows: Vec<_> = state
        .visible()
        .iter()
        .map(|node| PageTreeRow {
            key: node.key.to_hex().into(),
            name: node.name.as_str().into(),
            depth: node.depth,
            selected: state.current() == Some(node.key),
            branch: node.branch,
            expanded: state.expanded(node.key),
        })
        .collect();
    let current = rows
        .iter()
        .position(|row| row.selected)
        .and_then(|index| i32::try_from(index).ok())
        .unwrap_or(-1);
    window.set_page_tree(ModelRc::new(VecModel::from(rows)));
    window.set_page_tree_current_index(current);
}

pub(crate) fn bind_tree(window: &MainWindow, pages: &Rc<RefCell<Pages>>) {
    window.on_page_tree_selected({
        let pages = pages.clone();
        let weak = window.as_weak();
        move |text| {
            if let (Some(key), Some(window)) = (PageKey::from_hex(&text), weak.upgrade()) {
                let pages = pages.borrow();
                pages.page_tree.borrow_mut().select(key);
                show(&window, &pages);
            }
        }
    });
    window.on_page_tree_toggled({
        let pages = pages.clone();
        let weak = window.as_weak();
        move |text| {
            if let (Some(key), Some(window)) = (PageKey::from_hex(&text), weak.upgrade()) {
                let pages = pages.borrow();
                pages.page_tree.borrow_mut().toggle(key);
                show(&window, &pages);
            }
        }
    });
    window.on_page_tree_navigate({
        let pages = pages.clone();
        let weak = window.as_weak();
        move |action| {
            if let Some(window) = weak.upgrade() {
                let activated = {
                    let pages = pages.borrow();
                    let result = pages.page_tree.borrow_mut().navigate(action);
                    show(&window, &pages);
                    result
                };
                if let Some(key) = activated {
                    window.invoke_page_tree_chosen(key.to_hex().into());
                }
            }
        }
    });
}
