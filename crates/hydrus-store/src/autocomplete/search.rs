//! Running an autocomplete search against the store.
//!
//! The result is every tag, in the searched tag service(s), that some file
//! in the searched file domain(s) has, and that matches the query either
//! itself or through a sibling. Counts come from the derived count tables
//! (storage or display), so no mapping table is touched.
//!
//! Cost: the text search is a range scan of the word index, bounded by the
//! number of tags matching the text (a leading wildcard, as in the reference,
//! has to scan every subtag); counts are primary-key lookups. "Fetch all"
//! searches, which only run when a tag service's rules allow them, read the
//! searched domain's counts directly.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use rusqlite::{Connection, params};

use hydrus_core::numbers::human_int;
use hydrus_core::tag::split_tag;
use hydrus_core::{NamespaceId, ServiceId, ServiceType, SubtagId, TagId};

use super::input::TagQuery;
use super::pattern::{QueryWord, ResultFilter, SubtagSearch};
use crate::display::{DisplayGraph, DisplayGraphs};
use crate::error::Result;
use crate::master::{self, id_array};
use crate::schema::MappingTables;
use crate::services::ServiceRegistry;
use crate::text::searchable_subtag;

/// Stored tags, or tags as siblings and parents display them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagDisplayType {
    Storage,
    Display,
}

/// A file domain whose files' tags are counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CountDomain {
    pub service: ServiceId,
    /// Whether the domain's counts are exact for the search. Counts in
    /// "deleted from anywhere", standing in for one service's deleted files,
    /// only give an upper bound, so their lower bound is zero.
    pub exact: bool,
}

/// Where an autocomplete search looks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagSearchScope {
    /// File domains, each counted separately.
    pub domains: Vec<CountDomain>,
    /// A tag service, or `None` for all known tags.
    pub tag_service: Option<ServiceId>,
    pub display: TagDisplayType,
    pub include_current: bool,
    pub include_pending: bool,
}

/// How many files have a tag, as a range: counts from different domains or
/// tag services can't be de-duplicated without reading mappings, so they are
/// combined into `[largest, sum]`, as the reference does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CountRange {
    pub min_current: u64,
    pub max_current: u64,
    pub min_pending: u64,
    pub max_pending: u64,
}

impl CountRange {
    /// Exactly `n` current files.
    pub fn current(n: u64) -> Self {
        Self {
            min_current: n,
            max_current: n,
            ..Self::default()
        }
    }

    /// Combine with another domain's count: the larger lower bound, the
    /// summed upper bound.
    pub fn merge(&mut self, other: CountRange) {
        self.min_current = self.min_current.max(other.min_current);
        self.max_current += other.max_current;
        self.min_pending = self.min_pending.max(other.min_pending);
        self.max_pending += other.max_pending;
    }

    /// The count the Client API reports: the lower bounds, current plus
    /// pending.
    pub fn min_total(&self) -> u64 {
        self.min_current + self.min_pending
    }

    /// e.g. `(5-7) (+2)`, as the reference renders counts.
    pub fn suffix(&self) -> String {
        let mut parts = Vec::new();
        for (min, max, plus) in [
            (self.min_current, self.max_current, ""),
            (self.min_pending, self.max_pending, "+"),
        ] {
            if min > 0 || max > 0 {
                let mut text = format!("({plus}{}", human_int(min));
                if max > min {
                    let _ = write!(text, "-{}", human_int(max));
                }
                text.push(')');
                parts.push(text);
            }
        }
        parts.join(" ")
    }
}

/// One autocomplete result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagMatch {
    pub tag: String,
    pub count: CountRange,
}

/// Which tags a query's text selects, before counting.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TagMatcher {
    Nothing,
    /// Every tag.
    All,
    /// Every tag in these namespaces.
    Namespaces(Vec<NamespaceId>),
    /// Tags whose subtag matches, optionally only in some namespaces.
    Text {
        namespaces: Option<Vec<NamespaceId>>,
        subtags: SubtagSearch,
    },
}

