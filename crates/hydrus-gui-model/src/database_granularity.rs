//! Database > locations > "manage granularity" (`ReviewGranularityPanel`):
//! what it says, asks and reports.

use hydrus_core::numbers::human_int;
use hydrus_core::time::pretty_time_delta;

pub const TITLE: &str = "manage granularity";

pub const WARNING: &str = "THIS IS SERIOUSLY ONLY FOR ADVANCED USERS. THE MIGRATION IS BIG AND SLOW, SO USERS WITH >1m FILES SHOULD THINK CAREFULLY.\n\nDO NOT DO THIS IF YOU DO SYMLINK MAGIC INSIDE YOUR FILE STORAGE";

pub const INTRO: &str = "The client uses many subfolders to split up your collection into manageable chunks. As clients have grown, we are pushing from a granularity of 2, which means 512 subfolders, to 3, which means 8,192.\n\nMigrating to a higher granularity requires running a job here and then repeating it on your backups.";

pub const OFFLINE: &str = "If you have a backup from when you were granularity 2, updating it from your new 3 will be hellish expensive because all the files have moved. Click the button here to granularise your backup, too, so it has the same folder structure as your client. Trust me, this will save lots of time.";

const SPEED: &str = "Granularisation migration works at about this speed:\n\nNVME/SSD: 3,000-5,000 files/s\nSATA/USB HDD: 100-500 files/s\nNAS/SMB: 50-250 files/s\nCloud storage: should not be attempted\nBTRFS and EXT4 filesystems may be up to 10x this speed.\n\n";

/// "this client"'s line.
pub fn granularity_label(granularity: usize) -> String {
    match granularity {
        2 => "Your granularity is currently 2. If you are an advanced user, you are invited to move to 3.".into(),
        3 => "Your granularity is currently 3. How is it going?".into(),
        other => format!(
            "Your granularity is \"{other}\", which is undefined! Close the client and contact hydev immediately."
        ),
    }
}

/// The client migration's question: title, message, yes and no labels.
pub fn client_question(from: usize) -> (&'static str, String, &'static str, &'static str) {
    let mut message = if from == 2 {
        "We are going to be rearranging your file storage completely. The process can be cancelled if it is taking too long, but it has to do the same amount of work to undo. If it fails half way through, I will attempt to undo it.\n\n".to_owned()
    } else {
        "We are going to be rearranging your file storage completely, restoring it to how a client starts, granularity 2. The process can be cancelled if it is taking too long, but it has to do the same amount of work to undo. If it fails half way through, I will attempt to undo it.\n\nIf your client has done a bunch of multi-folder file migration since you moved to granularity 3, this job will require additional time to shuffle things around!".to_owned()
    };
    message
        .push_str(" Ideally, your client is currently calm and not trying to import many things. ");
    message.push_str(SPEED);
    message.push_str("\n\nDo you have a recent backup, in case something goes wrong?");
    (
        "One last check!",
        message,
        "yes, I have a backup and I am ready",
        "no, I am not ready",
    )
}

/// The client migration's popup title.
pub fn client_title(from: usize, to: usize) -> String {
    format!("Granularising Client File Storage {from} to {to}")
}

pub const CANCELLED: &str = "The job was cancelled by you and I think everything was undone ok.";

/// The client migration's error report.
pub fn client_error(error: &str) -> String {
    format!(
        "Something went wrong with the granularisation!! This is bad news. If this is the second time you are seeing this error, the database should have been able to undo the work successfully and you can return to normal work.\n\nIf this is the first error you saw, or the error has changed from the first time, then we have had a more serious problem. You should probably contact hydev regardless.\n\nThe error, which has been written in more detail to log, was:\n\n{error}"
    )
}

fn weird(dirs: usize, files: usize) -> String {
    format!(
        "By the way, you had {} weird directories and {} weird files in your storage. They were not touched, but everything has been logged, so check out your log and see what that old cruft is. It is probably something like OS-level thumbnail metadata; not a big deal.",
        human_int(dirs as u64),
        human_int(files as u64)
    )
}

fn moved(files: usize, seconds: i64) -> String {
    format!(
        "{} files moved in {}.",
        human_int(files as u64),
        pretty_time_delta(seconds, false)
    )
}

/// The client migration's report.
pub fn client_done(
    from: usize,
    to: usize,
    files: usize,
    seconds: i64,
    weird_dirs: usize,
    weird_files: usize,
) -> String {
    let desc = if from < to {
        "You now have finer, lower-latency file storage."
    } else {
        "You now have simpler file storage."
    };
    let mut message = format!(
        "{}\n\nEverything went great. {desc} The next step is to repeat this process for your backup file storage folders. Let hydev know if you have any problems!",
        moved(files, seconds)
    );
    if weird_dirs > 0 || weird_files > 0 {
        message.push_str("\n\n");
        message.push_str(&weird(weird_dirs, weird_files));
    }
    message
}

/// The offline folder migration's first question: title, message, labels.
pub fn folder_ready() -> (&'static str, String, &'static str, &'static str) {
    (
        "Ready check!",
        format!(
            "We are going to be rearranging the file storage of your selected folder completely.{SPEED}\n\nWhen I ask which folder to work on, you want to select the one that _contains_ stuff like f86 or t32, often called \"client_files\" under a \"db\" dir. I will scan it beforehand to make sure it looks correct.\n\nAre you ready?"
        ),
        "yes, I am ready",
        "no, I am not ready",
    )
}

pub const PICK_FOLDER: &str = "Select folder to regranularise.";

/// What the folder's scan says: title and question.
pub fn folder_check(estimated: Option<usize>, from: usize, to: usize) -> (&'static str, String) {
    match estimated {
        None => (
            "Unable to determine current granularity.",
            "I examined the folder but I am not sure what I am looking at. Are you sure this is correct?".into(),
        ),
        Some(found) if found == from => ("Looking good.", "Everything looks good. Start?".into()),
        Some(found) if found == to => (
            "Proceed at your own risk.",
            format!("I examined the folder, but I think it is already granularity {to}. Maybe it is a mixed folder? Are you sure this is correct?"),
        ),
        Some(found) => (
            "Proceed at your own risk.",
            format!("I examined the folder, but I think its current granularity is {found}! I do not know what is going on. Are you sure this is correct?"),
        ),
    }
}

pub const FOLDER_TITLE: &str = "Regranularising a Client File Storage Folder";

pub const FOLDER_CANCELLED: &str = "You cancelled the job. The folder is now likely in a mixed state, so you will want to re-do the job either way to complete or undo it.";

pub fn folder_error(error: &str) -> String {
    format!(
        "Something went wrong with the granularisation!! This may be bad news, but if it stopped half way through and you know how to fix the issue (maybe an errant file called \"a\" that you should move out?), you can just retry it and it will try to continue where it left off. The error, which has been written in more detail to log, was:\n\n{error}"
    )
}

/// The offline folder migration's report.
pub fn folder_done(
    to: usize,
    files: usize,
    seconds: i64,
    weird_dirs: usize,
    weird_files: usize,
) -> String {
    if weird_dirs == 0 && weird_files == 0 {
        format!(
            "{}\n\nEverything went great. Your folder has been granularised to level {to}.",
            moved(files, seconds)
        )
    } else {
        format!(
            "{}\n\nThings went great. Your folder has been granularised to level {to}.\n\n{}",
            moved(files, seconds),
            weird(weird_dirs, weird_files)
        )
    }
}
