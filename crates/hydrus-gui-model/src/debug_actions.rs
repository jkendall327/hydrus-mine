//! Help > debug's actions that hydrus-rs has (`ClientGUI`'s debug menu):
//! "make some popups", the delayed modal popups, "make a QMessageBox",
//! profiling's "what is this?", "show env" and "reset multi-column list
//! settings to default"'s question.

use hydrus_core::Sha256;
use hydrus_store::popups::Job;

/// Profiling > "what is this?"'s information.
pub const PROFILE_MESSAGE: &str = "If something is running slow, you can turn on a profile mode to have hydrus gather information on it. You probably want \"db\" profile mode, but if it seems to be lag related to dialog spawning or similar, you might like to try the \"ui\" mode.\n\nTurn the mode on, do the slow thing for a bit, and then turn it off. In your database directory will be a new profile log, which is really helpful for hydrus dev to figure out what is running slow for you and how to fix it.\n\nThe Query Planner mode makes detailed database analysis of specific database queries. This is sometimes useful to hydev, but he will usually ask for it specifically.\n\nMore information is available in the help, under 'reducing lag'.";

/// "make a QMessageBox"'s warning.
pub const MESSAGE_BOX_TEXT: &str = "This is a test message!\n\nI have a second line of information to give! I will repeat it! I have a second line of information to give! I will repeat it! I have a second line of information to give! I will repeat it! I have a second line of information to give! I will repeat it! I have a second line of information to give! I will repeat it!";

/// "reset multi-column list settings to default"'s question.
pub const RESET_COLUMNS_QUESTION: &str = "This will reset all saved column widths for all multi-column lists across the program. You may need to restart the client to see changes.";

/// "flush log"'s line.
pub const FLUSH_LOG: &str = "Flushing log";

/// The long message "make some popups" shows.
const LONG_MESSAGE: &str = "++++What the fuck did you just fucking say about me, you worthless heretic? I'll have you know I graduated top of my aspirant tournament in the Heralds of Ultramar, and I've led an endless crusade of secret raids against the forces of The Great Enemy, and I have over 30 million confirmed purgings. I am trained in armored warfare and I'm the top brother in all the 8th Company. You are nothing to me but just another heretic. I will wipe you the fuck out with precision the likes of which has never been seen before in this Galaxy, mark my fucking words. You think you can get away with saying that shit to me over the Divine Astropathic Network? Think again, traitor. As we speak I am contacting my secret network of inquisitors across the galaxy and your malign powers are being traced right now so you better prepare for the holy storm, maggot. The storm that wipes out the pathetic little thing you call your soul. You're fucking dead, kid. I can transit the immaterium to anywhere, anytime, and I can kill you in over seven hundred ways, and that's just with my purity seals. Not only am I extensively trained in unarmed combat, but I have access to the entire arsenal of the Departmento Munitorum and I will use it to its full extent to wipe your miserable ass off the face of the galaxy, you little shit. If only you could have known what holy retribution your little \"clever\" comment was about to bring down upon you, maybe you would have held your fucking impure mutant tongue. But you couldn't, you didn't, and now you're paying the price, you Emperor-damned heretic.++++\n\n++++Better crippled in body than corrupt in mind++++\n\n++++The Emperor Protects++++";

/// The file "make some popups"' client api test popup shows.
const TEST_FILE: &str = "78f92ba4a786225ee2a1236efa6b7dc81dd729faf4af99f96f3e20bad6d8b538";