impl TagMatcher {
    fn plan(conn: &Connection, text: &str) -> Result<Self> {
        let (namespace, subtag) = split_tag(text);
        if text.is_empty() || subtag.is_empty() {
            return Ok(TagMatcher::Nothing);
        }
        let namespaces = if namespace.is_empty() || namespace == "*" {
            None
        } else if namespace.contains('*') {
            let mut stmt =
                conn.prepare_cached("SELECT namespace_id FROM namespaces WHERE namespace LIKE ?")?;
            let ids = stmt
                .query_map([namespace.replace('*', "%")], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<NamespaceId>>>()?;
            Some(ids)
        } else {
            match master::namespace_id(conn, namespace)? {
                Some(id) => Some(vec![id]),
                None => return Ok(TagMatcher::Nothing),
            }
        };
        Ok(match (subtag, namespaces) {
            ("*", None) => TagMatcher::All,
            ("*", Some(ids)) => TagMatcher::Namespaces(ids),
            (_, namespaces) => TagMatcher::Text {
                namespaces,
                subtags: SubtagSearch::plan(subtag),
            },
        })
    }
}

/// The smallest string greater than every string starting with `prefix`.
fn prefix_upper_bound(prefix: &str) -> Option<String> {
    let mut chars: Vec<char> = prefix.chars().collect();
    while let Some(last) = chars.pop() {
        let next = match u32::from(last) + 1 {
            0xD800 => Some('\u{E000}'),
            n => char::from_u32(n),
        };
        if let Some(next) = next {
            chars.push(next);
            return Some(chars.into_iter().collect());
        }
    }
    None
}

/// Subtags an FTS4 phrase query could match, from the word index. Returns
/// whether the candidates still need checking against the phrase.
fn phrase_candidates(conn: &Connection, query: &[QueryWord]) -> Result<(Vec<SubtagId>, bool)> {
    if query.is_empty() {
        return Ok((Vec::new(), false));
    }
    let mut sql = Vec::new();
    let mut args: Vec<String> = Vec::new();
    let mut exact = query.len() == 1;
    for word in query {
        if word.prefix {
            args.push(word.word.clone());
            if let Some(upper) = prefix_upper_bound(&word.word) {
                sql.push(format!(
                    "SELECT subtag_id FROM cache_subtag_words WHERE word >= ?{} AND word < ?{}",
                    args.len(),
                    args.len() + 1
                ));
                args.push(upper);
            } else {
                sql.push(format!(
                    "SELECT subtag_id FROM cache_subtag_words WHERE word >= ?{}",
                    args.len()
                ));
                exact = false;
            }
        } else {
            args.push(word.word.clone());
            sql.push(format!(
                "SELECT subtag_id FROM cache_subtag_words WHERE word = ?{}",
                args.len()
            ));
        }
    }
    let sql = sql.join(" INTERSECT ");
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut ids: Vec<SubtagId> = stmt
        .query_map(rusqlite::params_from_iter(&args), |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    ids.sort_unstable();
    ids.dedup();
    Ok((ids, !exact))
}

/// Keep the subtags whose searchable form `search` matches.
fn verify_subtags(
    conn: &Connection,
    search: &SubtagSearch,
    ids: &[SubtagId],
) -> Result<Vec<SubtagId>> {
    let mut stmt =
        conn.prepare_cached("SELECT subtag_id, subtag FROM subtags WHERE subtag_id IN rarray(?)")?;
    let mut out = Vec::with_capacity(ids.len());
    let mut rows = stmt.query([id_array(ids)])?;
    while let Some(row) = rows.next()? {
        let subtag: String = row.get(1)?;
        if search.matches(&searchable_subtag(&subtag)) {
            out.push(row.get(0)?);
        }
    }
    Ok(out)
}

/// Every subtag `search` finds.
fn matching_subtags(conn: &Connection, search: &SubtagSearch) -> Result<Vec<SubtagId>> {
    let narrowing = match search {
        SubtagSearch::Phrase(query) => Some(query.clone()),
        SubtagSearch::Like { narrow, .. } => narrow.clone(),
        SubtagSearch::Exact(text) => {
            let words: Vec<QueryWord> = crate::text::words(text)
                .map(|word| QueryWord {
                    word,
                    prefix: false,
                })
                .collect();
            (!words.is_empty()).then_some(words)
        }
        SubtagSearch::All => None,
    };
    if let Some(query) = narrowing {
        let (candidates, needs_check) = phrase_candidates(conn, &query)?;
        return if needs_check || !matches!(search, SubtagSearch::Phrase(_)) {
            verify_subtags(conn, search, &candidates)
        } else {
            Ok(candidates)
        };
    }
    // no word to look up: check every subtag, as the reference does
    let mut stmt = conn.prepare_cached("SELECT subtag_id, subtag FROM subtags")?;
    let mut out = Vec::new();
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let subtag: String = row.get(1)?;
        if search.matches(&searchable_subtag(&subtag)) {
            out.push(row.get(0)?);
        }
    }
    Ok(out)
}

/// Every tag (in any service) whose text a `Text` matcher selects.
fn matching_tags(
    conn: &Connection,
    namespaces: Option<&[NamespaceId]>,
    subtags: &SubtagSearch,
) -> Result<Vec<TagId>> {
    let subtag_ids = matching_subtags(conn, subtags)?;
    if subtag_ids.is_empty() {
        return Ok(Vec::new());
    }
    let tags: Vec<TagId> = match namespaces {
        None => conn
            .prepare_cached("SELECT tag_id FROM tags WHERE subtag_id IN rarray(?)")?
            .query_map([id_array(&subtag_ids)], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?,
        Some(namespaces) => conn
            .prepare_cached(
                "SELECT tag_id FROM tags WHERE subtag_id IN rarray(?1) AND namespace_id IN rarray(?2)",
            )?
            .query_map(
                params![id_array(&subtag_ids), id_array(namespaces)],
                |r| r.get(0),
            )?
            .collect::<rusqlite::Result<_>>()?,
    };
    Ok(tags)
}

/// Tags matched directly plus every member of their sibling chains.
fn with_sibling_chains(graph: &DisplayGraph, matched: &[TagId]) -> Vec<TagId> {
    let mut out: HashSet<TagId> = matched.iter().copied().collect();
    for &tag in matched {
        if graph.in_sibling_chain(tag) {
            out.extend(graph.chain(tag));
        }
    }
    out.into_iter().collect()
}

/// Per-tag counts in one domain of one tag service.
struct DomainCounts<'a> {
    conn: &'a Connection,
    table: &'a str,
    domain: CountDomain,
    include_current: bool,
    include_pending: bool,
}

impl DomainCounts<'_> {
    fn range(&self, current: i64, pending: i64) -> Option<CountRange> {
        let current = if self.include_current {
            current.max(0) as u64
        } else {
            0
        };
        let pending = if self.include_pending {
            pending.max(0) as u64
        } else {
            0
        };
        if current == 0 && pending == 0 {
            return None;
        }
        let (min_current, min_pending) = if self.domain.exact {
            (current, pending)
        } else {
            (0, 0)
        };
        Some(CountRange {
            min_current,
            max_current: current,
            min_pending,
            max_pending: pending,
        })
    }

