//! Turning predicates into an expression over leaves, and evaluating it.
//!
//! [`build`] resolves each predicate against the live client (tag ids,
//! service ids, display rules, timestamps) into an [`Expr`]. [`evaluate`]
//! then computes the matching files:
//!
//! - an AND evaluates its *positive* terms (ones that can list their own
//!   files) cheapest first, each within the files still in play, then the
//!   *scoped* terms (negations, "has none" counts) that can only filter;
//! - the first positive term of the top-level AND runs unrestricted when it
//!   is estimated to be much smaller than the file domain, and its result
//!   is then checked against the domain; otherwise the domain is loaded and
//!   everything runs within it;
//! - an OR is the union of its terms, each only looking at files no earlier
//!   term matched.

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use roaring::RoaringBitmap;

use hydrus_core::{ContentStatus, NamespaceId, ServiceType, TagId};
use hydrus_store::media::FileFlags;
use hydrus_store::services::{Service, ServiceKind};

use super::context::{Domain, Env, Strategy, TagScope, resolve_tags, tag_service};
use super::leaf::{
    CountSource, FileCond, Leaf, NamespaceLookup, Param, TagLookup, TimeSource, UrlLeaf,
};
use super::numbers::{Counts, sql_condition};
use super::sql::PROBE_COST;
use super::tags::{
    self, NamespacePattern, SubtagPattern, TagMatch, stored_tags_displaying, stored_tags_for_search,
};
use super::time::range;
use super::{Result, SearchError, describe, similar};
use crate::filetype::SEARCHABLE_MIMES;
use crate::number::{Comparison, NumberOp, NumberTest, RatingOp, RatioOp, TagNumberOp};
use crate::predicate::{
    FileHashes, FileProperty, NamespaceFilter, NumericProperty, Predicate, RatingLogic, RatingTest,
    ServiceRef, ServiceSelection, SystemPredicate, TagDisplayType, UrlRule, ViewCanvases,
    ViewingStat, Wildcard,
};
use crate::time::TimeKind;

/// A search expression. Every node denotes a set of files; [`Expr::Not`]
/// is relative to the search's file domain.
#[derive(Debug, Clone)]
pub(crate) enum Expr {
    /// Every file in the domain.
    All,
    Nothing,
    Leaf(Leaf),
    Not(Box<Expr>),
    And(Vec<Expr>),
    Or(Vec<Expr>),
}

impl Expr {
    fn not(self) -> Expr {
        match self {
            Expr::All => Expr::Nothing,
            Expr::Nothing => Expr::All,
            Expr::Not(inner) => *inner,
            other => Expr::Not(Box::new(other)),
        }
    }

    fn negate_if(self, negate: bool) -> Expr {
        if negate { self.not() } else { self }
    }
}

/// A conjunction, flattened, with trivial terms removed and every
/// condition on the `files` table merged into one leaf (so they share one
/// pass over the table, as in the reference).
fn and(terms: impl IntoIterator<Item = Expr>) -> Expr {
    let mut out = Vec::new();
    let mut file_conds: Vec<FileCond> = Vec::new();
    for term in terms {
        match term {
            Expr::All => {}
            Expr::Nothing => return Expr::Nothing,
            Expr::And(inner) => match and(inner) {
                Expr::And(inner) => out.extend(inner),
                Expr::Nothing => return Expr::Nothing,
                Expr::All => {}
                other => out.push(other),
            },
            Expr::Leaf(Leaf::FileInfo(conds)) => file_conds.extend(conds),
            other => out.push(other),
        }
    }
    if !file_conds.is_empty() {
        out.push(Expr::Leaf(Leaf::FileInfo(file_conds)));
    }
    match out.len() {
        0 => Expr::All,
        1 => out.pop().expect("one term"),
        _ => Expr::And(out),
    }
}

fn or(terms: impl IntoIterator<Item = Expr>) -> Expr {
    let mut out = Vec::new();
    for term in terms {
        match term {
            Expr::All => return Expr::All,
            Expr::Nothing => {}
            Expr::Or(inner) => out.extend(inner),
            other => out.push(other),
        }
    }
    match out.len() {
        0 => Expr::Nothing,
        1 => out.pop().expect("one term"),
        _ => Expr::Or(out),
    }
}

fn file_cond(sql: impl Into<String>, params: Vec<Param>) -> Expr {
    Expr::Leaf(Leaf::FileInfo(vec![FileCond {
        sql: sql.into(),
        params,
    }]))
}

/// Resolve a search's predicates. Returns the expression and the
/// `system:limit`, if any (the smallest, if several).
pub(crate) fn build(env: &Env<'_>, predicates: &[Predicate]) -> Result<(Expr, Option<u64>)> {
    let mut limit: Option<u64> = None;
    let mut terms = Vec::with_capacity(predicates.len());
    for predicate in predicates {
        if let Predicate::System(SystemPredicate::Limit(n)) = predicate {
            limit = Some(limit.map_or(*n, |l| l.min(*n)));
            continue;
        }
        terms.push(predicate_expr(env, predicate)?);
    }
    Ok((and(terms), limit))
}

