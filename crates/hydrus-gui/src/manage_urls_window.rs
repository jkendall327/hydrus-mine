//! The "manage urls" dialog, bound (`ui/manage_urls.slint`): the files'
//! URLs read from the store into hydrus-gui-model's
//! [`UrlsEditor`](crate::urls_editor::UrlsEditor), its questions asked in
//! the window's own panel, and "apply" adding and deleting the URLs in
//! the order they were changed, as the reference's `EditURLsPanel` does.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::HashId;
use hydrus_store::Store;

use crate::list_selection::ListSelection;
use crate::urls_editor::{MULTIPLE_FILES_WARNING, TEXT_LEFT_QUESTION, UrlsEditor, title};
use crate::{ManageUrlsWindow, TableRow};

/// What the window's panel asks.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Question {
    /// Whether to add these, which aren't URLs, anyway.
    Force(Vec<String>),
    /// Whether to apply with text left in the box.
    TextLeft,
}

struct State {
    editor: UrlsEditor,
    selection: ListSelection<usize>,
    asking: Option<(Question, String)>,
}

fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = items.into_iter().map(Into::into).collect();
    ModelRc::new(VecModel::from(items))
}

fn show(window: &ManageUrlsWindow, state: &State) {
    let editor = &state.editor;
    window.set_warning(if editor.warning() {
        MULTIPLE_FILES_WARNING.into()
    } else {
        SharedString::new()
    });
    let selected = editor.selected();
    let rows: Vec<TableRow> = editor
        .rows()
        .into_iter()
        .map(|(url, label)| TableRow {
            cells: strings([label]),
            selected: selected.contains(&url),
        })
        .collect();
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    // (not while it is the text typed, so typing isn't disturbed)
    if window.get_input().as_str() != editor.input {
        window.set_input(editor.input.as_str().into());
    }
    window.set_asking(state.asking.is_some());
    if let Some((_, message)) = &state.asking {
        window.set_asking_title("Are you sure?".into());
        window.set_asking_message(message.as_str().into());
        window.set_asking_choices(strings(["yes".to_owned(), "no".to_owned()]));
    }
}

/// A URL normalised, or None if it isn't one.
type Normalise = dyn Fn(&str) -> Option<String>;

/// A URL as the store's URL classes normalise it, or None if it isn't one.
fn normaliser(store: &Store) -> impl Fn(&str) -> Option<String> + use<> {
    let classes = store.snapshot().url_classes.clone();
    move |url: &str| {
        hydrus_core::url::functions::check_full_url(url).ok()?;
        classes.normalise(url, true).ok()
    }
}

impl State {
    /// Enter `urls`, asking first (and leaving them) if some aren't URLs.
    fn enter(&mut self, urls: Vec<String>, normalise: &Normalise) {
        let mut asked = None;
        let entered = self.editor.enter(&urls, normalise, &mut |q| {
            asked = Some(q.to_owned());
            false
        });
        if !entered && let Some(message) = asked {
            self.asking = Some((Question::Force(urls), message));
        }
        self.reselect();
    }

    /// The list's selection as the editor's, after the rows change.
    fn reselect(&mut self) {
        let selected = self.editor.selected();
        let rows: Vec<usize> = self
            .editor
            .rows()
            .iter()
            .enumerate()
            .filter(|(_, (url, _))| selected.contains(url))
            .map(|(i, _)| i)
            .collect();
        self.selection = ListSelection::default();
        self.selection.select_many(&rows);
    }

    fn url(&self, row: usize) -> Option<String> {
        self.editor.rows().get(row).map(|(url, _)| url.clone())
    }
}

