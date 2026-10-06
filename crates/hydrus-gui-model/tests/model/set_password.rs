//! Database > set a password against `oracle/record_set_password.py`.
use hydrus_core::lock::LockPassword;
use hydrus_gui_model::set_password::{SetPassword, Step};

/// A run's name, entered texts (none: cancel) and clear answer.
type Script = (&'static str, &'static [Option<&'static str>], Option<bool>);

#[test]
fn scripted_answers_ask_and_save_as_the_reference() {
    let fixture = hydrus_testkit::fixture_json("set_password.json");
    let scripts: [Script; 6] = [
        ("cancel", &[None], None),
        ("blank, keep", &[Some("")], Some(false)),
        ("blank, clear", &[Some("")], Some(true)),
        ("matching", &[Some("hunter2"), Some("hunter2")], None),
        ("mismatched", &[Some("hunter2"), Some("hunter3")], None),
        ("second cancelled", &[Some("hunter2"), None], None),
    ];
    let before = fixture["before_hash"].as_str().unwrap().to_owned();
    for (event, (name, texts, yes)) in fixture["events"].as_array().unwrap().iter().zip(scripts) {
        assert_eq!(event["name"], name);
        let mut flow = SetPassword::default();
        let mut asked = Vec::new();
        let mut saved = LockPassword {
            sha256: Some(before.clone()),
        };
        let mut step = flow.start();
        let mut texts = texts.iter();
        loop {
            match step {
                Step::Ask {
                    message,
                    allow_blank,
                } => {
                    asked.push(("text", message.to_owned(), Some(allow_blank)));
                    match texts.next().unwrap() {
                        Some(text) => step = flow.entered(text),
                        None => break,
                    }
                }
                Step::ConfirmClear => {
                    asked.push(("yes_no", "Clear any existing password?".into(), None));
                    step = flow.answered(yes.unwrap());
                }
                Step::Problem { title, message } => {
                    assert_eq!(title, "Problem!");
                    asked.push(("critical", message.to_owned(), None));
                    break;
                }
                Step::Save(lock) => {
                    saved = lock;
                    break;
                }
                Step::Done => break,
            }
        }
        let recorded: Vec<_> = event["asked"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                let kind = match a["kind"].as_str().unwrap() {
                    "text" => "text",
                    "yes_no" => "yes_no",
                    _ => "critical",
                };
                (
                    kind,
                    a["message"].as_str().unwrap().to_owned(),
                    a["allow_blank"].as_bool(),
                )
            })
            .collect();
        assert_eq!(asked, recorded, "{name}");
        assert_eq!(saved.sha256.as_deref(), event["stored"].as_str(), "{name}");
    }
}
