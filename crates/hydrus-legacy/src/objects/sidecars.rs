//! Metadata routers (109), their importers (110, 111, 114, 118, 120, 123)
//! and exporters (113, 115, 116, 117, 119, 122), and timestamp data (121):
//! how import and export folders move metadata to and from sidecars.
//! Every version the reference upgrades from is read, upgraded as it does.

use hydrus_core::content::TimestampType;
use hydrus_core::url::strings::{ProcessingStep, SortKind, StringConverter, StringProcessor};
use hydrus_parse::formula::Formula;
use hydrus_parse::sidecar::{
    Exporter, Importer, Router, SidecarNaming, Source, TagDisplay, TimestampLocation, TimestampStub,
};

use super::domain::{expect, nested_list, string_converter, string_processor};
use super::util::{
    DecodeResult, boolean, int, malformed, nested, opt_string, string, strings, tuple,
};
use crate::pyjson::PyJson;
use crate::serialisable::{SerialisableObject, SerialisableType};

const ROUTER: SerialisableType = SerialisableType(109);
const IMPORTER_TXT: SerialisableType = SerialisableType(110);
const IMPORTER_MEDIA_TAGS: SerialisableType = SerialisableType(111);
const EXPORTER_JSON: SerialisableType = SerialisableType(113);
const IMPORTER_JSON: SerialisableType = SerialisableType(114);
const EXPORTER_MEDIA_TAGS: SerialisableType = SerialisableType(115);
const EXPORTER_TXT: SerialisableType = SerialisableType(116);
const EXPORTER_MEDIA_URLS: SerialisableType = SerialisableType(117);
const IMPORTER_MEDIA_URLS: SerialisableType = SerialisableType(118);
const EXPORTER_MEDIA_NOTES: SerialisableType = SerialisableType(119);
const IMPORTER_MEDIA_NOTES: SerialisableType = SerialisableType(120);
const TIMESTAMP_DATA: SerialisableType = SerialisableType(121);
const EXPORTER_MEDIA_TIMESTAMPS: SerialisableType = SerialisableType(122);
const IMPORTER_MEDIA_TIMESTAMPS: SerialisableType = SerialisableType(123);

fn processor(kind: SerialisableType, value: &PyJson, what: &str) -> DecodeResult<StringProcessor> {
    string_processor(&nested(kind, value, what)?)
}

fn converter(kind: SerialisableType, value: &PyJson, what: &str) -> DecodeResult<StringConverter> {
    string_converter(&nested(kind, value, what)?)
}

fn naming(
    kind: SerialisableType,
    remove: &PyJson,
    suffix: &PyJson,
    filename_converter: &PyJson,
) -> DecodeResult<SidecarNaming> {
    Ok(SidecarNaming {
        remove_actual_filename_ext: boolean(kind, remove, "remove actual filename ext")?,
        suffix: string(kind, suffix, "suffix")?,
        filename_converter: converter(kind, filename_converter, "filename string converter")?,
    })
}

/// Naming as the reference upgrades an old sidecar's: just a suffix.
fn suffix_only(kind: SerialisableType, suffix: &PyJson) -> DecodeResult<SidecarNaming> {
    Ok(SidecarNaming {
        remove_actual_filename_ext: false,
        suffix: string(kind, suffix, "suffix")?,
        filename_converter: StringConverter::default(),
    })
}

/// `TimestampData` (121, v1-2) as a stub; the time itself, if any, is
/// dropped (importers and exporters only store stubs).
pub fn timestamp_stub(object: &SerialisableObject) -> DecodeResult<TimestampStub> {
    let k = TIMESTAMP_DATA;
    expect(object, k, &[1, 2])?;
    let info = object.info();
    let [kind, location, _time] = tuple::<3>(k, &info, "timestamp data")?;
    let code = int(k, kind, "timestamp type")?;
    let kind = u8::try_from(code)
        .ok()
        .and_then(TimestampType::from_code)
        .ok_or_else(|| malformed(k, format!("unknown timestamp type {code}")))?;
    let location = match kind {
        TimestampType::Imported | TimestampType::Deleted | TimestampType::PreviouslyImported => {
            TimestampLocation::Service(string(k, location, "file service key")?)
        }
        TimestampType::LastViewed => TimestampLocation::Canvas(int(k, location, "canvas type")?),
        TimestampType::ModifiedDomain => TimestampLocation::Domain(string(k, location, "domain")?),
        _ => TimestampLocation::None,
    };
    Ok(TimestampStub { kind, location })
}

