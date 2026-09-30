//! Import folders (19, v7-11) and filename tagging options (56, v1-2).
//!
//! Folders from v7 on are read (the reference saves a folder each time it
//! checks it, so any folder in use is at v11); v7's "load tags from
//! neighbouring .txt files" become metadata routers, as the reference's
//! upgrade makes them, and v7-10's old-style import options are converted.

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_core::url::strings::{ProcessingStep, SortKind, StringConverter, StringProcessor};
use hydrus_parse::folders::{FilenameTagging, FolderAction, FolderActions, ImportFolderSettings};
use hydrus_parse::sidecar::{Exporter, Importer, Router, SidecarNaming, Source};

use super::domain::expect;
use super::subscriptions::{LegacyFileSeed, file_seed_cache};
use super::util::{DecodeResult, boolean, int, list, malformed, nested, string, strings, tuple};
use crate::pyjson::PyJson;
use crate::serialisable::{SerialisableObject, SerialisableType};

const IMPORT_FOLDER: SerialisableType = SerialisableType(19);
const FILENAME_TAGGING_OPTIONS: SerialisableType = SerialisableType(56);

/// A stored import folder.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyImportFolder {
    pub name: String,
    pub settings: ImportFolderSettings,
    pub paused: bool,
    /// Its own import options.
    pub options: ImportOptionsSlice,
    /// Its file seed cache: the files it has seen.
    pub file_seeds: Vec<LegacyFileSeed>,
}

/// Filename tagging options, and whether they asked (before v8) for tags
/// from neighbouring `.txt` files.
pub fn filename_tagging(object: &SerialisableObject) -> DecodeResult<(FilenameTagging, bool)> {
    let k = FILENAME_TAGGING_OPTIONS;
    expect(object, k, &[1, 2])?;
    let info = object.info();
    let flag = |value: &PyJson, what: &str| -> DecodeResult<Option<String>> {
        let [on, namespace] = tuple::<2>(k, value, what)?;
        boolean(k, on, what)?
            .then(|| string(k, namespace, what))
            .transpose()
    };
    let (tags, load_txt, add_filename, directories, quick, regexes) = if object.version == 1 {
        let [
            tags,
            load,
            add_filename,
            first,
            second,
            third,
            quick,
            regexes,
        ] = tuple::<8>(k, &info, "filename tagging options")?;
        let mut directories = Vec::new();
        for (index, value) in [(0, first), (1, second), (2, third)] {
            if let Some(namespace) = flag(value, "directory")? {
                directories.push((index, namespace));
            }
        }
        (tags, load, add_filename, directories, quick, regexes)
    } else {
        let [tags, load, add_filename, dirs, quick, regexes] =
            tuple::<6>(k, &info, "filename tagging options")?;
        let mut directories = Vec::new();
        for pair in list(k, dirs, "directories")? {
            let [index, value] = tuple::<2>(k, pair, "directory")?;
            if let Some(namespace) = flag(value, "directory")? {
                directories.push((int(k, index, "directory index")?, namespace));
            }
        }
        (tags, load, add_filename, directories, quick, regexes)
    };
    let quick_namespaces = list(k, quick, "quick namespaces")?
        .iter()
        .map(|pair| {
            let [namespace, regex] = tuple::<2>(k, pair, "quick namespace")?;
            Ok((
                string(k, namespace, "namespace")?,
                string(k, regex, "regex")?,
            ))
        })
        .collect::<DecodeResult<_>>()?;
    Ok((
        FilenameTagging {
            tags_for_all: strings(k, tags, "tags for all")?.into_iter().collect(),
            add_filename: flag(add_filename, "add filename")?,
            directories,
            quick_namespaces,
            regexes: strings(k, regexes, "regexes")?,
        },
        boolean(k, load_txt, "load from neighbouring txt files")?,
    ))
}

/// The router v8's upgrade makes for "load tags from neighbouring .txt
/// files": a plain `.txt` sidecar to the service's tags.
fn neighbouring_txt_router(service_key: &str) -> Router {
    Router {
        importers: vec![Importer {
            source: Source::Txt {
                naming: SidecarNaming {
                    remove_actual_filename_ext: false,
                    suffix: String::new(),
                    filename_converter: StringConverter::default(),
                },
                separator: "\n".into(),
            },
            processor: StringProcessor::default(),
        }],
        processor: StringProcessor {
            steps: vec![ProcessingStep::Sort {
                kind: SortKind::Human,
                ascending: true,
                regex: None,
            }],
        },
        exporter: Exporter::MediaTags {
            service_key: service_key.to_owned(),
        },
    }
}