    fn collect(&self, sql: &str, args: impl rusqlite::Params) -> Result<Vec<(TagId, CountRange)>> {
        let mut stmt = self.conn.prepare_cached(sql)?;
        let mut rows = stmt.query(args)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            if let Some(range) = self.range(row.get(1)?, row.get(2)?) {
                out.push((row.get(0)?, range));
            }
        }
        Ok(out)
    }

    fn for_tags(&self, tags: &[TagId]) -> Result<Vec<(TagId, CountRange)>> {
        if tags.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT tag_id, current, pending FROM {} WHERE domain_id = ?1 AND tag_id IN rarray(?2)",
            self.table
        );
        self.collect(&sql, params![self.domain.service, id_array(tags)])
    }

    fn all(&self) -> Result<Vec<(TagId, CountRange)>> {
        let sql = format!(
            "SELECT tag_id, current, pending FROM {} WHERE domain_id = ?1",
            self.table
        );
        self.collect(&sql, params![self.domain.service])
    }

    fn in_namespaces(&self, namespaces: &[NamespaceId]) -> Result<Vec<(TagId, CountRange)>> {
        let sql = format!(
            "SELECT c.tag_id, c.current, c.pending FROM {} c JOIN tags t ON t.tag_id = c.tag_id
             WHERE c.domain_id = ?1 AND t.namespace_id IN rarray(?2)",
            self.table
        );
        self.collect(&sql, params![self.domain.service, id_array(namespaces)])
    }
}