/// An importer, of any kind and version.
pub fn importer(object: &SerialisableObject) -> DecodeResult<Importer> {
    object.check_not_future()?;
    let info = object.info();
    let k = object.kind;
    let v = object.version;
    let (source, processor) = match k {
        IMPORTER_MEDIA_NOTES => {
            expect(object, k, &[1])?;
            (Source::MediaNotes, processor(k, &info, "string processor")?)
        }
        IMPORTER_MEDIA_TAGS => {
            expect(object, k, &[1, 2, 3])?;
            let (proc, key, display) = match v {
                1 => (
                    StringProcessor::default(),
                    string(k, &info, "service key")?,
                    0,
                ),
                2 => {
                    let [proc, key] = tuple::<2>(k, &info, "media tags importer")?;
                    (
                        processor(k, proc, "string processor")?,
                        string(k, key, "service key")?,
                        0,
                    )
                }
                _ => {
                    let [proc, key, display] = tuple::<3>(k, &info, "media tags importer")?;
                    (
                        processor(k, proc, "string processor")?,
                        string(k, key, "service key")?,
                        int(k, display, "tag display type")?,
                    )
                }
            };
            let display = match display {
                0 => TagDisplay::Storage,
                1 => TagDisplay::DisplayActual,
                other => {
                    return Err(malformed(k, format!("unexpected tag display type {other}")));
                }
            };
            (
                Source::MediaTags {
                    service_key: key,
                    display,
                },
                proc,
            )
        }
        IMPORTER_MEDIA_TIMESTAMPS => {
            expect(object, k, &[1])?;
            let [proc, stub] = tuple::<2>(k, &info, "media timestamps importer")?;
            (
                Source::MediaTimestamp(timestamp_stub(&nested(k, stub, "timestamp data")?)?),
                processor(k, proc, "string processor")?,
            )
        }
        IMPORTER_MEDIA_URLS => {
            expect(object, k, &[1, 2])?;
            let proc = if v == 1 {
                StringProcessor::default()
            } else {
                processor(k, &info, "string processor")?
            };
            (Source::MediaUrls, proc)
        }
        IMPORTER_TXT => {
            expect(object, k, &[1, 2, 3, 4])?;
            let (proc, naming, separator) = match v {
                1 => (
                    StringProcessor::default(),
                    suffix_only(k, &info)?,
                    "\n".to_owned(),
                ),
                2 => {
                    let [proc, suffix] = tuple::<2>(k, &info, "txt importer")?;
                    (
                        processor(k, proc, "string processor")?,
                        suffix_only(k, suffix)?,
                        "\n".into(),
                    )
                }
                3 => {
                    let [proc, remove, suffix, conv] = tuple::<4>(k, &info, "txt importer")?;
                    (
                        processor(k, proc, "string processor")?,
                        naming(k, remove, suffix, conv)?,
                        "\n".into(),
                    )
                }
                _ => {
                    let [proc, remove, suffix, conv, sep] = tuple::<5>(k, &info, "txt importer")?;
                    (
                        processor(k, proc, "string processor")?,
                        naming(k, remove, suffix, conv)?,
                        string(k, sep, "separator")?,
                    )
                }
            };
            (Source::Txt { naming, separator }, proc)
        }
        IMPORTER_JSON => {
            expect(object, k, &[1, 2, 3])?;
            let formula_of = |value: &PyJson| -> DecodeResult<Formula> {
                super::parsers::formula(&nested(k, value, "json parsing formula")?)
            };
            let (proc, naming, formula) = match v {
                1 => {
                    let [suffix, formula] = tuple::<2>(k, &info, "json importer")?;
                    (
                        StringProcessor::default(),
                        suffix_only(k, suffix)?,
                        formula_of(formula)?,
                    )
                }
                2 => {
                    let [proc, suffix, formula] = tuple::<3>(k, &info, "json importer")?;
                    (
                        processor(k, proc, "string processor")?,
                        suffix_only(k, suffix)?,
                        formula_of(formula)?,
                    )
                }
                _ => {
                    let [proc, remove, suffix, conv, formula] =
                        tuple::<5>(k, &info, "json importer")?;
                    (
                        processor(k, proc, "string processor")?,
                        naming(k, remove, suffix, conv)?,
                        formula_of(formula)?,
                    )
                }
            };
            (
                Source::Json {
                    naming,
                    formula: Box::new(formula),
                },
                proc,
            )
        }
        other => return Err(malformed(other, "not a metadata importer")),
    };
    Ok(Importer { source, processor })
}

