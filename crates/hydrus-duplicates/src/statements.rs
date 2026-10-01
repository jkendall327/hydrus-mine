//! The duplicate filter's comparison of the file shown against the other
//! (the reference's `ClientDuplicatesComparisonStatements`): a statement
//! per property that differs or is worth saying ("1.2 MB > 800 KB (+50%)",
//! "both are jpegs", "this has exif data, the other does not"), each with a
//! score for how much it counts towards the file shown being the better.
//!
//! The fast statements read only the files' metadata; their total decides
//! which file of a pair the filter shows first. The slow ones read the
//! files (jpeg quality, visual duplicates).

use hydrus_core::numbers::{float_to_percentage, human_bytes, human_int, resolution_text};
use hydrus_core::time::{pretty_time_delta_f64, timestamp_to_pretty_time_delta};
use hydrus_core::{HashId, Mime};
use hydrus_media::jpeg::Subsampling;
use hydrus_media::mimes;
use hydrus_search::media::FileFacts;
use hydrus_store::duplicates::ComparisonScores;

use crate::selector::FileContent;

/// The fast statements, in the order the filter lists them (the reference
/// looks for `software_source_metadata`, a key it never makes, so never
/// shows that one; we show it).
pub const FAST_KEYS: [&str; 16] = [
    "filesize",
    "resolution",
    "ratio",
    "mime",
    "num_tags",
    "time_imported",
    "pixel_duplicates",
    "has_transparency",
    "exif_data",
    "xmp_data",
    "iptc_data",
    "human_readable_metadata",
    "software_source_data",
    "icc_profile",
    "has_audio",
    "duration",
];

/// The slow statements, in the order the filter lists them.
pub const SLOW_KEYS: [&str; 3] = [
    "jpeg_subsampling",
    "jpeg_quality",
    "a_and_b_are_visual_duplicates",
];

/// One line of the comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// Which property (one of [`FAST_KEYS`] or [`SLOW_KEYS`]).
    pub key: &'static str,
    pub text: String,
    /// Positive if it counts for the file shown, negative against.
    pub score: i32,
}

/// The fast statements about a pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastComparison {
    /// In the order the reference makes them.
    pub statements: Vec<Statement>,
    /// The files have the same pixels.
    pub pixel_duplicates: bool,
}

impl FastComparison {
    /// The total score (`GetDuplicateComparisonScoreFast`).
    pub fn score(&self) -> i32 {
        self.statements.iter().map(|s| s.score).sum()
    }
}

/// `COMPLEX_COMPARISON_FILETYPES`: types whose files we can't really
/// compare, counted heavily against.
fn is_complex(mime: Mime) -> bool {
    [
        mimes::IMAGE_PROJECT_FILES,
        mimes::APPLICATIONS,
        mimes::ARCHIVES,
    ]
    .iter()
    .any(|set| set.contains(&mime))
}

/// `HC.NICE_RATIOS`: the name of a common aspect ratio, compared exactly
/// as the reference's float keys are.
#[allow(clippy::float_cmp)]
fn nice_ratio(ratio: f64) -> Option<&'static str> {
    [
        (1.0, "1:1"),
        (4.0 / 3.0, "4:3"),
        (5.0 / 4.0, "5:4"),
        (16.0 / 9.0, "16:9"),
        (21.0 / 9.0, "21:9"),
        (47.0 / 20.0, "2.35:1"),
        (9.0 / 16.0, "9:16"),
        (2.0 / 3.0, "2:3"),
        (4.0 / 5.0, "4:5"),
    ]
    .iter()
    .find(|(r, _)| *r == ratio)
    .map(|(_, name)| *name)
}

/// The statement for a flag either file may have (`has_exif` and the like).
fn flag_statement(
    key: &'static str,
    shown: bool,
    other: bool,
    [both, only_shown, only_other]: [&str; 3],
) -> Option<Statement> {
    let text = match (shown, other) {
        (false, false) => return None,
        (true, true) => both,
        (true, false) => only_shown,
        (false, true) => only_other,
    };
    Some(Statement {
        key,
        text: text.to_owned(),
        score: 0,
    })
}