fn predicate_expr(env: &Env<'_>, predicate: &Predicate) -> Result<Expr> {
    Ok(match predicate {
        Predicate::Tag { tag, inclusive } => match tags::tag_id(env.conn, tag)? {
            Some(id) => stored_tags_expr(&env.tags, |graph| stored_tags_for_search(graph, id)),
            None => Expr::Nothing,
        }
        .negate_if(!inclusive),
        Predicate::Namespace {
            namespace,
            inclusive,
        } => {
            let ids = NamespacePattern::parse(namespace).namespace_ids(env.conn)?;
            namespace_expr(env, ids)?.negate_if(!inclusive)
        }
        Predicate::Wildcard { pattern, inclusive } => {
            wildcard_expr(env, pattern)?.negate_if(!inclusive)
        }
        // a limit only means something for the whole search
        Predicate::Or(terms) => or(terms
            .iter()
            .map(|t| match t {
                Predicate::System(SystemPredicate::Limit(_)) => Ok(Expr::All),
                t => predicate_expr(env, t),
            })
            .collect::<Result<Vec<_>>>()?),
        Predicate::System(system) => system_expr(env, system)?,
    })
}

/// Files with any of the stored tags `stored_for(graph)` in each searched
/// service, in each searched status.
fn stored_tags_expr(
    scope: &TagScope,
    stored_for: impl Fn(&hydrus_store::display::DisplayGraph) -> Vec<TagId>,
) -> Expr {
    let mut lookups = Vec::new();
    for service in &scope.services {
        let tags: Arc<[TagId]> = stored_for(&service.graph).into();
        if tags.is_empty() {
            continue;
        }
        for &status in &scope.statuses {
            lookups.push(TagLookup {
                service: service.id,
                status,
                tags: Arc::clone(&tags),
            });
        }
    }
    if lookups.is_empty() {
        Expr::Nothing
    } else {
        Expr::Leaf(Leaf::StoredTags(lookups))
    }
}

/// Files that display a tag in one of `namespaces` (`None`: any tag).
fn namespace_expr(env: &Env<'_>, namespaces: Option<Vec<NamespaceId>>) -> Result<Expr> {
    let Some(namespaces) = namespaces else {
        return Ok(any_tag(&env.tags));
    };
    if namespaces.is_empty() {
        return Ok(Expr::Nothing);
    }
    let wanted: HashSet<NamespaceId> = namespaces.iter().copied().collect();
    let namespaces: Arc<[NamespaceId]> = namespaces.into();
    let mut lookups = Vec::new();
    for service in &env.tags.services {
        let mut excluded = HashSet::new();
        let mut added = Vec::new();
        if !service.graph.is_empty() {
            let graph_ns = env.graph_namespaces()?;
            let in_wanted = |t: &TagId| graph_ns.get(t).is_some_and(|ns| wanted.contains(ns));
            for tag in service.graph.all_tags() {
                let own = in_wanted(&tag);
                let displayed = service.graph.display_tags(tag).any(|d| in_wanted(&d));
                if own && !displayed {
                    excluded.insert(tag);
                } else if !own && displayed {
                    added.push(tag);
                }
            }
        }
        added.sort_unstable();
        let excluded = Arc::new(excluded);
        let added: Arc<[TagId]> = added.into();
        for &status in &env.tags.statuses {
            lookups.push(NamespaceLookup {
                service: service.id,
                status,
                namespaces: Arc::clone(&namespaces),
                excluded: Arc::clone(&excluded),
                added: Arc::clone(&added),
            });
        }
    }
    Ok(if lookups.is_empty() {
        Expr::Nothing
    } else {
        Expr::Leaf(Leaf::Namespaces(lookups))
    })
}

fn any_tag(scope: &TagScope) -> Expr {
    let tables: Vec<String> = scope.tables().map(|(_, t)| t.to_owned()).collect();
    if tables.is_empty() {
        Expr::Nothing
    } else {
        Expr::Leaf(Leaf::AnyTag(tables))
    }
}

fn wildcard_expr(env: &Env<'_>, pattern: &Wildcard) -> Result<Expr> {
    let namespace = NamespacePattern::parse(pattern.namespace());
    let subtag = SubtagPattern::parse(pattern.subtag());
    Ok(match tags::matching_tags(env.conn, &namespace, &subtag)? {
        TagMatch::InNamespaces(namespaces) => namespace_expr(env, namespaces)?,
        TagMatch::Tags(matched) => {
            stored_tags_expr(&env.tags, |graph| stored_tags_displaying(graph, &matched))
        }
    })
}

/// The namespaces a tag-counting predicate looks at (`None`: all).
fn namespace_filter(env: &Env<'_>, filter: &NamespaceFilter) -> Result<Option<Vec<NamespaceId>>> {
    let pattern = match filter {
        NamespaceFilter::Any => return Ok(None),
        NamespaceFilter::Unnamespaced => NamespacePattern::Exact(String::new()),
        NamespaceFilter::Namespace(ns) => NamespacePattern::parse(ns),
    };
    Ok(Some(pattern.namespace_ids(env.conn)?.unwrap_or_default()))
}