/// Sibling-chained tags of `graph` that are in one of `namespaces`, with
/// their whole chains.
fn chained_in_namespaces(
    conn: &Connection,
    graph: &DisplayGraph,
    namespaces: &[NamespaceId],
) -> Result<Vec<TagId>> {
    let chained: Vec<TagId> = graph.sibling_chained_tags().collect();
    if chained.is_empty() {
        return Ok(Vec::new());
    }
    let in_namespaces: Vec<TagId> = conn
        .prepare_cached(
            "SELECT tag_id FROM tags WHERE tag_id IN rarray(?1) AND namespace_id IN rarray(?2)",
        )?
        .query_map(params![id_array(&chained), id_array(namespaces)], |r| {
            r.get(0)
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(with_sibling_chains(graph, &in_namespaces))
}

/// Count an explicit set of tag IDs, retaining zero-count tags for children tabs.
pub fn count_tags(
    conn: &Connection,
    services: &ServiceRegistry,
    scope: &TagSearchScope,
    tags: &[TagId],
) -> Result<Vec<TagMatch>> {
    let mut merged: HashMap<TagId, CountRange> =
        tags.iter().map(|id| (*id, CountRange::default())).collect();
    let tag_services: Vec<_> = scope.tag_service.map_or_else(
        || services.tag_services().map(|s| s.id).collect(),
        |id| vec![id],
    );
    for service in tag_services {
        let tables = MappingTables::new(service);
        let table = match scope.display {
            TagDisplayType::Storage => tables.counts,
            TagDisplayType::Display => tables.display_counts,
        };
        for &domain in &scope.domains {
            let counts = DomainCounts {
                conn,
                table: &table,
                domain,
                include_current: scope.include_current,
                include_pending: scope.include_pending,
            };
            for (tag, range) in counts.for_tags(tags)? {
                merged.entry(tag).or_default().merge(range);
            }
        }
    }
    let texts = master::tags(conn, tags)?;
    let mut matches: Vec<_> = merged
        .into_iter()
        .filter_map(|(id, count)| {
            texts.get(&id).map(|t| TagMatch {
                tag: t.as_str().to_owned(),
                count,
            })
        })
        .collect();
    sort_matches(&mut matches);
    Ok(matches)
}

/// Run an autocomplete search.
///
/// Results are sorted by count, largest first, then by their text as the
/// reference renders it.
pub fn search_tags(
    conn: &Connection,
    services: &ServiceRegistry,
    graphs: &DisplayGraphs,
    scope: &TagSearchScope,
    query: &TagQuery,
) -> Result<Vec<TagMatch>> {
    search_tags_inner(conn, services, graphs, scope, query, None)
}

/// Write autocomplete keeps matching known tags even with no mappings in the searched domain.
pub fn search_tags_for_write(
    conn: &Connection,
    services: &ServiceRegistry,
    graphs: &DisplayGraphs,
    scope: &TagSearchScope,
    query: &TagQuery,
    display_service: ServiceId,
) -> Result<Vec<TagMatch>> {
    search_tags_inner(conn, services, graphs, scope, query, Some(display_service))
}

fn search_tags_inner(
    conn: &Connection,
    services: &ServiceRegistry,
    graphs: &DisplayGraphs,
    scope: &TagSearchScope,
    query: &TagQuery,
    write_display_service: Option<ServiceId>,
) -> Result<Vec<TagMatch>> {
    let all_known_files = services
        .of_type(ServiceType::CombinedFile)
        .next()
        .map(|s| s.id);
    if scope.tag_service.is_none()
        && scope
            .domains
            .iter()
            .any(|d| Some(d.service) == all_known_files)
    {
        // every tag of every file ever seen: the reference refuses too
        return Ok(Vec::new());
    }
    let tag_services: Vec<ServiceId> = match scope.tag_service {
        Some(service) => vec![service],
        None => services.tag_services().map(|s| s.id).collect(),
    };

    let mut matchers = vec![TagMatcher::plan(conn, &query.text)?];
    if query.search_namespaces_into_full_tags && !query.text.contains(':') {
        // 'char' also finds 'character:...'
        matchers.push(TagMatcher::plan(conn, &format!("{}*:*", query.text))?);
    }
    // text searches are the same for every tag service: run them once
    let mut text_matches: Vec<TagId> = Vec::new();
    for matcher in &matchers {
        if let TagMatcher::Text {
            namespaces,
            subtags,
        } = matcher
        {
            text_matches.extend(matching_tags(conn, namespaces.as_deref(), subtags)?);
        }
    }
    text_matches.sort_unstable();
    text_matches.dedup();

    let mut merged: HashMap<TagId, CountRange> = HashMap::new();
    for &service in &tag_services {
        let graph = graphs.get(write_display_service.unwrap_or(service));
        let tables = MappingTables::new(service);
        let table = match scope.display {
            TagDisplayType::Storage => tables.counts,
            TagDisplayType::Display => tables.display_counts,
        };
        // tags to count in every domain: text matches and their sibling
        // chains, and for namespace fetch-alls the chains reaching into them
        let mut candidates = with_sibling_chains(&graph, &text_matches);
        for matcher in &matchers {
            if let TagMatcher::Namespaces(namespaces) = matcher {
                candidates.extend(chained_in_namespaces(conn, &graph, namespaces)?);
            }
        }
        for &domain in &scope.domains {
            let counts = DomainCounts {
                conn,
                table: &table,
                domain,
                include_current: scope.include_current,
                include_pending: scope.include_pending,
            };
            let mut found = counts.for_tags(&candidates)?;
            for matcher in &matchers {
                match matcher {
                    TagMatcher::All => found.extend(counts.all()?),
                    TagMatcher::Namespaces(namespaces) => {
                        found.extend(counts.in_namespaces(namespaces)?);
                    }
                    TagMatcher::Nothing | TagMatcher::Text { .. } => {}
                }
            }
            // a tag found twice in one domain is still counted once there
            let mut seen = HashSet::new();
            for (tag, range) in found {
                if seen.insert(tag) {
                    merged
                        .entry(tag)
                        .and_modify(|r| r.merge(range))
                        .or_insert(range);
                }
            }
        }
    }
    if let Some(display_service) = write_display_service {
        let graph = graphs.get(display_service);
        let mut candidates = with_sibling_chains(&graph, &text_matches);
        for matcher in &matchers {
            match matcher {
                TagMatcher::All => candidates.extend(
                    conn.prepare_cached("SELECT tag_id FROM tags")?
                        .query_map([], |row| row.get::<_, TagId>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()?,
                ),
                TagMatcher::Namespaces(namespaces) => candidates.extend(
                    conn.prepare_cached("SELECT tag_id FROM tags WHERE namespace_id IN rarray(?)")?
                        .query_map([id_array(namespaces)], |row| row.get::<_, TagId>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()?,
                ),
                TagMatcher::Nothing | TagMatcher::Text { .. } => (),
            }
        }
        for candidate in candidates {
            merged.entry(candidate).or_default();
        }
    }
    if merged.is_empty() {
        return Ok(Vec::new());
    }

    // the siblings a result may also be matched by
    let known_siblings = |tag: TagId| -> Vec<TagId> {
        if let Some(service) = write_display_service {
            let graph = graphs.get(service);
            return if graph.in_sibling_chain(tag) {
                graph.chain(tag)
            } else {
                Vec::new()
            };
        }
        let sources: &[ServiceId] = match (scope.display, scope.tag_service) {
            (TagDisplayType::Storage, None) => &[],
            (TagDisplayType::Storage, Some(_)) | (TagDisplayType::Display, _) => &tag_services,
        };
        let mut out = Vec::new();
        for &service in sources {
            let graph = graphs.get(service);
            if graph.in_sibling_chain(tag) {
                out.extend(graph.chain(tag));
            }
        }
        out
    };
    let siblings: HashMap<TagId, Vec<TagId>> =
        merged.keys().map(|&t| (t, known_siblings(t))).collect();
    let mut all_ids: Vec<TagId> = merged.keys().copied().collect();
    all_ids.extend(siblings.values().flatten().copied());
    all_ids.sort_unstable();
    all_ids.dedup();
    let texts = master::tags(conn, &all_ids)?;

    let filter = ResultFilter::new(&query.text);
    let mut results: Vec<TagMatch> = merged
        .into_iter()
        .filter_map(|(tag, count)| {
            let text = texts.get(&tag)?;
            let matched = filter.matches(text.as_str())
                || siblings[&tag]
                    .iter()
                    .filter_map(|s| texts.get(s))
                    .any(|s| filter.matches(s.as_str()));
            matched.then(|| TagMatch {
                tag: text.as_str().to_owned(),
                count,
            })
        })
        .collect();
    sort_matches(&mut results);
    Ok(results)
}

/// Largest count first, then by the text the reference renders
/// (`tag (count)`), which is what it sorts by.
fn sort_matches(results: &mut [TagMatch]) {
    results.sort_by_cached_key(|m| {
        let mut rendered = m.tag.clone();
        let suffix = m.count.suffix();
        if !suffix.is_empty() {
            rendered.push(' ');
            rendered.push_str(&suffix);
        }
        (std::cmp::Reverse(m.count.min_total()), rendered)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upper_bounds_cover_exactly_the_prefix() {
        assert_eq!(prefix_upper_bound("ab").as_deref(), Some("ac"));
        assert_eq!(
            prefix_upper_bound("a\u{D7FF}").as_deref(),
            Some("a\u{E000}")
        );
        assert_eq!(prefix_upper_bound("a\u{10FFFF}").as_deref(), Some("b"));
        assert_eq!(prefix_upper_bound("\u{10FFFF}"), None);
    }

    #[test]
    fn counts_merge_and_render_like_the_reference() {
        let mut a = CountRange {
            min_current: 3,
            max_current: 3,
            ..CountRange::default()
        };
        a.merge(CountRange {
            min_current: 5,
            max_current: 5,
            min_pending: 1,
            max_pending: 1,
        });
        assert_eq!(a.min_total(), 6);
        assert_eq!(a.suffix(), "(5-8) (+1)");
    }
}
