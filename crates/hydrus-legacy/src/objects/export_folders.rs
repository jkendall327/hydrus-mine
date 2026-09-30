//! Export folders (16, v1-9), upgraded as the reference does.

use hydrus_parse::sidecar::Router;

use super::domain::expect;
use super::favourites::FileSearchContext;
use super::util::{DecodeResult, boolean, int, list, malformed, nested, string};
use crate::serialisable::{SerialisableObject, SerialisableType};

const EXPORT_FOLDER: SerialisableType = SerialisableType(16);

/// A stored export folder; its search is as stored (the caller converts it).
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyExportFolder {
    pub name: String,
    pub path: String,
    /// `HC.EXPORT_FOLDER_TYPE_*`: 0 regular, 1 synchronise.
    pub export_type: i64,
    pub delete_from_client_after_export: bool,
    pub export_symlinks: bool,
    pub search: FileSearchContext,
    pub routers: Vec<Router>,
    pub run_regularly: bool,
    pub period: i64,
    pub phrase: String,
    pub last_checked: i64,
    pub run_now: bool,
    pub last_error: String,
    pub show_working_popup: bool,
    pub overwrite_sidecars_on_next_run: bool,
    pub always_overwrite_sidecars: bool,
}

/// An export folder (16, v1-9).
pub fn export_folder(object: &SerialisableObject) -> DecodeResult<LegacyExportFolder> {
    let k = EXPORT_FOLDER;
    expect(object, k, &[1, 2, 3, 4, 5, 6, 7, 8, 9])?;
    let name = object
        .name
        .clone()
        .ok_or_else(|| malformed(k, "export folder has no name"))?;
    let v = object.version;
    let info = object.info();
    let items = list(k, &info, "export folder")?;
    // v2 the path, v3 deleting after export, v4 the schedule, v5 the last
    // error, v6 routers, v7 symlinks; v8 swapped "paused" for the popup
    // setting; v9 the sidecar overwrite settings
    let expected_len = match v {
        1 => 5,
        2 => 6,
        3 => 7,
        4 => 10,
        5 => 11,
        6 => 12,
        7 | 8 => 13,
        _ => 15,
    };
    if items.len() != expected_len {
        return Err(malformed(
            k,
            format!(
                "a v{v} export folder should have {expected_len} items, found {}",
                items.len()
            ),
        ));
    }
    let mut it = items.iter();
    let mut next = || it.next().expect("length checked");
    // v2 took the path from the name
    let path = if v >= 2 {
        string(k, next(), "path")?
    } else {
        name.clone()
    };
    let export_type = int(k, next(), "export type")?;
    let delete_from_client_after_export = if v >= 3 {
        boolean(k, next(), "delete from client after export")?
    } else {
        false
    };
    let export_symlinks = if v >= 7 {
        boolean(k, next(), "export symlinks")?
    } else {
        false
    };
    let search = FileSearchContext::from_object(&nested(k, next(), "file search context")?)?;
    let routers = if v >= 6 {
        super::sidecars::routers(k, next())?
    } else {
        Vec::new()
    };
    let mut run_regularly = if v >= 4 {
        boolean(k, next(), "run regularly")?
    } else {
        true
    };
    let period = int(k, next(), "period")?;
    let phrase = string(k, next(), "phrase")?;
    let last_checked = int(k, next(), "last checked")?;
    if (4..=7).contains(&v) {
        // v8 folded "paused" into "run regularly"
        if boolean(k, next(), "paused")? {
            run_regularly = false;
        }
    }
    let run_now = if v >= 4 {
        boolean(k, next(), "run now")?
    } else {
        false
    };
    let last_error = if v >= 5 {
        string(k, next(), "last error")?
    } else {
        String::new()
    };
    let show_working_popup = if v >= 8 {
        boolean(k, next(), "show working popup")?
    } else {
        true
    };
    let (overwrite_sidecars_on_next_run, always_overwrite_sidecars) = if v >= 9 {
        (
            boolean(k, next(), "overwrite sidecars on next run")?,
            boolean(k, next(), "always overwrite sidecars")?,
        )
    } else {
        (false, false)
    };
    // (the reference never deletes after a synchronising export)
    let delete_from_client_after_export = delete_from_client_after_export && export_type != 1;
    Ok(LegacyExportFolder {
        name,
        path,
        export_type,
        delete_from_client_after_export,
        export_symlinks,
        search,
        routers,
        run_regularly,
        period,
        phrase,
        last_checked,
        run_now,
        last_error,
        show_working_popup,
        overwrite_sidecars_on_next_run,
        always_overwrite_sidecars,
    })
}