/// The number test a legacy comparison means for tag, note and URL counts
/// (the reference builds a `NumberTest` from the operator character).
fn comparison_test(op: Comparison, value: u64) -> NumberTest {
    let op = match op {
        Comparison::Less => NumberOp::Less,
        Comparison::Greater => NumberOp::Greater,
        Comparison::Equal => NumberOp::Equal,
        Comparison::NotEqual => NumberOp::NotEqual,
        Comparison::Approx => NumberOp::ApproxPercent {
            percent: crate::number::DEFAULT_APPROX_PERCENT,
        },
    };
    NumberTest::new(op, value)
}

#[allow(clippy::too_many_lines)]
fn system_expr(env: &Env<'_>, predicate: &SystemPredicate) -> Result<Expr> {
    use SystemPredicate as S;
    Ok(match predicate {
        S::Everything | S::Limit(_) => Expr::All,
        S::Inbox => Expr::Leaf(Leaf::Inbox),
        // archived files are local files that are not in the inbox
        S::Archive => and([Expr::Leaf(Leaf::Inbox).not(), local_expr(env)]),
        S::Local => local_expr(env),
        S::NotLocal => local_expr(env).not(),
        S::Filetype {
            filetypes,
            inclusive,
        } => {
            let mut mimes = filetypes.specific_mimes();
            if !inclusive {
                mimes = SEARCHABLE_MIMES
                    .iter()
                    .copied()
                    .filter(|m| !mimes.contains(m))
                    .collect();
            }
            if mimes.is_empty() {
                Expr::Nothing
            } else {
                file_cond(
                    "coalesce(forced_mime, mime) IN rarray(?)",
                    vec![Param::Ints(
                        mimes.iter().map(|m| i64::from(m.code())).collect(),
                    )],
                )
            }
        }
        S::Hash { hashes, inclusive } => {
            Expr::Leaf(Leaf::Files(hash_ids(env, hashes)?)).negate_if(!inclusive)
        }
        S::SimilarToFiles {
            files,
            max_distance,
        } => Expr::Leaf(Leaf::Files(similar::similar_to_files(
            env.conn,
            files,
            *max_distance,
        )?)),
        S::SimilarToData {
            pixel_hashes,
            perceptual_hashes,
            max_distance,
        } => Expr::Leaf(Leaf::Files(similar::similar_to_data(
            env.conn,
            pixel_hashes,
            perceptual_hashes,
            *max_distance,
        )?)),
        S::FileProperty { property, has } => file_property(*property, *has),
        S::Number { property, test } => number(*property, *test),
        S::NumTags {
            namespace,
            op,
            count,
        } => num_tags(env, namespace, comparison_test(*op, *count))?,
        S::FileSize { op, size, unit } => {
            let bytes = size.saturating_mul(unit.bytes());
            legacy_file_comparison("size", *op, bytes)
        }
        S::NumPixels { op, count, unit } => {
            let pixels = count.saturating_mul(unit.pixels());
            legacy_file_comparison("width * height", *op, pixels)
        }
        S::Ratio { op, width, height } => ratio(*op, *width, *height),
        S::Time { kind, test } => {
            let range = range(*test, env.clock);
            if range.is_unbounded() {
                Expr::All
            } else {
                let source = match kind {
                    TimeKind::Imported => match &env.domain {
                        Domain::Tables { tables, .. } => TimeSource::Imported(tables.clone()),
                        // the reference has no import time outside a domain
                        Domain::AllKnownFiles => return Ok(Expr::All),
                    },
                    TimeKind::Modified => TimeSource::Modified,
                    TimeKind::LastViewed => TimeSource::LastViewed,
                    TimeKind::Archived => TimeSource::Archived,
                };
                Expr::Leaf(Leaf::Time { source, range })
            }
        }
        S::FileService {
            service,
            status,
            is_in,
        } => {
            let service = resolve_service(
                env,
                service,
                |t| t.is_file_service() && t != ServiceType::CombinedFile,
                "file",
            )?;
            Expr::Leaf(Leaf::FileStatus {
                service: service.id,
                status: *status,
            })
            .negate_if(!is_in)
        }
        S::FileRelationshipCount {
            op,
            count,
            relationship,
        } => Expr::Leaf(Leaf::Count {
            source: CountSource::Relationships(*relationship),
            accept: Counts::from_comparison(*op, *count, |v| {
                Counts::open_float_range(0.8 * v as f64, 1.2 * v as f64)
            }),
        }),
        S::BestQualityOfGroup { is_best } => Expr::Leaf(Leaf::NonKings).negate_if(*is_best),
        S::FileViewingStats {
            stat,
            canvases,
            op,
            value,
        } => {
            // The reference treats '< 1' as zero before converting viewtime
            // seconds to milliseconds. A fractional 1.001 remains a normal
            // comparison even when its DB threshold truncates to 1000 ms.
            let only_zero = *op == Comparison::Less
                && *value
                    == match stat {
                        ViewingStat::Views | ViewingStat::ViewTime => 1,
                        ViewingStat::ViewTimeMilliseconds => 1000,
                    };
            let canvases = match canvases {
                ViewCanvases::Default => env.viewing.interesting_canvases.clone(),
                ViewCanvases::Specific(set) => set.iter().map(|c| c.canvas_type()).collect(),
            };
            let viewtime = *stat != ViewingStat::Views;
            let value = match stat {
                ViewingStat::Views => *value,
                ViewingStat::ViewTime => value.saturating_mul(1000),
                ViewingStat::ViewTimeMilliseconds => {
                    // Qt composes integer seconds plus milliseconds / 1000;
                    // the DB then truncates seconds * 1000. Preserve that
                    // floating-point boundary (e.g. 1.001 becomes 1000 ms).
                    let seconds = (value / 1000) as f64 + (value % 1000) as f64 / 1000.0;
                    (seconds * 1000.0) as u64
                }
            };
            Expr::Leaf(Leaf::Count {
                source: CountSource::Views { canvases, viewtime },
                accept: if only_zero {
                    Counts::exactly(0)
                } else {
                    Counts::from_comparison(*op, value, |v| {
                        Counts::range((0.8 * v as f64) as u64, (1.2 * v as f64) as u64)
                    })
                },
            })
        }
        S::KnownUrl { rule, has } => url_expr(env, rule)?.negate_if(!has),
        S::TagAsNumber {
            namespace,
            op,
            value,
        } => tag_as_number(env, namespace, *op, *value)?,
        S::NoteName { name, has } => match hydrus_store::master::label_id(env.conn, name)? {
            Some(label) => Expr::Leaf(Leaf::NoteName(label)),
            None => Expr::Nothing,
        }
        .negate_if(!has),
        S::Rating { service, test } => rating(env, service, *test)?,
        S::RatingAdvanced {
            logic,
            services,
            rated,
        } => rating_advanced(env, logic, services, *rated)?,
        S::TagAdvanced {
            service,
            display,
            statuses,
            tag,
            inclusive,
        } => tag_advanced(env, service.as_ref(), *display, statuses, tag)?.negate_if(!inclusive),
    })
}