/// `GetDuplicateComparisonStatementsFast`: `shown` against `other`, ages
/// relative to `now` (in seconds).
#[allow(clippy::float_cmp, clippy::too_many_lines)] // (compared as the reference compares)
pub fn fast(
    shown: &FileFacts,
    other: &FileFacts,
    scores: &ComparisonScores,
    now: i64,
) -> FastComparison {
    let mut out = Vec::new();
    let mut push = |key: &'static str, text: String, score: i32| {
        out.push(Statement { key, text, score });
    };

    let resolution = |f: &FileFacts| (f.width, f.height);
    let mut pixel_duplicates = false;
    if let (Some(s_mime), Some(c_mime)) = (shown.mime, other.mime)
        && mimes::can_have_pixel_hash(s_mime)
        && mimes::can_have_pixel_hash(c_mime)
        && resolution(shown) == resolution(other)
    {
        match (shown.pixel_hash, other.pixel_hash) {
            (Some(s), Some(c)) if s == c && shown.width == other.width => {
                pixel_duplicates = true;
                let (text, score) = match (s_mime, c_mime) {
                    (Mime::ImagePng, Mime::ImageJpeg) => (
                        "this is a pixel-for-pixel duplicate png!\nit is almost certainly a derivative copy!",
                        -100,
                    ),
                    (Mime::ImageJpeg, Mime::ImagePng) => (
                        "other file is a pixel-for-pixel duplicate png!\nthis is almost certainly an original!",
                        100,
                    ),
                    _ => ("images are pixel-for-pixel duplicates!", 0),
                };
                push("pixel_duplicates", text.to_owned(), score);
            }
            (Some(_), Some(_)) => {}
            _ => push(
                "pixel_duplicates",
                "could not determine if files were pixel-for-pixel duplicates!".to_owned(),
                0,
            ),
        }
    }

    if shown.size != other.size
        && let (Some(s), Some(c)) = (shown.size, other.size)
        && s > 0
        && c > 0
    {
        let (sf, cf) = (s as f64, c as f64);
        let ratio = sf.max(cf) / sf.min(cf);
        let (op, mut score) = if ratio > 2.0 {
            if s > c {
                (">>", scores.much_higher_filesize)
            } else {
                ("<<", -scores.much_higher_filesize)
            }
        } else if ratio > 1.05 {
            if s > c {
                (">", scores.higher_filesize)
            } else {
                ("<", -scores.higher_filesize)
            }
        } else {
            ("\u{2248}", 0)
        };
        let sign = if s > c { "+" } else { "" };
        if pixel_duplicates {
            score = 0;
        }
        push(
            "filesize",
            format!(
                "{} {op} {} ({sign}{})",
                human_bytes(s),
                human_bytes(c),
                float_to_percentage(sf / cf - 1.0)
            ),
            score,
        );
    }

    if let (Some(s_w), Some(s_h), Some(c_w), Some(c_h)) =
        (shown.width, shown.height, other.width, other.height)
        && s_w > 0
        && s_h > 0
        && c_w > 0
        && c_h > 0
    {
        if s_w == c_w && s_h == c_h {
            push(
                "resolution",
                format!("both are {}", resolution_text(s_w, s_h)),
                0,
            );
        } else {
            let ratio = (s_w * s_h) as f64 / (c_w * c_h) as f64;
            let (op, score) = if ratio == 1.0 {
                ("!=", 0)
            } else if ratio > 2.0 {
                (">>", scores.much_higher_resolution)
            } else if ratio > 1.0 {
                (">", scores.higher_resolution)
            } else if ratio < 0.5 {
                ("<<", -scores.much_higher_resolution)
            } else {
                ("<", -scores.higher_resolution)
            };
            let text = |w: u64, h: u64| {
                let mut text = resolution_text(w, h);
                if w % 2 == 1 || h % 2 == 1 {
                    text.push_str(" (unusual)");
                }
                text
            };
            push(
                "resolution",
                format!("{} {op} {}", text(s_w, s_h), text(c_w, c_h)),
                score,
            );

            let s_nice = nice_ratio(s_w as f64 / s_h as f64);
            let c_nice = nice_ratio(c_w as f64 / c_h as f64);
            if s_nice.is_some() || c_nice.is_some() {
                let (op, score) = match (s_nice, c_nice) {
                    (Some(_), Some(_)) => ("-", 0),
                    (Some(_), None) => (">", scores.nicer_ratio),
                    _ => ("<", -scores.nicer_ratio),
                };
                let (s_text, c_text) = (s_nice.unwrap_or("unusual"), c_nice.unwrap_or("unusual"));
                let text = if s_text == c_text {
                    format!("both {s_text}")
                } else {
                    format!("{s_text} {op} {c_text}")
                };
                push("ratio", text, score);
            }
        }
    }

    if let (Some(s_mime), Some(c_mime)) = (shown.mime, other.mime) {
        let score = if is_complex(s_mime) || is_complex(c_mime) {
            -100
        } else {
            0
        };
        let text = if s_mime == c_mime {
            format!("both are {}s", s_mime.human_name())
        } else {
            format!("{} vs {}", s_mime.human_name(), c_mime.human_name())
        };
        push("mime", text, score);
    }

    if shown.has_audio || other.has_audio {
        let (text, score) = match (shown.has_audio, other.has_audio) {
            (true, true) => ("both have audio", 0),
            (true, false) => ("this has audio, the other does not", scores.has_audio),
            _ => ("the other has audio, this does not", -scores.has_audio),
        };
        push("has_audio", text.to_owned(), score);
    }

    let (s_tags, c_tags) = (shown.tags.len() as u64, other.tags.len() as u64);
    if s_tags == c_tags {
        push(
            "num_tags",
            format!("both have {} tags", human_int(s_tags)),
            0,
        );
    } else {
        let (op, score) = if s_tags > 0 && c_tags > 0 {
            if s_tags > c_tags {
                (">", scores.more_tags)
            } else {
                ("<", -scores.more_tags)
            }
        } else if s_tags > 0 {
            (">>", scores.more_tags)
        } else {
            ("<<", -scores.more_tags)
        };
        push(
            "num_tags",
            format!("{} tags {op} {} tags", human_int(s_tags), human_int(c_tags)),
            score,
        );
    }

    if let (Some(s_ms), Some(c_ms)) = (shown.imported_ms, other.imported_ms) {
        let (s, c) = (s_ms.div_euclid(1000), c_ms.div_euclid(1000));
        let (mut op, mut score) = if s < c {
            ("older than".to_owned(), scores.older)
        } else {
            ("newer than".to_owned(), -scores.older)
        };
        if (s - c).abs() < 86_400 * 30 {
            op = format!("a little {op}");
            score = 0;
        }
        if pixel_duplicates {
            score = 0;
        }
        push(
            "time_imported",
            format!(
                "{}, {op} {}",
                timestamp_to_pretty_time_delta(s, now, " old"),
                timestamp_to_pretty_time_delta(c, now, " old")
            ),
            score,
        );
    }

    let mut statements = out;
    for (key, (s, c), texts) in [
        (
            "has_transparency",
            (shown.has_transparency, other.has_transparency),
            [
                "both have transparency",
                "this has transparency, the other is opaque",
                "this is opaque, the other has transparency",
            ],
        ),
        (
            "exif_data",
            (shown.has_exif, other.has_exif),
            [
                "both have exif data",
                "this has exif data, the other does not",
                "the other has exif data, this does not",
            ],
        ),
        (
            "xmp_data",
            (shown.has_xmp, other.has_xmp),
            [
                "both have xmp data",
                "this has xmp data, the other does not",
                "the other has xmp data, this does not",
            ],
        ),
        (
            "iptc_data",
            (shown.has_iptc, other.has_iptc),
            [
                "both have iptc data",
                "this has iptc data, the other does not",
                "the other has iptc data, this does not",
            ],
        ),
        (
            "human_readable_metadata",
            (
                shown.has_human_readable_embedded_metadata,
                other.has_human_readable_embedded_metadata,
            ),
            [
                // (said only when they differ)
                "",
                "this has human-readable metadata, the other does not",
                "the other has human-readable metadata, this does not",
            ],
        ),
        (
            "software_source_data",
            (shown.has_software_source, other.has_software_source),
            [
                "both have software/source data",
                "this has software/source metadata, the other does not",
                "the other has software/source metadata, this does not",
            ],
        ),
        (
            "icc_profile",
            (shown.has_icc_profile, other.has_icc_profile),
            [
                "both have icc profile",
                "this has icc profile, the other does not",
                "the other has icc profile, this does not",
            ],
        ),
    ] {
        if key == "human_readable_metadata" && s && c {
            continue;
        }
        statements.extend(flag_statement(key, s, c, texts));
    }

    let duration = |f: &FileFacts| f.duration_ms.filter(|&d| d > 0);
    if duration(shown).is_some() || duration(other).is_some() {
        let seconds = |f: &FileFacts| f.duration_ms.map(|d| d as f64 / 1000.0);
        let (text, score) = match (duration(shown), duration(other)) {
            (Some(_), Some(_)) => {
                let (s, c) = (seconds(shown).unwrap_or(0.0), seconds(other).unwrap_or(0.0));
                if s == c {
                    ("same duration".to_owned(), 0)
                } else {
                    let multiple = s.max(c) / s.min(c);
                    let score = (((multiple - 1.0) * 50.0) as i32).clamp(1, 50);
                    let (op, sign, score) = if s > c {
                        (">", "+", score)
                    } else {
                        ("<", "", -score)
                    };
                    (
                        format!(
                            "{} {op} {} ({sign}{})",
                            pretty_time_delta_f64(s),
                            pretty_time_delta_f64(c),
                            float_to_percentage(s / c - 1.0)
                        ),
                        score,
                    )
                }
            }
            (Some(_), None) => (
                format!(
                    "this has duration ({}), the other does not",
                    pretty_time_delta_f64(seconds(shown).unwrap_or(0.0))
                ),
                0,
            ),
            _ => (
                format!(
                    "the other has duration ({}), this does not",
                    pretty_time_delta_f64(seconds(other).unwrap_or(0.0))
                ),
                0,
            ),
        };
        statements.push(Statement {
            key: "duration",
            text,
            score,
        });
    }

    FastComparison {
        statements,
        pixel_duplicates,
    }
}