/// Open the dialog on `files`' URLs; it forgets itself from `slot` when
/// closed, and calls `applied` once the URLs are written.
pub(crate) fn open(
    store: &Arc<Store>,
    files: Vec<HashId>,
    slot: &Rc<RefCell<Option<ManageUrlsWindow>>>,
    applied: Rc<dyn Fn()>,
) -> Result<ManageUrlsWindow, String> {
    let services = store.snapshot().services.clone();
    let mut urls: HashMap<HashId, Vec<String>> = store
        .read(|c| hydrus_store::media::load(c, &services, None, &files))
        .map_err(|e| e.to_string())?
        .results
        .into_iter()
        .map(|m| (m.hash_id, m.urls))
        .collect();
    let file_urls = files
        .iter()
        .map(|f| urls.remove(f).unwrap_or_default())
        .collect();
    let window = ManageUrlsWindow::new().map_err(|e| e.to_string())?;
    window.set_window_title(title(files.len()).into());
    let state = Rc::new(RefCell::new(State {
        editor: UrlsEditor::new(file_urls),
        selection: ListSelection::default(),
        asking: None,
    }));
    let normalise: Rc<Normalise> = Rc::new(normaliser(store));
    let refresh = {
        let state = state.clone();
        let weak = window.as_weak();
        Rc::new(move || {
            if let Some(window) = weak.upgrade() {
                show(&window, &state.borrow());
            }
        })
    };
    let close = {
        let slot = slot.clone();
        let weak = window.as_weak();
        Rc::new(move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        })
    };
    // the box's text into the editor, as typed
    let typed = {
        let state = state.clone();
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().editor.input = window.get_input().to_string();
            }
        }
    };
    let apply = {
        let state = state.clone();
        let store = store.clone();
        let close = close.clone();
        Rc::new(move || {
            let updates = state.borrow().editor.updates().to_vec();
            let files = files.clone();
            let written = store.write_content(move |w| {
                for update in &updates {
                    let hashes: Vec<HashId> = update.files.iter().map(|&i| files[i]).collect();
                    let url = [update.url.clone()];
                    if update.add {
                        w.add_urls(&hashes, &url)?;
                    } else {
                        w.delete_urls(&hashes, &url)?;
                    }
                }
                Ok(())
            });
            match written {
                Ok(()) => {
                    applied();
                    close();
                }
                Err(e) => eprintln!("could not write the urls: {e}"),
            }
        })
    };
    // "apply", asking first with text left in the box
    let ok = {
        let state = state.clone();
        let apply = apply.clone();
        let refresh = refresh.clone();
        let typed = typed.clone();
        Rc::new(move || {
            typed();
            let question = state.borrow().editor.ok_question();
            if question.is_some() {
                state.borrow_mut().asking =
                    Some((Question::TextLeft, TEXT_LEFT_QUESTION.to_owned()));
                refresh();
            } else {
                apply();
            }
        })
    };
    window.on_row_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        move |row, ctrl, shift| {
            let Ok(row) = usize::try_from(row) else {
                return;
            };
            let mut state = state.borrow_mut();
            let order: Vec<usize> = (0..state.editor.rows().len()).collect();
            state.selection.click(&order, row, ctrl, shift);
            let urls: Vec<String> = state
                .selection
                .in_order(&order)
                .into_iter()
                .filter_map(|i| state.url(i))
                .collect();
            state.editor.select(&urls);
            drop(state);
            refresh();
        }
    });
    window.on_row_activated({
        let state = state.clone();
        let refresh = refresh.clone();
        move |row| {
            let mut state = state.borrow_mut();
            let url = usize::try_from(row).ok().and_then(|r| state.url(r));
            if let Some(url) = url {
                state.editor.double_click(&url);
                state.reselect();
            }
            drop(state);
            refresh();
        }
    });
    window.on_delete_pressed({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            let mut state = state.borrow_mut();
            state.editor.delete_selected();
            state.reselect();
            drop(state);
            refresh();
        }
    });
    window.on_entered({
        let state = state.clone();
        let refresh = refresh.clone();
        let normalise = normalise.clone();
        let typed = typed.clone();
        let ok = ok.clone();
        move || {
            typed();
            let mut state = state.borrow_mut();
            let url = std::mem::take(&mut state.editor.input);
            if url.is_empty() {
                drop(state);
                ok();
                return;
            }
            state.enter(vec![url], &*normalise);
            drop(state);
            refresh();
        }
    });
    window.on_copy({
        let state = state.clone();
        move || crate::copy_to_clipboard(&state.borrow().editor.copy_text())
    });
    window.on_paste({
        let state = state.clone();
        let refresh = refresh.clone();
        let normalise = normalise.clone();
        move || {
            let text = match crate::from_clipboard() {
                Ok(text) => text,
                Err(e) => {
                    eprintln!("could not paste: {e}");
                    return;
                }
            };
            let urls: Vec<String> = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect();
            state.borrow_mut().enter(urls, &*normalise);
            refresh();
        }
    });
    window.on_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        let apply = apply.clone();
        move |index| {
            let asked = state.borrow_mut().asking.take();
            match asked {
                Some((Question::Force(urls), _)) if index == 0 => {
                    let mut state = state.borrow_mut();
                    state.editor.enter(&urls, &*normalise, &mut |_| true);
                    state.reselect();
                }
                Some((Question::TextLeft, _)) if index == 0 => {
                    apply();
                    return;
                }
                _ => {}
            }
            refresh();
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
    window.on_apply(move || ok());
    window.on_cancel(move || close());
    window.window().on_close_requested({
        let slot = slot.clone();
        move || {
            slot.borrow_mut().take();
            slint::CloseRequestResponse::HideWindow
        }
    });
    show(&window, &state.borrow());
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