/// "Stored locally": every file of a local domain is.
fn local_expr(env: &Env<'_>) -> Expr {
    if env.location.covered_by_local_storage {
        return Expr::All;
    }
    if env.location.local_storage_deleted_only {
        return Expr::Nothing;
    }
    match env.service_of_type(ServiceType::HydrusLocalFileStorage) {
        Some(local) => Expr::Leaf(Leaf::FileStatus {
            service: local.id,
            status: ContentStatus::Current,
        }),
        None => Expr::Nothing,
    }
}

fn hash_ids(env: &Env<'_>, hashes: &FileHashes) -> Result<RoaringBitmap> {
    let conn = env.conn;
    let mut out = RoaringBitmap::new();
    match hashes {
        FileHashes::Sha256(hashes) => {
            let hashes: Vec<_> = hashes.iter().copied().collect();
            out.extend(
                hydrus_store::master::hash_ids(conn, &hashes)?
                    .values()
                    .map(|id| id.get()),
            );
        }
        other => {
            let (kind, raw): (_, Vec<Vec<u8>>) = match other {
                FileHashes::Md5(h) => (
                    hydrus_core::HashKind::Md5,
                    h.iter().map(|h| h.0.to_vec()).collect(),
                ),
                FileHashes::Sha1(h) => (
                    hydrus_core::HashKind::Sha1,
                    h.iter().map(|h| h.0.to_vec()).collect(),
                ),
                FileHashes::Sha512(h) => (
                    hydrus_core::HashKind::Sha512,
                    h.iter().map(|h| h.0.to_vec()).collect(),
                ),
                FileHashes::Sha256(_) => unreachable!("handled above"),
            };
            let pairs = hydrus_store::master::convert_hashes(
                conn,
                kind,
                hydrus_core::HashKind::Sha256,
                &raw,
            )?;
            let sha256: Vec<hydrus_core::Sha256> = pairs
                .iter()
                .filter_map(|(_, to)| hydrus_core::Sha256::from_slice(to).ok())
                .collect();
            out.extend(
                hydrus_store::master::hash_ids(conn, &sha256)?
                    .values()
                    .map(|id| id.get()),
            );
        }
    }
    Ok(out)
}

fn file_property(property: FileProperty, has: bool) -> Expr {
    let flag = |bit: u32| Expr::Leaf(Leaf::Flag(bit)).negate_if(!has);
    match property {
        FileProperty::Audio => file_cond("has_audio = ?", vec![Param::Int(i64::from(has))]),
        FileProperty::ForcedFiletype => file_cond(
            if has {
                "forced_mime IS NOT NULL"
            } else {
                "forced_mime IS NULL"
            },
            vec![],
        ),
        FileProperty::Exif => flag(FileFlags::EXIF),
        FileProperty::IccProfile => flag(FileFlags::ICC_PROFILE),
        FileProperty::HumanReadableEmbeddedMetadata => flag(FileFlags::HUMAN_READABLE_METADATA),
        FileProperty::Transparency => flag(FileFlags::TRANSPARENCY),
        FileProperty::Xmp => flag(FileFlags::XMP),
        FileProperty::Iptc => flag(FileFlags::IPTC),
        FileProperty::SoftwareSourceMetadata => flag(FileFlags::SOFTWARE_SOURCE),
    }
}