/// An exporter, of any kind and version.
pub fn exporter(object: &SerialisableObject) -> DecodeResult<Exporter> {
    object.check_not_future()?;
    let info = object.info();
    let k = object.kind;
    let v = object.version;
    Ok(match k {
        EXPORTER_MEDIA_NOTES => {
            expect(object, k, &[1, 2])?;
            Exporter::MediaNotes {
                forced_name: if v == 1 {
                    None
                } else {
                    opt_string(k, &info, "forced name")?
                },
            }
        }
        EXPORTER_MEDIA_TAGS => {
            expect(object, k, &[1])?;
            Exporter::MediaTags {
                service_key: string(k, &info, "service key")?,
            }
        }
        EXPORTER_MEDIA_TIMESTAMPS => {
            expect(object, k, &[1])?;
            Exporter::MediaTimestamp(timestamp_stub(&nested(k, &info, "timestamp data")?)?)
        }
        EXPORTER_MEDIA_URLS => {
            expect(object, k, &[1])?;
            Exporter::MediaUrls
        }
        EXPORTER_TXT => {
            expect(object, k, &[1, 2, 3])?;
            let (naming, separator) = match v {
                1 => (suffix_only(k, &info)?, "\n".to_owned()),
                2 => {
                    let [remove, suffix, conv] = tuple::<3>(k, &info, "txt exporter")?;
                    (naming(k, remove, suffix, conv)?, "\n".into())
                }
                _ => {
                    let [remove, suffix, conv, sep] = tuple::<4>(k, &info, "txt exporter")?;
                    (
                        naming(k, remove, suffix, conv)?,
                        string(k, sep, "separator")?,
                    )
                }
            };
            Exporter::Txt { naming, separator }
        }
        EXPORTER_JSON => {
            expect(object, k, &[1, 2])?;
            let (naming, names) = if v == 1 {
                let [suffix, names] = tuple::<2>(k, &info, "json exporter")?;
                (suffix_only(k, suffix)?, names)
            } else {
                let [remove, suffix, conv, names] = tuple::<4>(k, &info, "json exporter")?;
                (naming(k, remove, suffix, conv)?, names)
            };
            Exporter::Json {
                naming,
                nested_object_names: strings(k, names, "nested object names")?,
            }
        }
        other => return Err(malformed(other, "not a metadata exporter")),
    })
}

/// A `.txt` sidecar exporter with just a suffix (the default has none).
fn txt_exporter(suffix: String) -> Exporter {
    Exporter::Txt {
        naming: SidecarNaming {
            remove_actual_filename_ext: false,
            suffix,
            filename_converter: StringConverter::default(),
        },
        separator: "\n".into(),
    }
}

/// A metadata router (109, v1-3).
pub fn router(object: &SerialisableObject) -> DecodeResult<Router> {
    let k = ROUTER;
    expect(object, k, &[1, 2, 3])?;
    let info = object.info();
    let [importers, proc, exp] = tuple::<3>(k, &info, "router")?;
    let importers = nested_list(k, importers, "importers")?
        .iter()
        .map(importer)
        .collect::<DecodeResult<Vec<_>>>()?;
    let mut processor = processor(k, proc, "string processor")?;
    let exp = nested(k, exp, "exporter")?;
    let exporter = if object.version == 1 {
        // v1 kept combined importer/exporters; the reference remakes the
        // one in the export slot as an exporter
        match exp.kind {
            IMPORTER_TXT => match importer(&exp)?.source {
                Source::Txt { naming, .. } => txt_exporter(naming.suffix),
                _ => unreachable!("a txt importer's source is a txt sidecar"),
            },
            IMPORTER_MEDIA_TAGS => match importer(&exp)?.source {
                Source::MediaTags { service_key, .. } => Exporter::MediaTags { service_key },
                _ => unreachable!("a media tags importer's source is media tags"),
            },
            _ => txt_exporter(String::new()),
        }
    } else {
        exporter(&exp)?
    };
    if object.version < 3 {
        // v3 moved the router's hardcoded sort into its processor
        processor.steps.push(ProcessingStep::Sort {
            kind: SortKind::Human,
            ascending: true,
            regex: None,
        });
    }
    Ok(Router {
        importers,
        processor,
        exporter,
    })
}

/// A serialisable list of routers, as import and export folders keep them.
pub fn routers(kind: SerialisableType, value: &PyJson) -> DecodeResult<Vec<Router>> {
    nested_list(kind, value, "metadata routers")?
        .iter()
        .map(router)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(text: &str) -> Router {
        router(&SerialisableObject::from_tuple_str(text).unwrap()).unwrap()
    }

    #[test]
    fn old_routers_are_upgraded_as_the_reference_does() {
        // v2: a txt importer (v4) to media tags, with an empty processor
        let r = decode(
            r#"[109, 2, [[26, 3, [[2, [110, 4, [[84, 1, [26, 3, []]], false, "", [55, 1, [[], "x"]], "\n"]]]]], [84, 1, [26, 3, []]], [115, 1, "6c6f63616c2074616773"]]]"#,
        );
        assert_eq!(r.importers.len(), 1);
        assert!(matches!(r.importers[0].source, Source::Txt { .. }));
        assert_eq!(
            r.exporter,
            Exporter::MediaTags {
                service_key: "6c6f63616c2074616773".into()
            }
        );
        assert_eq!(
            r.processor.steps,
            vec![ProcessingStep::Sort {
                kind: SortKind::Human,
                ascending: true,
                regex: None
            }]
        );
    }
}