/// The popups "make some popups" shows at once (`_DebugMakeSomePopups`),
/// at `now` (seconds), with `random` keys for its made-up files. Its
/// buttons that call back (the user call, auto-account and gap downloader
/// tests) and its network-job popup are left out.
pub fn some_popups(now: f64, mut random: impl FnMut() -> Sha256) -> Vec<Job> {
    let text = |t: &str| Job::text(t, now);
    let mut out: Vec<Job> = (1..7)
        .map(|i| text(&format!("This is a test popup message -- {i}")))
        .collect();
    out.push(text(&format!(
        "This is a very long message:  \n\n{LONG_MESSAGE}"
    )));
    let mut job = text("test");
    job.status_title = Some(
        "This popup has a very long title -- it is a subscription that is running with a long \"artist sub 123456\" kind of name".into(),
    );
    out.push(job);
    let mut job = text("client api test file popup");
    if let Ok(hash) = TEST_FILE.parse() {
        job.set_files(vec![hash], Some("go".into()));
    }
    out.push(job);
    let first = random();
    let mut job = text("hey I should have five files");
    job.status_title = Some("Popup file merge test".into());
    job.attached_files_mergable = true;
    job.set_files(vec![first, random(), random()], Some("cool pics".into()));
    out.push(job);
    let mut job = text("hey you should not see me, I should be merged");
    job.status_title = Some("Popup file merge test".into());
    job.attached_files_mergable = true;
    job.set_files(vec![random(), random(), first], Some("cool pics".into()));
    out.push(job);
    let mut job = text("\u{24b2}\u{24a0}\u{24b2} \u{24a7}\u{249c}\u{249f}");
    job.status_title = Some("\u{24c9}\u{24d7}\u{24d8}\u{24e2} \u{24d8}\u{24e2} \u{24d0} \u{24e3}\u{24d4}\u{24e2}\u{24e3} \u{24e4}\u{24dd}\u{24d8}\u{24d2}\u{24de}\u{24d3}\u{24d4} \u{24dc}\u{24d4}\u{24e2}\u{24e2}\u{24d0}\u{24d6}\u{24d4}".into());
    job.status_text_2 = Some("p\u{250}\u{5df} \u{28d}\u{1dd}\u{28d}".into());
    out.push(job);
    let mut job = Job::new(true, true, now);
    job.status_title = Some("test job".into());
    job.status_text_1 = Some("Currently processing test job 5/8".into());
    job.popup_gauge_1 = Some((4, 8));
    out.push(job);
    let mut job = text("This is a test exception");
    job.status_title = Some("DataMissing".into());
    job.had_error = true;
    job.traceback = Some("DataMissing: This is a test exception".into());
    out.push(job);
    out
}

/// The popups "make some popups" shows after half a second, a second and
/// a second and a half.
pub fn delayed_popup_text(i: usize) -> String {
    format!("This is a delayed popup message -- {i}")
}

/// The delayed modal popup's title, and its text `i` seconds in (of ten).
pub const MODAL_TITLE: &str = "debug modal job";
pub fn modal_text(i: i64) -> String {
    format!(
        "Will auto-dismiss in {}.",
        hydrus_core::time::pretty_time_delta(10 - i, false)
    )
}

/// "show env"'s popup (`HydrusEnvironment.DumpEnv`): every variable by
/// name, path-like ones (names with PATH or DIRS holding `separator`) one
/// entry per line.
pub fn env_text(vars: impl IntoIterator<Item = (String, String)>, separator: char) -> String {
    let mut vars: Vec<(String, String)> = vars.into_iter().collect();
    vars.sort();
    let mut rows = Vec::new();
    for (key, value) in vars {
        if (key.contains("PATH") || key.contains("DIRS")) && value.contains(separator) {
            rows.push(format!("{key}:"));
            rows.extend(value.split(separator).map(|v| format!("    {v}")));
        } else {
            rows.push(format!("{key}: {value}"));
        }
    }
    format!("Full environment:\n{}", rows.join("\n"))
}

/// A Help > debug entry hydrus-rs runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// profiling > "what is this?"
    ProfileInfo,
    /// "make a modal popup in five seconds" (cancellable) and its
    /// non-cancellable twin.
    ModalPopup {
        cancellable: bool,
    },
    MessageBox,
    SomePopups,
    ResetColumns,
    SaveLastSession,
    FlushLog,
    ForceCommit,
    ShowEnv,
    /// "simulate program exit signal"
    Exit,
    ClearRenderingCaches,
}