fn number(property: NumericProperty, test: NumberTest) -> Expr {
    let column = match property {
        NumericProperty::Width => "width",
        NumericProperty::Height => "height",
        NumericProperty::Duration => "duration_ms",
        NumericProperty::NumFrames => "num_frames",
        NumericProperty::NumWords => "num_words",
        NumericProperty::Framerate => "((num_frames * 1.0) / (duration_ms / 1000.0))",
        NumericProperty::NumNotes => {
            return Expr::Leaf(Leaf::Count {
                source: CountSource::Notes,
                accept: Counts::from_number_test(test),
            });
        }
        NumericProperty::NumUrls => {
            return Expr::Leaf(Leaf::Count {
                source: CountSource::Urls,
                accept: Counts::from_number_test(test),
            });
        }
    };
    let (sql, params) = sql_condition(column, test);
    file_cond(sql, params.into_iter().map(Param::Real).collect())
}

/// A size or pixel-count comparison: "about" is 15% either side, exclusive.
fn legacy_file_comparison(expr: &str, op: Comparison, value: u64) -> Expr {
    let value = i64::try_from(value).unwrap_or(i64::MAX);
    let (sql, params) = match op {
        Comparison::Less => (format!("{expr} < ?"), vec![value]),
        Comparison::Greater => (format!("{expr} > ?"), vec![value]),
        Comparison::Equal => (format!("{expr} = ?"), vec![value]),
        Comparison::NotEqual => (format!("{expr} != ?"), vec![value]),
        Comparison::Approx => (
            format!("({expr} > ? AND {expr} < ?)"),
            vec![(value as f64 * 0.85) as i64, (value as f64 * 1.15) as i64],
        ),
    };
    file_cond(sql, params.into_iter().map(Param::Int).collect())
}

fn ratio(op: RatioOp, width: u64, height: u64) -> Expr {
    if height == 0 {
        return Expr::Nothing;
    }
    let (w, h) = (width as f64, height as f64);
    let ratio = "((width * 1.0) / height)";
    let (sql, params) = match op {
        RatioOp::Equal => (format!("{ratio} = ?"), vec![w / h]),
        RatioOp::NotEqual => (format!("{ratio} != ?"), vec![w / h]),
        RatioOp::WiderThan => (format!("{ratio} > ?"), vec![w / h]),
        RatioOp::TallerThan => (format!("{ratio} < ?"), vec![w / h]),
        RatioOp::Approx => (
            format!("({ratio} > ? AND {ratio} < ?)"),
            vec![(w * 0.85) / h, (w * 1.15) / h],
        ),
    };
    file_cond(sql, params.into_iter().map(Param::Real).collect())
}

fn num_tags(env: &Env<'_>, namespace: &NamespaceFilter, test: NumberTest) -> Result<Expr> {
    let accept = Counts::from_number_test(test);
    let namespaces = namespace_filter(env, namespace)?;
    // "has tags" and "no tags" need no counting
    let any = Counts::at_least(1);
    let none = Counts::exactly(0);
    if accept == any || accept == none {
        let has = match namespaces {
            None => any_tag(&env.tags),
            Some(ns) => namespace_expr(env, Some(ns))?,
        };
        return Ok(has.negate_if(accept == none));
    }
    Ok(Expr::Leaf(Leaf::Count {
        source: CountSource::DisplayTags(namespaces.map(Into::into)),
        accept,
    }))
}

fn tag_as_number(
    env: &Env<'_>,
    namespace: &NamespaceFilter,
    op: TagNumberOp,
    value: i64,
) -> Result<Expr> {
    let v = value as f64;
    let (above, below) = match op {
        TagNumberOp::Less => (None, Some(value)),
        TagNumberOp::Greater => (Some(value), None),
        TagNumberOp::Approx => (Some((v * 0.85) as i64), Some((v * 1.15) as i64)),
    };
    let mut stmt = env.conn.prepare_cached(
        "SELECT subtag_id FROM cache_integer_subtags WHERE value > ?1 AND value < ?2",
    )?;
    let subtags: Vec<hydrus_core::SubtagId> = stmt
        .query_map(
            [above.unwrap_or(i64::MIN), below.unwrap_or(i64::MAX)],
            |r| r.get(0),
        )?
        .collect::<rusqlite::Result<_>>()?;
    let namespaces = namespace_filter(env, namespace)?;
    let matched = tags::tags_from_parts(env.conn, namespaces.as_deref(), &subtags)?;
    Ok(stored_tags_expr(&env.tags, |graph| {
        stored_tags_displaying(graph, &matched)
    }))
}

