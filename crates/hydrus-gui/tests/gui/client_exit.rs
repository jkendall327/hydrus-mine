//! Exit timeouts and retained close callbacks belong to their main binding.
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::settings::{self, GuiSettings};
use slint::ComponentHandle as _;

fn request_close(ui: &MainWindow) {
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
}

#[test]
fn cancel_hidden_answer_and_accepted_close_respect_the_live_main_binding() {
    let (_directories, store) = crate::subscriptions::store();
    store
        .write(|ctx| {
            let mut preferences: GuiSettings = settings::get(ctx.conn())?;
            preferences.confirm_exit = true;
            settings::set(ctx.conn(), &preferences)
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let old = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    request_close(&ui);
    let question = ui.get_question();
    assert!(question.starts_with("Are you sure you want to exit the client?"));

    let current = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    // Full rebinding drops the old timer. The private fast held-timer test
    // separately covers queued replies; this checks actual main-window owners.
    request_close(&ui);
    assert_eq!(ui.get_question(), question);
    slint::platform::update_timers_and_animations();
    assert!(ui.window().is_visible(), "rebinding must leave B live");
    assert_eq!(
        ui.get_question(),
        question,
        "B still owns its pending question"
    );

    ui.invoke_answer(false);
    assert!(ui.window().is_visible());
    assert!(ui.get_question().is_empty());
    request_close(&ui);
    assert_eq!(ui.get_question(), question, "Cancel keeps B live");
    // An accepted answer retained while hidden cannot retire the visible owner.
    ui.hide().unwrap();
    ui.invoke_answer(true);
    ui.show().unwrap();
    request_close(&ui);
    assert_eq!(ui.get_question(), question);
    ui.invoke_answer(true);
    assert!(!ui.window().is_visible());

    // Accepted close is permanent even when the component and Bound survive.
    ui.show().unwrap();
    request_close(&ui);
    assert!(ui.window().is_visible());
    assert!(ui.get_question().is_empty());
    ui.invoke_answer(true);
    assert!(ui.window().is_visible());
    let reopened = bind(&ui, Pages::single(SearchPage::new(store)));
    request_close(&ui);
    assert_eq!(ui.get_question(), question, "a fresh binding can ask again");
    ui.invoke_answer(false);
    drop((old, current, reopened));
}
