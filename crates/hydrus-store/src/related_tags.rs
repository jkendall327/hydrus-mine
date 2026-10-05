//! Preferences and namespace weights used by related-tag ranking.
use serde::{Deserialize, Serialize};

/// Ordered override rows; empty and colon are the two protected catch-all slices.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Weights {
    pub search: Vec<(String, u16)>,
    pub result: Vec<(String, u16)>,
}
impl Default for Weights {
    fn default() -> Self {
        let rows = |unnamespaced, character, creator| {
            vec![
                ("".into(), unnamespaced),
                (":".into(), 100),
                ("character:".into(), character),
                ("creator:".into(), creator),
                ("series:".into(), creator),
                ("filename:".into(), 0),
                ("page:".into(), 0),
                ("chapter:".into(), 0),
                ("volume:".into(), 0),
                ("title:".into(), 0),
            ]
        };
        Self {
            search: rows(10, 300, 300),
            result: rows(50, 400, 200),
        }
    }
}
impl Weights {
    pub fn percent(rows: &[(String, u16)], tag: &str) -> u16 {
        let namespace = hydrus_core::tag::split_tag(tag).0;
        let exact = if namespace.is_empty() {
            String::new()
        } else {
            format!("{namespace}:")
        };
        rows.iter()
            .rev()
            .find(|(slice, _)| slice == &exact)
            .or_else(|| {
                (!namespace.is_empty())
                    .then(|| rows.iter().rev().find(|(slice, _)| slice == ":"))
                    .flatten()
            })
            .map_or(100, |(_, weight)| *weight)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub weights: Weights,
    pub enabled: bool,
    pub durations_ms: [u32; 3],
    pub concurrence_percent: u8,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            weights: Weights::default(),
            enabled: true,
            durations_ms: [250, 2_000, 6_000],
            concurrence_percent: 6,
        }
    }
}
impl crate::settings::Setting for Settings {
    const KEY: &'static str = "related_tags";
}

/// A captured service/context; changing the UI cannot retarget a running query.
#[derive(Clone, Debug)]
pub struct Query {
    pub service: hydrus_core::ServiceKey,
    pub searches: Vec<String>,
    pub local: bool,
    pub display: bool,
    pub weights: Weights,
    pub concurrence_percent: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Suggestion {
    pub tag: String,
    pub score: u64,
}
/// Exact small-corpus cosine ranking and the two separate integer truncations.
/// Large reference queries additionally sample under a wall-clock budget.
pub fn rank(
    files: &std::collections::BTreeMap<String, std::collections::BTreeSet<hydrus_core::HashId>>,
    query: &Query,
    cancelled: &dyn Fn() -> bool,
) -> Vec<Suggestion> {
    let searches: Vec<_> = query
        .searches
        .iter()
        .filter_map(|tag| {
            let weight = Weights::percent(&query.weights.search, tag);
            let set = files.get(tag)?;
            (weight > 0 && !set.is_empty()).then_some((tag, set, weight))
        })
        .collect();
    let mut scores = Vec::new();
    for (tag, candidate) in files {
        if cancelled() {
            return Vec::new();
        }
        if candidate.is_empty() || query.searches.contains(tag) {
            continue;
        }
        let mut score = 0.0;
        for (_, search, weight) in &searches {
            let matching = search.intersection(candidate).count();
            if matching as f64 / search.len() as f64 >= f64::from(query.concurrence_percent) / 100.0
            {
                score += matching as f64 / ((candidate.len() * search.len()) as f64).sqrt()
                    * f64::from(*weight)
                    / 100.0;
            }
        }
        if score > 0.0 {
            scores.push((tag, score));
        }
    }
    scores.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let mut result: Vec<_> = scores
        .into_iter()
        .take(100)
        .filter_map(|(tag, score)| {
            let weight = Weights::percent(&query.weights.result, tag);
            (weight > 0).then(|| Suggestion {
                tag: tag.clone(),
                score: ((score * 1_000.0) as u64 * u64::from(weight)) / 100,
            })
        })
        .collect();
    result.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.tag.cmp(&b.tag)));
    result
}
/// Read current and pending mappings on the worker connection, optionally applying
/// this service's sibling/parent graph. Cancellation is checked during row reads.
pub fn query(
    store: &crate::Store,
    request: &Query,
    cancelled: &dyn Fn() -> bool,
) -> crate::Result<Vec<Suggestion>> {
    use hydrus_core::{HashId, TagId};
    use std::collections::{BTreeMap, BTreeSet};
    let snapshot = store.snapshot();
    let service = snapshot.services.by_key(&request.service)?;
    let local = snapshot.services.by_key(&hydrus_core::ServiceKey::new(
        hydrus_core::service::builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec(),
    ))?;
    let graph = snapshot.display.get(service.id);
    let tables = crate::schema::MappingTables::new(service.id);
    store.read(|conn| {
        let mut sets: BTreeMap<TagId,BTreeSet<HashId>> = BTreeMap::new();
        for table in [&tables.current,&tables.pending] {
            let sql = if request.local { format!("SELECT tag_id,hash_id FROM {table} m WHERE EXISTS (SELECT 1 FROM file_domain_current d WHERE d.hash_id=m.hash_id AND d.service_id=?1)") } else { format!("SELECT tag_id,hash_id FROM {table} WHERE ?1 IS NOT NULL") };
            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query([local.id])?;
            while let Some(row) = rows.next()? {
                if cancelled() { return Ok(Vec::new()); }
                let tag: TagId = row.get(0)?;
                let file: HashId = row.get(1)?;
                if request.display {
                    for shown in graph.display_tags(tag) { sets.entry(shown).or_default().insert(file); }
                } else { sets.entry(tag).or_default().insert(file); }
            }
        }
        let names = crate::master::tags(conn,&sets.keys().copied().collect::<Vec<_>>())?;
        let files: BTreeMap<_,_> = sets.into_iter().filter_map(|(id,files)| names.get(&id).map(|tag| (tag.as_str().to_owned(),files))).collect();
        let mut request = request.clone();
        if request.display {
            request.searches = request.searches.iter().filter(|tag| Weights::percent(&request.weights.search,tag)>0).filter_map(|tag| hydrus_core::Tag::new(tag)).map(|tag| {
                crate::master::tag_id(conn,&tag).map(|id| id.and_then(|id| names.get(&graph.ideal(id))).map_or_else(|| tag.as_str().to_owned(),|t| t.as_str().to_owned()))
            }).collect::<crate::Result<Vec<_>>>()?;
        }
        Ok(rank(&files,&request,cancelled))
    })
}
