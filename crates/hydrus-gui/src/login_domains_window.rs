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
    pub run: crate::login_test_window::RunSlot,
    pub entry: crate::login_domain_entry::Slots,
    pub status: Rc<RefCell<String>>,
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
        self.run.stop();
    }
}
fn start_next(
    run: &crate::login_test_window::RunSlot,
    queue: Rc<RefCell<std::collections::VecDeque<crate::login_test_window::Input>>>,
    status: Rc<RefCell<String>>,
) {
    let Some(input) = queue.borrow_mut().pop_front() else {
        return;
    };
    let domain = input.domain.clone();
    let key = input.script.key.clone();
    let store = input.source.clone();
    let progress: crate::login_test_window::Progress = Rc::new({
        let status = status.clone();
        let domain = domain.clone();
        move |text| *status.borrow_mut() = format!("{domain}: {text}")
    });
    let completed: crate::login_test_window::Completed = Rc::new({
        let run = run.clone();
        move |execution| {
            let outcome = execution.outcome;
            *status.borrow_mut() = format!("{domain}: {}", outcome.text());
            let now = jiff::Timestamp::now().as_second();
            let saved = store.write_and_refresh({
                let domain = domain.clone();
                let key = key.clone();
                let outcome = outcome.clone();
                move |ctx| {
                    let mut manager = hydrus_store::logins::load(ctx.conn())?;
                    if let Some(login) = manager.domains.get_mut(&domain)
                        && outcome.update_domain(login, &key, now)
                    {
                        hydrus_store::logins::save(ctx.conn(), &manager)?;
                    }
                    Ok(())
                }
            });
            if let Err(error) = saved {
                *status.borrow_mut() = error.to_string();
                return;
            }
            if outcome != hydrus_net::login::Outcome::Cancelled {
                start_next(&run, queue.clone(), status.clone());
            }
        }
    });
    run.start(input, progress, completed);
}
fn show(window: &LoginDomainsWindow, editor: &DomainsEditor, store: &Store) {
    let selected = editor.selected_domains();
    let now = jiff::Timestamp::now().as_second();
    let rows = editor
        .draft
        .domains
        .iter()
        .map(|(domain, login)| {
            let script = editor.draft.script(login);
            let state = script
                .and_then(|script| hydrus_net::login::session_state(store, script, domain).ok());
            let cells = hydrus_gui_model::login_workflows::domain_cells(
                domain,
                login,
                script,
                state.is_some_and(|state| state.logged_in),
                state.and_then(|state| state.expires),
                now,
            );
            TableRow {
                cells: ModelRc::new(VecModel::from(
                    cells
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
            .selected_domain()
            .as_ref()
            .and_then(|domain| editor.draft.domains.get(domain))
            .and_then(|login| editor.draft.script(login))
            .is_some_and(|script| !script.credentials.is_empty()),
    );
    window.set_can_scrub_delays(selected.iter().any(|domain| {
        let login = &editor.draft.domains[domain];
        login.no_work_until > now || !login.delay_reason.is_empty()
    }));
    window.set_can_scrub_invalidity(selected.iter().any(|domain| {
        let login = &editor.draft.domains[domain];
        login.active && login.validity == Validity::Invalid
    }));
    window.set_can_do_login(selected.iter().any(|domain| {
        let login = &editor.draft.domains[domain];
        login.active
            && login.validity != Validity::Invalid
            && editor.draft.script(login).is_some_and(|script| {
                hydrus_net::login::logged_in(store, script, domain).is_ok_and(|logged| !logged)
            })
    }));
}
type ProcessSlot = Rc<RefCell<Option<(String, u64)>>>;
fn monitor_process(
    window: &LoginDomainsWindow,
    run: &crate::login_test_window::RunSlot,
    status: &RefCell<String>,
    store: &Store,
    process: &RefCell<Option<(String, u64)>>,
) {
    let active = hydrus_store::login_runtime::current(store)
        .ok()
        .flatten()
        .map(|login| (login.epoch.clone(), login));
    if !run.busy()
        && let Some((epoch, login)) = active
    {
        window.set_running(true);
        window.set_status(format!("Logging in {} — {}", login.domain, login.status).into());
        *process.borrow_mut() = Some((epoch, login.id));
    } else {
        process.borrow_mut().take();
        window.set_running(run.busy());
        window.set_status(status.borrow().as_str().into());
    }
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
    show(&window, &editor.borrow(), store);
    let active = Rc::new(Cell::new(true));
    let pending = Rc::new(RefCell::new(None::<String>));
    let attempts = Rc::new(RefCell::new(Vec::<crate::login_test_window::Input>::new()));
    let resetting = Rc::new(RefCell::new(Vec::<String>::new()));
    let process = ProcessSlot::default();
    monitor_process(&window, &slots.run, &slots.status, store, &process);
    let timer = Rc::new(slint::Timer::default());
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(50),
        {
            let weak = window.as_weak();
            let run = slots.run.clone();
            let status = slots.status.clone();
            let editor = editor.clone();
            let store = store.clone();
            let ticks = Cell::new(0_u8);
            let process = process.clone();
            move || {
                if let Some(window) = weak.upgrade() {
                    if run.busy() {
                        window.set_running(true);
                        window.set_status(status.borrow().as_str().into());
                    }
                    ticks.set(ticks.get() + 1);
                    if ticks.get() == 20 {
                        ticks.set(0);
                        monitor_process(&window, &run, &status, &store, &process);
                        show(&window, &editor.borrow(), &store);
                    }
                }
            }
        },
    );
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(&slots.domains);
        let credentials = slots.credentials.clone();
        let entry = slots.entry.clone();
        let active = active.clone();
        let timer = timer.clone();
        move || {
            timer.stop();
            if !active.replace(false) {
                return;
            }
            crate::login_credential_window::cancel_credentials(&credentials);
            entry.cancel();
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
        let store = store.clone();
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
                show(&window, &editor, &store);
            }
        }
    });
    window.on_action({let weak=window.as_weak();let editor=editor.clone();let store=store.clone();let credentials=slots.credentials.clone();let active=active.clone();let close=close.clone();let pending=pending.clone();let attempts=attempts.clone();let resetting=resetting.clone();let run=slots.run.clone();let status=slots.status.clone();let entry=slots.entry.clone();let process=process.clone();move|action|{
        if !active.get(){return;}let Some(window)=weak.upgrade()else{return;};
        if action=="cancel"{close();return;}
        if action=="cancel-login"{
            if run.busy(){run.cancel();}
            else if let Some((epoch, id))=process.borrow().clone(){
                if let Err(error)=hydrus_store::login_runtime::cancel(&store, epoch, id){window.set_error(error.to_string().into());}
            }
            return;
        }
        if window.get_running(){return;}
        if window.get_child_open(){return;}
        if !window.get_question().is_empty()&&!matches!(action.as_str(),"activate"|"leave-inactive"|"confirm-login"|"back-login"|"confirm-reset"|"back-reset"|"confirm-delete"|"back-delete"){return;}
        match action.as_str(){
            "add"|"change-script"=>{
                let editing=if action=="change-script"{editor.borrow().selected_domains().into_iter().next()}else{None};
                if action=="change-script"&&editing.is_none(){return;}
                let applied:crate::login_domain_entry::Applied=Rc::new({let editor=editor.clone();let active=active.clone();let weak=weak.clone();let store=store.clone();move|domain,login|{
                    if !active.get(){return Err("The domain login editor has closed.".into());}
                    editor.borrow_mut().put(domain,login);
                    if let Some(window)=weak.upgrade(){show(&window,&editor.borrow(),&store);}
                    Ok(())
                }});
                let report:crate::login_domain_entry::Report=Rc::new({let weak=weak.clone();let active=active.clone();move|error|{if active.get()&&let Some(window)=weak.upgrade(){window.set_error(error.into());}}});
                let manager=editor.borrow().draft.clone();
                match crate::login_domain_entry::open(&manager,editing.as_deref(),&entry,applied,report){
                    Ok(child)=>{window.set_child_open(true);let weak=weak.clone();child.on_closed(move||{if let Some(window)=weak.upgrade(){window.set_child_open(false);}});},
                    Err(error)=>window.set_error(error.into()),
                }
            },
            "delete"=>{
                if editor.borrow().selected_domains().is_empty(){return;}
                window.set_confirming_delete(true);window.set_question("Remove all selected?".into());
            },
            "confirm-delete"|"back-delete"=>{
                if !window.get_confirming_delete(){return;}
                if action=="confirm-delete"{editor.borrow_mut().delete();}
                window.set_confirming_delete(false);window.set_question("".into());show(&window,&editor.borrow(),&store);
            },
            "reset-login"=>{
                let domains=editor.borrow().selected_domains();
                if domains.is_empty(){return;}
                *resetting.borrow_mut()=domains;
                window.set_confirming_reset(true);
                window.set_question("Are you sure you want to clear these domains' sessions? This will delete all their existing cookies and cannot be undone.".into());
            },
            "confirm-reset"|"back-reset"=>{
                let domains=std::mem::take(&mut *resetting.borrow_mut());
                if action=="confirm-reset"&&!domains.is_empty(){
                    if let Err(error)=hydrus_net::login::clear_sessions(&store,&domains){window.set_error(error.into());}
                }
                window.set_confirming_reset(false);window.set_question("".into());
                show(&window,&editor.borrow(),&store);
            },
            "do-login"=>{
                let eligible = {
                    let editor=editor.borrow();
                    let selected=editor.selected_domains();
                    let mut eligible=Vec::new();
                    for domain in selected {
                        let login=&editor.draft.domains[&domain];
                        if !login.active || login.validity==Validity::Invalid { continue; }
                        let Some(script)=editor.draft.script(login) else { continue; };
                        match hydrus_net::login::logged_in(&store,script,&domain) {
                            Ok(true)=>continue,
                            Err(error)=>{window.set_error(error.into());return;},
                            Ok(false)=>{}
                        }
                        eligible.push(crate::login_test_window::Input { source:store.clone(),script:script.clone(),domain,credentials:login.credentials.clone(),test:false });
                    }
                    eligible
                };
                if eligible.is_empty() { window.set_error("Unfortunately, none of the selected domains appear able to log in. Do you need to activate or scrub something somewhere?".into());return; }
                let names=eligible.iter().map(|input| input.domain.as_str()).collect::<Vec<_>>().join("\n");
                *attempts.borrow_mut()=eligible;
                window.set_confirming_login(true);
                window.set_question(format!("It looks like the following domains can log in:\n\n{names}\n\nThe dialog will ok and the login attempts will start. Is this ok?").into());
            },
            "back-login"=>{attempts.borrow_mut().clear();window.set_confirming_login(false);window.set_question("".into());},
            "confirm-login"=>{
                if attempts.borrow().is_empty(){return;}
                if let Err(error) = editor.borrow().save(&store) {
                    window.set_error(error.to_string().into());
                } else {
                    let inputs=std::mem::take(&mut *attempts.borrow_mut());
                    close();
                    *status.borrow_mut()="starting login attempts".into();
                    start_next(&run,Rc::new(RefCell::new(inputs.into())),status.clone());
                }
            },
            "credentials"=>{
                let domain=editor.borrow().selected_domain();let Some(domain)=domain else{return;};
                let (definitions,values)={let editor=editor.borrow();let login=&editor.draft.domains[&domain];let Some(script)=editor.draft.script(login)else{window.set_error(format!("Could not find a login script for \"{domain}\"! Please re-add the login script in the other dialog or update the entry here to a new one!").into());return;};
        if script.credentials.is_empty(){return;}(script.credentials.clone(),login.credentials.clone())};
                let applied:crate::login_credential_window::CredentialsApplied=Rc::new({let weak=weak.clone();let editor=editor.clone();let pending=pending.clone();let active=active.clone();let store=store.clone();move|values|{if !active.get(){return Err("The domain login editor has closed.".into());}let activate=editor.borrow_mut().replace_credentials(&domain,values)?;
        if activate{*pending.borrow_mut()=Some(domain.clone());}
        if let Some(window)=weak.upgrade(){show(&window,&editor.borrow(),&store);
        if activate{window.set_question("Activate this login script for this domain?".into());}}Ok(())}});
                match crate::login_credential_window::open_credentials(&definitions,&values,&credentials,applied){Ok(child)=>{window.set_child_open(true);let weak=weak.clone();child.on_closed(move||{if let Some(window)=weak.upgrade(){window.set_child_open(false);}});},Err(error)=>window.set_error(error.to_string().into())}
            },
            "activate"|"leave-inactive"=>{
                let domain = pending.borrow_mut().take();
                if let Some(domain) = domain && action == "activate"
                    && let Some(login) = editor.borrow_mut().draft.domains.get_mut(&domain) {
                    login.active = true;
                }
                window.set_question("".into());show(&window,&editor.borrow(),&store);
            },
            "flip-active"|"scrub-delays"|"scrub-invalidity"=>{let mut editor=editor.borrow_mut();let selected=editor.selected_domains();for domain in selected{
                if action=="scrub-invalidity"{let login=&editor.draft.domains[&domain];
        if !login.active||login.validity!=Validity::Invalid{continue;}let Some(script)=editor.draft.script(login)else{continue;};let result=script.check_credentials_for_entry(&login.credentials);let login=editor.draft.domains.get_mut(&domain).expect("selected domain");match result{Ok(())=>{login.validity=Validity::Untested;login.validity_error.clear();},Err(error)=>{login.validity_error=error;}}}
                else if let Some(login)=editor.draft.domains.get_mut(&domain){if action=="flip-active"{login.active = !login.active;}else{login.no_work_until=0;login.delay_reason.clear();}}
            }show(&window,&editor,&store);},
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