fn url_expr(env: &Env<'_>, rule: &UrlRule) -> Result<Expr> {
    Ok(match rule {
        UrlRule::ExactMatch(url) => match hydrus_store::master::url_id(env.conn, url)? {
            Some(id) => Expr::Leaf(Leaf::Url(UrlLeaf::Exact(id))),
            None => Expr::Nothing,
        },
        UrlRule::Domain(domain) => {
            // the domain itself, or any subdomain of it
            let suffix = format!(".{domain}");
            let mut stmt = env
                .conn
                .prepare_cached("SELECT domain_id, domain FROM url_domains")?;
            let mut rows = stmt.query([])?;
            let mut ids = Vec::new();
            while let Some(row) = rows.next()? {
                let d: String = row.get(1)?;
                if d == *domain || d.ends_with(&suffix) {
                    ids.push(row.get(0)?);
                }
            }
            Expr::Leaf(Leaf::Url(UrlLeaf::Domains(ids)))
        }
        UrlRule::Regex(pattern) => {
            let regex =
                fancy_regex::Regex::new(pattern).map_err(|e| SearchError::InvalidRegex {
                    pattern: pattern.clone(),
                    reason: e.to_string(),
                })?;
            Expr::Leaf(Leaf::Url(UrlLeaf::Regex(Arc::new(regex))))
        }
        UrlRule::UrlClass(name) => {
            let settings = env.snapshot.url_classes.settings();
            let class = settings
                .class_by_name(name)
                .ok_or_else(|| SearchError::UnknownUrlClass(name.clone()))?;
            Expr::Leaf(Leaf::Url(UrlLeaf::Class {
                class: Arc::new(class.clone()),
                collapse_leading_slashes: settings.collapse_leading_slashes,
            }))
        }
    })
}

/// Find a service a predicate names, among the services `allowed` accepts:
/// an exact name match first, then a case-insensitive one.
fn resolve_service<'e>(
    env: &'e Env<'_>,
    service: &ServiceRef,
    allowed: impl Fn(ServiceType) -> bool,
    kinds: &'static str,
) -> Result<&'e Arc<Service>> {
    let unknown = || SearchError::UnknownService {
        service: describe(service),
        kinds,
    };
    match service {
        ServiceRef::Key(key) => env
            .snapshot
            .services
            .by_key(key)
            .ok()
            .filter(|s| allowed(s.service_type()))
            .ok_or_else(unknown),
        ServiceRef::Name(name) => {
            let candidates = || {
                env.snapshot
                    .services
                    .all()
                    .filter(|s| allowed(s.service_type()))
            };
            candidates()
                .find(|s| s.name == *name)
                .or_else(|| candidates().find(|s| s.name.to_lowercase() == name.to_lowercase()))
                .ok_or_else(unknown)
        }
    }
}

fn rating(env: &Env<'_>, service: &ServiceRef, test: RatingTest) -> Result<Expr> {
    let service = resolve_service(env, service, ServiceType::is_rating_service, "rating")?;
    let id = service.id;
    Ok(match &service.kind {
        ServiceKind::RatingLike(_) | ServiceKind::RatingNumerical(_) => {
            let numerical = match &service.kind {
                ServiceKind::RatingNumerical(config) => Some(config),
                _ => None,
            };
            // stars typed against a numerical service mean its own stars
            let as_rating = |stars: u64| match numerical {
                Some(config) => {
                    let stars = stars.min(u64::from(config.num_stars)) as u32;
                    config.rating(stars.max(config.min_stars()))
                }
                None => stars as f64,
            };
            let (op, value) = match test {
                RatingTest::Rated => return Ok(Expr::Leaf(Leaf::Rated(id))),
                RatingTest::NotRated => return Ok(Expr::Leaf(Leaf::Rated(id)).not()),
                RatingTest::Liked => (RatingOp::Equal, 1.0),
                RatingTest::Disliked => (RatingOp::Equal, 0.0),
                RatingTest::Stars { op, stars, .. } => (op, as_rating(stars)),
                RatingTest::Count { op, value } => (op, as_rating(value)),
            };
            let half = match numerical {
                Some(config) => {
                    let choices =
                        f64::from(config.num_stars) + f64::from(u8::from(config.allow_zero));
                    if choices > 1.0 {
                        1.0 / (choices - 1.0) / 2.0
                    } else {
                        0.5
                    }
                }
                None => 0.5,
            };
            let (above, below) = match op {
                RatingOp::Approx => (
                    Some(((value - half) * 0.8, false)),
                    Some(((value + half) * 1.2, false)),
                ),
                RatingOp::Less => (None, Some((value - half, true))),
                RatingOp::LessOrEqual => (None, Some((value + half, true))),
                RatingOp::Greater => (Some((value + half, false)), None),
                RatingOp::GreaterOrEqual => (Some((value - half, false)), None),
                RatingOp::Equal => (Some((value - half, false)), Some((value + half, true))),
            };
            Expr::Leaf(Leaf::RatingRange {
                service: id,
                above,
                below,
            })
        }
        ServiceKind::RatingIncDec(_) => {
            let accept = match test {
                RatingTest::Rated => Counts::at_least(1),
                RatingTest::NotRated | RatingTest::Disliked => Counts::exactly(0),
                RatingTest::Liked => Counts::exactly(1),
                RatingTest::Stars { op, stars: v, .. } | RatingTest::Count { op, value: v } => {
                    match op {
                        RatingOp::Equal => Counts::exactly(v),
                        RatingOp::Less => Counts::below(v),
                        RatingOp::Greater => Counts::above(v),
                        RatingOp::LessOrEqual => Counts::range(0, v),
                        RatingOp::GreaterOrEqual => Counts::at_least(v),
                        RatingOp::Approx => Counts::range(
                            v.saturating_sub(1).max((v as f64 * 0.8) as u64),
                            v.saturating_add(1).min((v as f64 * 1.2) as u64),
                        ),
                    }
                }
            };
            Expr::Leaf(Leaf::Count {
                source: CountSource::IncDec(id),
                accept,
            })
        }
        // remote rating services have no local ratings
        _ => Expr::Nothing,
    })
}