fn actions(k: SerialisableType, pairs: &PyJson, locations: &PyJson) -> DecodeResult<FolderActions> {
    let location = |status: i64| -> DecodeResult<Option<String>> {
        for pair in list(k, locations, "action locations")? {
            let [s, path] = tuple::<2>(k, pair, "action location")?;
            if int(k, s, "status")? == status {
                return Ok(Some(string(k, path, "action location")?));
            }
        }
        Ok(None)
    };
    let mut out = FolderActions::default();
    for pair in list(k, pairs, "actions")? {
        let [status, action] = tuple::<2>(k, pair, "action")?;
        let status = int(k, status, "status")?;
        let action = match int(k, action, "action")? {
            0 => FolderAction::Delete,
            1 => FolderAction::Ignore,
            // a move without a location fails when tried, as in the
            // reference, which then pauses the folder
            2 => FolderAction::Move(location(status)?.unwrap_or_default()),
            other => {
                return Err(malformed(
                    k,
                    format!("unknown import folder action {other}"),
                ));
            }
        };
        let slot = match status {
            1 => &mut out.successful_and_new,
            2 => &mut out.successful_but_redundant,
            3 => &mut out.deleted,
            4 => &mut out.error,
            _ => continue,
        };
        *slot = action;
    }
    Ok(out)
}

/// An import folder (19, v7-11).
pub fn import_folder(object: &SerialisableObject) -> DecodeResult<LegacyImportFolder> {
    let k = IMPORT_FOLDER;
    expect(object, k, &[7, 8, 9, 10, 11])?;
    let name = object
        .name
        .clone()
        .ok_or_else(|| malformed(k, "import folder has no name"))?;
    let info = object.info();
    let items = list(k, &info, "import folder")?;
    let v = object.version;
    // v8 added routers, v9 the modified-time grace, v10 subdirectories;
    // v11 swapped the two old-style options for one container
    let expected_len = match v {
        7 => 15,
        8 => 16,
        10 => 18,
        _ => 17,
    };
    if items.len() != expected_len {
        return Err(malformed(
            k,
            format!(
                "a v{v} import folder should have {expected_len} items, found {}",
                items.len()
            ),
        ));
    }
    let mut it = items.iter();
    let mut next = || it.next().expect("length checked");
    let path = string(k, next(), "path")?;
    let search_subdirectories = if v >= 10 {
        boolean(k, next(), "search subdirectories")?
    } else {
        true
    };
    let options = if v >= 11 {
        super::import_options::slice(&nested(k, next(), "import options")?)?
    } else {
        let file = super::legacy_import_options::file_import_options(&nested(
            k,
            next(),
            "file import options",
        )?)?;
        let tags = super::legacy_import_options::tag_import_options(&nested(
            k,
            next(),
            "tag import options",
        )?)?;
        super::legacy_import_options::convert(Some(&file), Some(&tags), None)
    };
    let mut routers = if v >= 8 {
        super::sidecars::routers(k, next())?
    } else {
        Vec::new()
    };
    let mut filename_tagging = Vec::new();
    for pair in list(k, next(), "filename tagging options")? {
        let [key, options] = tuple::<2>(k, pair, "filename tagging options")?;
        let key = string(k, key, "tag service key")?;
        let (tagging, load_txt) = filename_tagging_of(k, options)?;
        if v == 7 && load_txt {
            routers.push(neighbouring_txt_router(&key));
        }
        filename_tagging.push((key, tagging));
    }
    let action_pairs = next();
    let action_locations = next();
    let actions = actions(k, action_pairs, action_locations)?;
    let period = int(k, next(), "period")?;
    let check_regularly = boolean(k, next(), "check regularly")?;
    let file_seeds = file_seed_cache(&nested(k, next(), "file seed cache")?)?;
    let last_checked = int(k, next(), "last checked")?;
    let paused = boolean(k, next(), "paused")?;
    let check_now = boolean(k, next(), "check now")?;
    let last_modified_time_skip_period = if v >= 9 {
        int(k, next(), "last modified time skip period")?
    } else {
        60
    };
    let show_working_popup = boolean(k, next(), "show working popup")?;
    let publish_files_to_popup_button = boolean(k, next(), "publish files to popup button")?;
    let publish_files_to_page = boolean(k, next(), "publish files to page")?;
    Ok(LegacyImportFolder {
        name,
        settings: ImportFolderSettings {
            path,
            search_subdirectories,
            routers,
            filename_tagging,
            actions,
            period,
            check_regularly,
            last_checked,
            check_now,
            last_modified_time_skip_period,
            show_working_popup,
            publish_files_to_popup_button,
            publish_files_to_page,
        },
        paused,
        options,
        file_seeds,
    })
}

fn filename_tagging_of(
    k: SerialisableType,
    value: &PyJson,
) -> DecodeResult<(FilenameTagging, bool)> {
    filename_tagging(&nested(k, value, "filename tagging options")?)
}
