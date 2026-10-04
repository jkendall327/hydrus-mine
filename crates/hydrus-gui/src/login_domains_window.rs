//! Domain credential drafts and activation, distinct from script-list ownership.
use crate::{LoginDomainsWindow, TableRow};
use hydrus_gui_model::login_workflows::DomainsEditor;
use hydrus_parse::login::Validity;
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
/// Domain window and its credential child, isolated from script test entries.
#[derive(Clone, Default)]
pub struct Slots {
    pub domains: Rc<RefCell<Option<LoginDomainsWindow>>>,
    pub credentials: crate::login_credential_window::CredentialsSlot,
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginDomainSlots")
            .field("open", &self.domains.borrow().is_some())
            .finish_non_exhaustive()
    }
}
impl Slots {
    pub fn cancel(&self) {
        let window = self
            .domains
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(window) = window {
            window.invoke_action("cancel".into());
        }
    }
}
fn show(window: &LoginDomainsWindow, editor: &DomainsEditor) {
    let selected = editor.selection.in_order(&editor.order());
    let rows = editor
        .draft
        .domains
        .iter()
        .map(|(domain, login)| {
            let script = editor.draft.script(login);
            let validity = if login.active {
                if login.validity_error.is_empty() {
                    login.validity.label().to_owned()
                } else {
                    format!("{} - {}", login.validity.label(), login.validity_error)
                }
            } else {
                String::new()
            };
            let delay = if login.no_work_until > jiff::Timestamp::now().as_second() {
                format!("{} - {}", login.no_work_until, login.delay_reason)
            } else {
                String::new()
            };
            let values = vec![
                domain.clone(),
                script.map_or_else(|| "login script not found".to_owned(), |s| s.name.clone()),
                format!("{} - {}", login.access.label(), login.description),
                if login.active { "yes" } else { "no" }.into(),
                validity,
                delay,
            ];
            TableRow {
                cells: ModelRc::new(VecModel::from(
                    values
                        .into_iter()
                        .map(SharedString::from)
                        .collect::<Vec<_>>(),
                )),
                selected: selected.contains(domain),
            }
        })
        .collect::<Vec<_>>();
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    window.set_any_selected(!selected.is_empty());
    window.set_can_edit(
        editor
            .selection
            .one()
            .as_ref()
            .and_then(|domain| editor.draft.domains.get(domain))
            .and_then(|login| editor.draft.script(login))
            .is_some_and(|script| !script.credentials.is_empty()),
    );
}
/// Load preserved/native domain credentials and stage all edits until Apply.
pub fn open(store: &Arc<Store>, slots: &Slots) -> Result<LoginDomainsWindow, String> {
    if let Some(window) = slots.domains.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let window = LoginDomainsWindow::new().map_err(|e| e.to_string())?;
    let editor = Rc::new(RefCell::new(DomainsEditor::new(
        store
            .read(hydrus_store::logins::load)
            .map_err(|e| e.to_string())?,
    )));
    show(&window, &editor.borrow());
    let active = Rc::new(Cell::new(true));
    let pending = Rc::new(RefCell::new(None::<String>));
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(&slots.domains);
        let credentials = slots.credentials.clone();
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            crate::login_credential_window::cancel_credentials(&credentials);
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
            if let Some(window) = weak.upgrade() {
                window.invoke_closed();
            }
        }
    });
    window.on_row_clicked({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        move |i, c, s| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_child_open() || !window.get_question().is_empty() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                let mut editor = editor.borrow_mut();
                let order = editor.order();
                editor.selection.click(&order, i, c, s);
                show(&window, &editor);
            }
        }
    });
    window.on_action({let weak=window.as_weak();let editor=editor.clone();let store=store.clone();let credentials=slots.credentials.clone();let active=active.clone();let close=close.clone();let pending=pending.clone();move|action|{
        if !active.get(){return;}let Some(window)=weak.upgrade()else{return;};if action=="cancel"{close();return;}if window.get_child_open(){return;}if !window.get_question().is_empty()&&!matches!(action.as_str(),"activate"|"leave-inactive"){return;}
        match action.as_str(){
            "credentials"=>{
                let domain=editor.borrow().selection.one();let Some(domain)=domain else{return;};
                let (definitions,values)={let editor=editor.borrow();let login=&editor.draft.domains[&domain];let Some(script)=editor.draft.script(login)else{window.set_error(format!("Could not find a login script for \"{domain}\"! Please re-add the login script in the other dialog or update the entry here to a new one!").into());return;};if script.credentials.is_empty(){return;}(script.credentials.clone(),login.credentials.clone())};
                let applied:crate::login_credential_window::CredentialsApplied=Rc::new({let weak=weak.clone();let editor=editor.clone();let pending=pending.clone();let active=active.clone();move|values|{if !active.get(){return Err("The domain login editor has closed.".into());}let activate=editor.borrow_mut().replace_credentials(&domain,values)?;if activate{*pending.borrow_mut()=Some(domain.clone());}if let Some(window)=weak.upgrade(){show(&window,&editor.borrow());if activate{window.set_question("Activate this login script for this domain?".into());}}Ok(())}});
                match crate::login_credential_window::open_credentials(&definitions,&values,&credentials,applied){Ok(child)=>{window.set_child_open(true);let weak=weak.clone();child.on_closed(move||{if let Some(window)=weak.upgrade(){window.set_child_open(false);}});},Err(error)=>window.set_error(error.to_string().into())}
            },
            "activate"|"leave-inactive"=>{if let Some(domain)=pending.borrow_mut().take(){if action=="activate" && let Some(login)=editor.borrow_mut().draft.domains.get_mut(&domain){login.active=true;}}window.set_question("".into());show(&window,&editor.borrow());},
            "flip-active"|"scrub-delays"|"scrub-invalidity"=>{let mut editor=editor.borrow_mut();let selected=editor.selection.in_order(&editor.order());for domain in selected{
                if action=="scrub-invalidity"{let login=&editor.draft.domains[&domain];if !login.active||login.validity!=Validity::Invalid{continue;}let Some(script)=editor.draft.script(login)else{continue;};let result=script.check_credentials_for_entry(&login.credentials);let login=editor.draft.domains.get_mut(&domain).expect("selected domain");match result{Ok(())=>{login.validity=Validity::Untested;login.validity_error.clear();},Err(error)=>{login.validity_error=error;}}}
                else if let Some(login)=editor.draft.domains.get_mut(&domain){if action=="flip-active"{login.active=!login.active;}else{login.no_work_until=0;login.delay_reason.clear();}}
            }show(&window,&editor);},
            "apply"=>match editor.borrow().save(&store){Ok(())=>close(),Err(error)=>window.set_error(error.to_string().into())},
            _=>{},
        }
    }});
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show().map_err(|e| e.to_string())?;
    *slots.domains.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