fn rating_advanced(
    env: &Env<'_>,
    logic: &RatingLogic,
    services: &ServiceSelection,
    rated: bool,
) -> Result<Expr> {
    let primary = rating_services(env, services)?;
    let rated_on = |s: &Arc<Service>| match s.kind {
        ServiceKind::RatingIncDec(_) => Expr::Leaf(Leaf::Count {
            source: CountSource::IncDec(s.id),
            accept: Counts::at_least(1),
        }),
        _ => Expr::Leaf(Leaf::Rated(s.id)),
    };
    // with no services, "rated on all/any of them" matches nothing
    let all_rated = |set: &[&Arc<Service>]| {
        if set.is_empty() {
            Expr::Nothing
        } else {
            and(set.iter().map(|s| rated_on(s)))
        }
    };
    let any_rated = |set: &[&Arc<Service>]| or(set.iter().map(|s| rated_on(s)));
    Ok(match (logic, rated) {
        (RatingLogic::All, true) => all_rated(&primary),
        (RatingLogic::All, false) => any_rated(&primary).not(),
        (RatingLogic::Any, true) => any_rated(&primary),
        (RatingLogic::Any, false) => all_rated(&primary).not(),
        (RatingLogic::Only { amongst }, rated) => {
            let primary_ids: HashSet<_> = primary.iter().map(|s| s.id).collect();
            let secondary: Vec<&Arc<Service>> = rating_services(env, amongst)?
                .into_iter()
                .filter(|s| !primary_ids.contains(&s.id))
                .collect();
            if rated {
                and([all_rated(&primary), any_rated(&secondary).not()])
            } else {
                and([any_rated(&primary).not(), all_rated(&secondary)])
            }
        }
    })
}

fn rating_services<'e>(
    env: &'e Env<'_>,
    selection: &ServiceSelection,
) -> Result<Vec<&'e Arc<Service>>> {
    let local = |t: ServiceType| t.is_local_rating_service();
    match selection {
        ServiceSelection::Types(types) => Ok(env
            .snapshot
            .services
            .all()
            .filter(|s| local(s.service_type()) && types.contains(&s.service_type()))
            .collect()),
        ServiceSelection::Names(names) => {
            let mut out: Vec<&Arc<Service>> = Vec::new();
            for name in names {
                let service =
                    resolve_service(env, &ServiceRef::Name(name.clone()), local, "rating")?;
                if !out.iter().any(|s| s.id == service.id) {
                    out.push(service);
                }
            }
            Ok(out)
        }
        ServiceSelection::Keys(keys) => keys
            .iter()
            .map(|key| resolve_service(env, &ServiceRef::Key(key.clone()), local, "rating"))
            .collect(),
    }
}

fn tag_advanced(
    env: &Env<'_>,
    service: Option<&ServiceRef>,
    display: TagDisplayType,
    statuses: &BTreeSet<ContentStatus>,
    tag: &hydrus_core::Tag,
) -> Result<Expr> {
    let scope = match service {
        None => env.tags.clone(),
        Some(service) => {
            let service = resolve_service(env, service, ServiceType::is_tag_service, "tag")?;
            let context = crate::context::TagContext::new(service.key.clone(), true, false);
            let mut scope = resolve_tags(env.snapshot, &context)?;
            scope.statuses.clear();
            scope
        }
    };
    let Some(tag_id) = tags::tag_id(env.conn, tag)? else {
        return Ok(Expr::Nothing);
    };
    let mut lookups = Vec::new();
    for service in &scope.services {
        let service = tag_service(env.snapshot, service.id);
        for &status in statuses {
            let stored: Vec<TagId> = match (display, status) {
                (TagDisplayType::Storage, _) => vec![tag_id],
                (TagDisplayType::Display, ContentStatus::Current | ContentStatus::Pending) => {
                    service.graph.stored_tags_for(tag_id)
                }
                // deleted and petitioned mappings have no display: use siblings
                (TagDisplayType::Display, _) => {
                    let mut chain = service.graph.chain(tag_id);
                    chain.sort_unstable();
                    chain
                }
            };
            lookups.push(TagLookup {
                service: service.id,
                status,
                tags: stored.into(),
            });
        }
    }
    Ok(if lookups.is_empty() {
        Expr::Nothing
    } else {
        Expr::Leaf(Leaf::StoredTags(lookups))
    })
}

/// A set of files, and whether it is known to be inside the domain.
struct Evaluated {
    files: RoaringBitmap,
    in_domain: bool,
}

impl Evaluated {
    fn in_domain(files: RoaringBitmap) -> Self {
        Self {
            files,
            in_domain: true,
        }
    }
}

