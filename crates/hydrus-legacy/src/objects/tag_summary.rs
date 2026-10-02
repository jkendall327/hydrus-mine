//! Tag summary generators (`TagSummaryGenerator`, serialisable type 61):
//! what the reference writes over thumbnails and the media viewer from a
//! file's tags in some namespaces.

use hydrus_core::tag_summary::{NamespaceInfo, TagSummaryGenerator};

use super::util::{DecodeResult, boolean, int, list, malformed, string, strings, tuple};
use crate::pyjson::PyJson;
use crate::serialisable::{SerialisableObject, SerialisableType};

const KIND: SerialisableType = SerialisableType(61);

fn rgba(value: &PyJson, what: &str) -> DecodeResult<[u8; 4]> {
    let [r, g, b, a] = tuple::<4>(KIND, value, what)?;
    let channel = |v: &PyJson| -> DecodeResult<u8> {
        u8::try_from(int(KIND, v, what)?)
            .map_err(|_| malformed(KIND, format!("{what}: a channel past 255")))
    };
    Ok([channel(r)?, channel(g)?, channel(b)?, channel(a)?])
}

fn namespace_info(value: &PyJson) -> DecodeResult<Vec<NamespaceInfo>> {
    list(KIND, value, "namespace info")?
        .iter()
        .map(|row| {
            let [namespace, prefix, separator] = tuple::<3>(KIND, row, "namespace row")?;
            Ok(NamespaceInfo {
                namespace: string(KIND, namespace, "namespace")?,
                prefix: string(KIND, prefix, "prefix")?,
                separator: string(KIND, separator, "separator")?,
            })
        })
        .collect()
}

/// Decode a tag summary generator (versions 1 and 2: version 1 had the
/// default colours and showed).
pub fn tag_summary_generator(object: &SerialisableObject) -> DecodeResult<TagSummaryGenerator> {
    object.expect_kind(KIND)?;
    object.check_not_future()?;
    let info = object.info();
    let defaults = TagSummaryGenerator::thumbnail_top();
    if object.version == 1 {
        let [info_rows, separator, examples] = tuple::<3>(KIND, &info, "generator")?;
        Ok(TagSummaryGenerator {
            namespace_info: namespace_info(info_rows)?,
            separator: string(KIND, separator, "separator")?,
            example_tags: strings(KIND, examples, "example tags")?,
            ..defaults
        })
    } else {
        let [background, text, info_rows, separator, examples, show] =
            tuple::<6>(KIND, &info, "generator")?;
        Ok(TagSummaryGenerator {
            background: rgba(background, "background colour")?,
            text: rgba(text, "text colour")?,
            namespace_info: namespace_info(info_rows)?,
            separator: string(KIND, separator, "separator")?,
            example_tags: strings(KIND, examples, "example tags")?,
            show: boolean(KIND, show, "show")?,
        })
    }
}
