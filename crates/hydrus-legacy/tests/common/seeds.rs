//! Facts about seeds, as the oracles record them.

use serde_json::{Value as Json, json};

use hydrus_legacy::objects::subscriptions::{LegacyFileSeed, LegacyGallerySeed};
use hydrus_legacy::serialisable::SerialisableObject;

pub fn object(value: &Json) -> SerialisableObject {
    SerialisableObject::from_tuple_str(&value.to_string()).unwrap()
}

pub fn sorted(items: &[String]) -> Json {
    let mut items = items.to_vec();
    items.sort();
    json!(items)
}

pub fn sorted_pairs(items: &[(String, String)]) -> Json {
    let mut items = items.to_vec();
    items.sort();
    json!(items)
}

pub fn service_tags(items: &[(String, Vec<String>)]) -> Json {
    items
        .iter()
        .map(|(key, tags)| (key.clone(), sorted(tags)))
        .collect::<serde_json::Map<_, _>>()
        .into()
}

pub fn file_seed_facts(f: &LegacyFileSeed) -> Json {
    json!({
        "type": f.seed_type,
        "data": f.data,
        // (the oracle has no URL classes, so it cannot normalise either)
        "comparison": f.data_for_comparison.as_ref().unwrap_or(&f.data),
        "created": f.created,
        "modified": f.modified,
        "source_time": f.source_time,
        "status": f.status,
        "note": f.note,
        "referral": f.referral_url,
        "headers": sorted_pairs(&f.request_headers),
        "filterable": sorted(&f.external_filterable_tags),
        "additional": service_tags(&f.external_additional_tags),
        "primary": sorted(&f.primary_urls),
        "source": sorted(&f.source_urls),
        "tags": sorted(&f.tags),
        "notes": sorted_pairs(&f.notes),
        "hashes": sorted_pairs(&f.hashes),
    })
}

pub fn gallery_seed_facts(g: &LegacyGallerySeed) -> Json {
    json!({
        "url": g.url,
        "can_generate_more_pages": g.can_generate_more_pages,
        "created": g.created,
        "modified": g.modified,
        "status": g.status,
        "note": g.note,
        "referral": g.referral_url,
        "headers": sorted_pairs(&g.request_headers),
        "filterable": sorted(&g.external_filterable_tags),
        "additional": service_tags(&g.external_additional_tags),
    })
}