/// `GetDuplicateComparisonStatementsSlow`: jpeg quality and visual
/// duplicates, which read the files.
#[allow(clippy::float_cmp)] // (compared as the reference compares)
pub fn slow(
    (shown_id, shown): (HashId, &FileFacts),
    (other_id, other): (HashId, &FileFacts),
    pixel_duplicates: bool,
    scores: &ComparisonScores,
    content: &mut impl FileContent,
) -> Vec<Statement> {
    let mut out = Vec::new();
    let (Some(s_mime), Some(c_mime)) = (shown.mime, other.mime) else {
        return out;
    };

    if s_mime == Mime::ImageJpeg && c_mime == Mime::ImageJpeg {
        let (s, c) = (
            content.jpeg_quality(shown_id),
            content.jpeg_quality(other_id),
        );
        let (s_sub, c_sub) = (
            s.subsampling.relative_quality(),
            c.subsampling.relative_quality(),
        );
        let (text, score) = if s_sub == c_sub {
            (format!("both {}", s.subsampling.name()), 0)
        } else {
            let greyscale =
                s.subsampling == Subsampling::Greyscale || c.subsampling == Subsampling::Greyscale;
            let score = if greyscale {
                0
            } else if s_sub > c_sub {
                10
            } else {
                -10
            };
            (
                format!("{} vs {}", s.subsampling.name(), c.subsampling.name()),
                score,
            )
        };
        out.push(Statement {
            key: "jpeg_subsampling",
            text,
            score,
        });

        if let (Some(s_quality), Some(c_quality)) = (s.quality, c.quality)
            && s_quality > 0.0
            && c_quality > 0.0
        {
            let (s_label, c_label) = (s.quality_label(), c.quality_label());
            let (text, score) = if s_label == c_label {
                (format!("both {s_label} quality"), 0)
            } else {
                // (a lower number is a better quality)
                let ratio = c_quality / s_quality;
                let score = if ratio > 2.0 {
                    scores.much_higher_jpeg_quality
                } else if ratio > 1.0 {
                    scores.higher_jpeg_quality
                } else if ratio < 0.5 {
                    -scores.much_higher_jpeg_quality
                } else {
                    -scores.higher_jpeg_quality
                };
                (format!("{s_label} vs {c_label} jpeg quality"), score)
            };
            out.push(Statement {
                key: "jpeg_quality",
                text,
                score,
            });
        }
    }

    if !pixel_duplicates
        && mimes::IMAGES.contains(&s_mime)
        && mimes::IMAGES.contains(&c_mime)
        && let Some((simple, regional)) = content.visual_comparison(shown_id, other_id)
    {
        let (verdict, score) = match regional {
            Some(regional) if simple.similar => (regional, if regional.similar { 0 } else { -5 }),
            _ => (simple, -10),
        };
        out.push(Statement {
            key: "a_and_b_are_visual_duplicates",
            text: verdict.statement.to_owned(),
            score,
        });
    }
    out
}

/// Each pair's files in the order the duplicate filter shows them
/// (`ABPairsUsingFastComparisonScore`): as given if the first file's fast
/// statements against the second score above zero, else swapped.
pub fn ab_order(
    conn: &rusqlite::Connection,
    snapshot: &hydrus_store::Snapshot,
    pairs: Vec<(HashId, HashId)>,
    scores: &ComparisonScores,
) -> hydrus_store::Result<Vec<(HashId, HashId)>> {
    let ids: Vec<HashId> = pairs
        .iter()
        .flat_map(|&(a, b)| [a, b])
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let facts = hydrus_search::media::load_facts(conn, snapshot, &ids)?;
    Ok(pairs
        .into_iter()
        .map(|(a, b)| match (facts.get(&a), facts.get(&b)) {
            // (the score doesn't depend on now)
            (Some(fa), Some(fb)) if fast(fa, fb, scores, 0).score() > 0 => (a, b),
            _ => (b, a),
        })
        .collect())
}