/// The files in the domain that match `expr`.
pub(crate) fn evaluate(env: &Env<'_>, expr: &Expr) -> Result<RoaringBitmap> {
    let result = eval(env, expr, None)?;
    if result.in_domain {
        Ok(result.files)
    } else {
        env.restrict_to_domain(result.files)
    }
}

/// Whether an expression can only filter given files (it may match files
/// that have no rows in any index).
fn needs_scope(expr: &Expr) -> bool {
    match expr {
        Expr::All | Expr::Not(_) => true,
        Expr::Nothing => false,
        Expr::Leaf(leaf) => leaf.needs_scope(),
        Expr::And(terms) => terms.iter().all(needs_scope),
        Expr::Or(terms) => terms.iter().any(needs_scope),
    }
}

/// Roughly how many files evaluating `expr` unrestricted touches.
fn estimate(env: &Env<'_>, expr: &Expr) -> Result<Option<u64>> {
    Ok(match expr {
        Expr::All => env.domain_size()?,
        Expr::Nothing => Some(0),
        Expr::Leaf(leaf) => leaf.estimate(env)?,
        Expr::Not(_) => None,
        Expr::And(terms) => {
            let mut best: Option<u64> = None;
            for term in terms.iter().filter(|t| !needs_scope(t)) {
                if let Some(e) = estimate(env, term)? {
                    best = Some(best.map_or(e, |b| b.min(e)));
                }
            }
            best
        }
        Expr::Or(terms) => {
            let mut total = 0u64;
            for term in terms {
                match estimate(env, term)? {
                    Some(e) => total = total.saturating_add(e),
                    None => return Ok(None),
                }
            }
            Some(total)
        }
    })
}

/// Evaluate `expr`. Within `within` (a subset of the domain) the result is
/// a subset of it; unrestricted, the result may include files outside the
/// domain unless it says otherwise.
fn eval(env: &Env<'_>, expr: &Expr, within: Option<&RoaringBitmap>) -> Result<Evaluated> {
    use super::leaf::Scope;
    if let Some(w) = within {
        let files = match expr {
            Expr::All => w.clone(),
            Expr::Nothing => RoaringBitmap::new(),
            Expr::Leaf(leaf) => leaf.eval(env, Scope::Within(w))?,
            Expr::Not(inner) => w - eval(env, inner, Some(w))?.files,
            Expr::And(terms) => return eval_and(env, terms, Some(w)),
            Expr::Or(terms) => {
                let mut matched = RoaringBitmap::new();
                for term in terms {
                    let remaining = w - &matched;
                    if remaining.is_empty() {
                        break;
                    }
                    matched |= eval(env, term, Some(&remaining))?.files;
                }
                matched
            }
        };
        return Ok(Evaluated::in_domain(files));
    }
    Ok(match expr {
        Expr::All => Evaluated::in_domain(env.domain_files()?.clone()),
        Expr::Nothing => Evaluated::in_domain(RoaringBitmap::new()),
        Expr::And(terms) => eval_and(env, terms, None)?,
        Expr::Leaf(leaf) if !leaf.needs_scope() => Evaluated {
            files: leaf.eval(env, Scope::All)?,
            in_domain: false,
        },
        Expr::Or(terms) if !terms.iter().any(needs_scope) => {
            let mut files = RoaringBitmap::new();
            let mut in_domain = true;
            for term in terms {
                let e = eval(env, term, None)?;
                files |= e.files;
                in_domain &= e.in_domain;
            }
            Evaluated { files, in_domain }
        }
        // negations and filters need the whole domain to work in
        other => {
            let domain = env.domain_files()?;
            eval(env, other, Some(domain))?
        }
    })
}

fn eval_and(env: &Env<'_>, terms: &[Expr], within: Option<&RoaringBitmap>) -> Result<Evaluated> {
    let mut positive: Vec<(&Expr, Option<u64>)> = Vec::new();
    let mut scoped: Vec<&Expr> = Vec::new();
    for term in terms {
        if needs_scope(term) {
            scoped.push(term);
        } else {
            positive.push((term, estimate(env, term)?));
        }
    }
    // cheapest first; unknown sizes last
    positive.sort_by_key(|(_, e)| e.unwrap_or(u64::MAX));

    let mut current = if let Some(w) = within {
        w.clone()
    } else {
        let first_is_small = match (env.strategy, positive.first()) {
            (_, None) | (Strategy::AlwaysScan, _) | (Strategy::Auto, Some((_, None))) => false,
            (Strategy::AlwaysProbe, Some(_)) => true,
            (Strategy::Auto, Some((_, Some(e)))) => match env.domain_size()? {
                Some(domain) => (*e as f64) * PROBE_COST < domain as f64,
                None => true,
            },
        };
        if first_is_small {
            let (first, _) = positive.remove(0);
            let e = eval(env, first, None)?;
            if e.in_domain {
                e.files
            } else {
                env.restrict_to_domain(e.files)?
            }
        } else {
            env.domain_files()?.clone()
        }
    };
    for term in positive.iter().map(|(t, _)| *t).chain(scoped) {
        if current.is_empty() {
            break;
        }
        current = eval(env, term, Some(&current))?.files;
    }
    Ok(Evaluated::in_domain(current))
}
