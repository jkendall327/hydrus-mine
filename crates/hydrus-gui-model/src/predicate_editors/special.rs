//! The editors with controls of their own: "system:filetype"'s tree of
//! filetypes, "system:hash"'s hashes (with the reference's clean-up
//! buttons), "system:rating"'s panels, one for each rating service with
//! the advanced all/any/only panel over them, and "system:similar
//! files"'s hashes.

use std::collections::BTreeSet;

use hydrus_core::search::filetype::FiletypeSet;
use hydrus_core::search::number::RatingOp;
use hydrus_core::search::predicate::{
    FileHashes, Predicate, RatingLogic, RatingTest, ServiceRef, ServiceSelection, SystemPredicate,
};
use hydrus_core::{Mime, PerceptualHash, ServiceKey, ServiceType, Sha256};

use super::{Condition, Context, Field, Kind, Panel, RatingService, TreeGroup};

/// The filetypes the reference's tree offers, by group, in its order
/// (`general_mimetypes_to_mime_groups`, the searchable ones).
pub(crate) const FILETYPE_TREE: [(Mime, &[Mime]); 7] = [
    (
        Mime::GeneralImage,
        &[
            Mime::ImageJpeg,
            Mime::ImagePng,
            Mime::ImageGif,
            Mime::ImageWebp,
            Mime::ImageJxl,
            Mime::ImageAvif,
            Mime::ImageBmp,
            Mime::ImageHeic,
            Mime::ImageHeif,
            Mime::ImageIcon,
            Mime::ImageQoi,
            Mime::ImageTiff,
        ],
    ),
    (
        Mime::GeneralAnimation,
        &[
            Mime::AnimationGif,
            Mime::AnimationApng,
            Mime::AnimationWebp,
            Mime::AnimationJxl,
            Mime::ImageAvifSequence,
            Mime::ImageHeicSequence,
            Mime::ImageHeifSequence,
            Mime::AnimationUgoira,
        ],
    ),
    (
        Mime::GeneralVideo,
        &[
            Mime::VideoMp4,
            Mime::VideoWebm,
            Mime::VideoMkv,
            Mime::VideoAvi,
            Mime::VideoFlv,
            Mime::VideoMov,
            Mime::VideoMpeg,
            Mime::VideoOgv,
            Mime::VideoRealmedia,
            Mime::VideoWmv,
        ],
    ),
    (
        Mime::GeneralAudio,
        &[
            Mime::AudioMp3,
            Mime::AudioOgg,
            Mime::AudioFlac,
            Mime::AudioM4a,
            Mime::AudioMkv,
            Mime::AudioMp4,
            Mime::AudioRealmedia,
            Mime::AudioTrueaudio,
            Mime::AudioWave,
            Mime::AudioWavpack,
            Mime::AudioWma,
        ],
    ),
    (
        Mime::GeneralApplication,
        &[
            Mime::ApplicationFlash,
            Mime::ApplicationPdf,
            Mime::ApplicationEpub,
            Mime::ApplicationDjvu,
            Mime::ApplicationDocx,
            Mime::ApplicationXlsx,
            Mime::ApplicationPptx,
            Mime::ApplicationDoc,
            Mime::ApplicationXls,
            Mime::ApplicationPpt,
            Mime::ApplicationRtf,
        ],
    ),
    (
        Mime::GeneralImageProject,
        &[
            Mime::ApplicationClip,
            Mime::ApplicationKrita,
            Mime::ImageOpenraster,
            Mime::ApplicationPaintDotNet,
            Mime::ApplicationProcreate,
            Mime::ApplicationPsd,
            Mime::ApplicationSai2,
            Mime::ImageSvg,
            Mime::ApplicationXcf,
        ],
    ),
    (
        Mime::GeneralApplicationArchive,
        &[
            Mime::ApplicationCbz,
            Mime::Application7z,
            Mime::ApplicationGzip,
            Mime::ApplicationRar,
            Mime::ApplicationZip,
        ],
    ),
];

/// The hash types `system:hash` takes, with their lengths in hex.
const HASH_TYPES: [(&str, usize); 4] = [("sha256", 64), ("md5", 32), ("sha1", 40), ("sha512", 128)];

/// The local rating service types, in the reference's order
/// (`LOCAL_RATINGS_SERVICES`).
const LOCAL_RATINGS: [ServiceType; 3] = [
    ServiceType::LocalRatingLike,
    ServiceType::LocalRatingNumerical,
    ServiceType::LocalRatingIncDec,
];

/// A star rating's "none".
const NOT_SET: &str = "(not set)";

/// What pressing a panel's button did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pressed {
    /// Done (the panel changed, or nothing needed to).
    Done,
    /// Something to tell the user, the panel left as it was.
    Warning(String),
    /// Paste an image's hashes into the panel (which the window does: it
    /// needs the clipboard).
    Paste,
}

pub(super) fn filetype_panel() -> Panel {
    let mut panel = Panel::new(Kind::Filetype);
    panel.push(Field::label("system:filetype"));
    panel.push(Field::choice(&["is", "is not"], "is"));
    panel.push(Field::Tree {
        groups: FILETYPE_TREE
            .iter()
            .map(|(group, members)| TreeGroup {
                name: group.human_name().to_owned(),
                options: members.iter().map(|m| m.human_name().to_owned()).collect(),
                ticked: vec![false; members.len()],
                expanded: false,
            })
            .collect(),
    });
    panel
}

pub(super) fn hash_panel() -> Panel {
    let mut panel = Panel::new(Kind::Hash);
    panel.push(Field::label("system:hash"));
    panel.push(Field::choice(&["is", "is not"], "is"));
    panel.push(Field::lines(
        "",
        "enter hash (paste newline-separated for multiple hashes)",
    ));
    panel.push(Field::Button("clean up text and guess hash type".into()));
    panel.push(Field::Button(
        "clean up text and guess hash type (remove bad lines)".into(),
    ));
    let types: Vec<&str> = HASH_TYPES.iter().map(|(t, _)| *t).collect();
    panel.push(Field::choice(&types, "sha256"));
    // (the buttons under the hashes)
    panel.second_line = vec![3, 4];
    panel
}

/// A specifier of rating services (`ServiceSpecifierButton`), shown in
/// place: by type or by service, and the types or services ticked; shown
/// only while `shown_with` holds, if given.
fn specifier_fields(panel: &mut Panel, context: &Context, shown_with: Option<(usize, usize)>) {
    let mode = panel.push(Field::choice(&["service type", "service"], "service type"));
    let types = panel.push(Field::Ticks {
        options: LOCAL_RATINGS
            .iter()
            .map(|t| t.short_name().to_owned())
            .collect(),
        ticked: vec![true; LOCAL_RATINGS.len()],
    });
    let local = local_ratings(context);
    let services = panel.push(Field::Ticks {
        options: local
            .iter()
            .map(|s| format!("{}: {}", s.service_type.short_name(), s.name))
            .collect(),
        ticked: vec![false; local.len()],
    });
    for (field, option) in [(types, 0), (services, 1)] {
        panel.conditions.push(Condition {
            field,
            choice: mode,
            options: vec![option],
            hide: true,
        });
    }
    if let Some((choice, option)) = shown_with {
        for field in [mode, types, services] {
            panel.conditions.push(Condition {
                field,
                choice,
                options: vec![option],
                hide: true,
            });
        }
    }
}

fn local_ratings(context: &Context) -> Vec<&RatingService> {
    context
        .rating_services
        .iter()
        .filter(|s| LOCAL_RATINGS.contains(&s.service_type))
        .collect()
}

pub(super) fn rating_advanced_panel(context: &Context) -> Panel {
    let mut panel = Panel::new(Kind::RatingAdvanced);
    let logic = panel.push(Field::choice(&["all", "any", "only"], "any"));
    specifier_fields(&mut panel, context, None);
    let amongst = panel.push(Field::label(" amongst "));
    panel.conditions.push(Condition {
        field: amongst,
        choice: logic,
        options: vec![2],
        hide: true,
    });
    specifier_fields(&mut panel, context, Some((logic, 2)));
    panel.push(Field::choice(&["rated", "not rated"], "rated"));
    panel
}

pub(super) fn rating_like_panel(index: usize, service: &RatingService) -> Panel {
    let mut panel = Panel::new(Kind::RatingLike(index));
    panel.push(Field::label(&service.name));
    panel.push(Field::choice(
        &["has rating", "no rating", "is"],
        "has rating",
    ));
    panel.push(Field::choice(&[NOT_SET, "like", "dislike"], NOT_SET));
    panel
}

/// The stars a numerical rating service's control offers: "1/5" to "5/5"
/// (from "0/5" if no stars is a rating).
fn star_options(service: &RatingService) -> Vec<String> {
    let (stars, allow_zero) = service.stars;
    let mut options = vec![NOT_SET.to_owned()];
    options.extend((u64::from(!allow_zero)..=stars).map(|i| format!("{i}/{stars}")));
    options
}

pub(super) fn rating_numerical_panel(index: usize, service: &RatingService) -> Panel {
    let mut panel = Panel::new(Kind::RatingNumerical(index));
    panel.push(Field::label(&service.name));
    panel.push(Field::choice(
        &[
            "has rating",
            "no rating",
            "more than",
            "less than",
            "is",
            "is about",
        ],
        "has rating",
    ));
    panel.push(Field::Choice {
        options: star_options(service),
        chosen: 0,
    });
    panel
}

pub(super) fn rating_incdec_panel(index: usize, service: &RatingService) -> Panel {
    let mut panel = Panel::new(Kind::RatingIncDec(index));
    panel.push(Field::label(&service.name));
    let choice = panel.push(Field::choice(
        &[
            "has count",
            "no count",
            "more than",
            "less than",
            "is",
            "is about",
        ],
        "has count",
    ));
    let count = panel.push(Field::number(0, 0, 1_000_000, ""));
    panel.conditions.push(Condition {
        field: count,
        choice,
        options: vec![2, 3, 4, 5],
        hide: false,
    });
    panel
}

pub(super) fn similar_to_data_panel() -> Panel {
    let mut panel = Panel::new(Kind::SimilarToData);
    panel.push(Field::label(
        "Use this if you want to look up a file without needing to import it. Just copy its \
         file path or image data to your clipboard and paste, and hydrus will figure out the \
         search hash data.\n\nYou only need one hash, but allowing both is fine and will add \
         files that match either. Pixel hash is very fast and always returns exact pixel \
         matches. Perceptual hash is the same \"looks similar to\" system used in \"potential \
         duplicates\" discovery and the \"files\" mode of this predicate and uses the 0/2/4/8 \
         \"search distance\".",
    ));
    panel.push(Field::Button("clear".into()));
    panel.push(Field::Button("Paste image!".into()));
    panel.push(Field::label("system:similar to"));
    panel.push(Field::lines(
        "",
        "pixel hash (64 chars each, paste newline-separated for multiple)",
    ));
    panel.push(Field::lines(
        "",
        "perceptual hash (16 chars each, paste newline-separated for multiple)",
    ));
    panel.push(Field::label("\u{2248}"));
    panel.push(Field::number(8, 0, 256, ""));
    // (the note over the rest)
    panel.second_line = (1..panel.fields.len()).collect();
    panel
}

pub(super) fn similar_to_files_panel() -> Panel {
    let mut panel = Panel::new(Kind::SimilarToFiles);
    panel.push(Field::label(
        "This searches for files that look like each other within your database, just like in \
         the duplicates system. It uses the SHA256 hash and will find all similar-looking files \
         of any of the hashes you paste, regardless of whether they are currently set as \
         potential duplicates, duplicates, alternates, or false positive.",
    ));
    panel.push(Field::label("system:similar to"));
    panel.push(Field::lines(
        "",
        "file sha-256 hashes (64 chars each, paste newline-separated for multiple)",
    ));
    panel.push(Field::label("\u{2248}"));
    panel.push(Field::number(4, 0, 256, ""));
    panel.second_line = (1..panel.fields.len()).collect();
    panel
}

/// The lines of some text, each stripped, the empty ones dropped
/// (`DeserialiseNewlinedTexts`).
fn text_lines(text: &str) -> Vec<String> {
    text.lines()
        .flat_map(|l| l.split('\r'))
        .map(|l| l.trim_start_matches('\u{feff}').trim().to_owned())
        .filter(|l| !l.is_empty())
        .collect()
}

fn dedupe(items: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    items
        .into_iter()
        .filter(|i| seen.insert(i.clone()))
        .collect()
}

/// Some lines as the reference sums them up in a line
/// (`ConvertManyStringsToNiceInsertableHumanSummarySingleLine`, unsorted).
fn lines_summary(lines: &[String], noun: &str) -> String {
    const LONGEST: usize = 48;
    let quoted = |t: &str| format!("\"{t}\"");
    match lines {
        [] => format!("0(?) {noun}"),
        [one] if one.chars().count() + 2 > LONGEST => format!("1 {noun}"),
        [one] => quoted(one),
        many => {
            let full: Vec<String> = many.iter().map(|t| quoted(t)).collect();
            let full = full.join(", ");
            if full.chars().count() <= LONGEST {
                return full;
            }
            let leading = format!(
                "{} & {} other {noun}",
                quoted(&many[0]),
                hydrus_core::numbers::human_int((many.len() - 1) as u64)
            );
            if leading.chars().count() <= LONGEST {
                leading
            } else {
                format!(
                    "{} {noun}",
                    hydrus_core::numbers::human_int(many.len() as u64)
                )
            }
        }
    }
}

/// Hashes, the one type they all fit if there is one, and the lines that
/// are no hash, by why.
type ParsedHashes = (
    Vec<String>,
    Option<&'static str>,
    Vec<(&'static str, Vec<String>)>,
);

/// Hashes of any of the `allowed` types, one to a line, as the reference
/// reads them (`ParseHashesFromRawHexTextUnknownHashType`): each lowercase
/// hex, perhaps after its type and a colon, or "0x". The hashes (deduped),
/// the one type they all fit if there is one, and the lines that are no
/// hash, by why.
fn parse_hashes(text: &str, allowed: &[&str]) -> ParsedHashes {
    let types: Vec<(&'static str, usize)> = HASH_TYPES
        .iter()
        .copied()
        .filter(|(t, _)| allowed.contains(t))
        .collect();
    let mut bad: Vec<(&'static str, Vec<String>)> = Vec::new();
    let mut add_bad = |why: &'static str, line: &str| match bad.iter_mut().find(|(w, _)| *w == why)
    {
        Some((_, lines)) => lines.push(line.to_owned()),
        None => bad.push((why, vec![line.to_owned()])),
    };
    let mut seen: BTreeSet<&'static str> = BTreeSet::new();
    let mut hashes = Vec::new();
    for line in text_lines(text) {
        let mut hex = line.to_lowercase();
        if let Some((prefix, rest)) = hex.clone().split_once(':') {
            let Some((t, _)) = types.iter().find(|(t, _)| *t == prefix) else {
                add_bad("Unknown hash type (unusual prefix)", &line);
                continue;
            };
            seen.insert(t);
            rest.clone_into(&mut hex);
        }
        if hex.starts_with("0x") {
            hex.drain(..2);
        }
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            add_bad("Includes non-hex characters", &line);
            continue;
        }
        if hex.len() % 2 != 0 {
            add_bad("Has an odd number of characters", &line);
            continue;
        }
        let fitting: Vec<&'static str> = types
            .iter()
            .filter(|(_, len)| *len == hex.len())
            .map(|(t, _)| *t)
            .collect();
        if fitting.is_empty() {
            add_bad(
                "Has a length that does not match to an expected hash",
                &line,
            );
            continue;
        }
        seen.extend(fitting);
        hashes.push(hex);
    }
    let suspected = if seen.len() == 1 {
        seen.into_iter().next()
    } else {
        None
    };
    (dedupe(hashes), suspected, bad)
}

/// The lines that are no hash, as the reference says so; `gap` between
/// each reason and its lines.
fn bad_lines_message(bad: &[(&str, Vec<String>)], gap: &str) -> String {
    let blocks: Vec<String> = bad
        .iter()
        .map(|(why, lines)| format!("{why}{gap}{}", lines_summary(lines, "lines")))
        .collect();
    format!(
        "Unfortunately, some hashes did not parse correctly.\n\n{}",
        blocks.join("\n\n")
    )
}

/// Hashes of one type, one to a line, as the reference reads them
/// (`ParseHashesFromRawHexText`): whatever follows a line's last colon,
/// its hex characters; each must be `length` long.
fn parse_hashes_of(text: &str, name: &str, length: usize) -> Result<Vec<String>, String> {
    let hexes: Vec<String> = text_lines(text)
        .iter()
        .map(|line| {
            line.rsplit(':')
                .next()
                .unwrap_or_default()
                .to_lowercase()
                .chars()
                .filter(char::is_ascii_hexdigit)
                .collect()
        })
        .collect();
    let wrong: Vec<String> = hexes
        .iter()
        .filter(|h| h.len() != length)
        .map(|h| format!("{h} ({} characters)", h.len()))
        .collect();
    if !wrong.is_empty() {
        return Err(format!(
            "Sorry, {name} hashes should have {length} hex characters! These did not:\n\n{}",
            wrong.join("\n")
        ));
    }
    Ok(dedupe(hexes))
}

fn parsed<T: std::str::FromStr + Ord>(hexes: &[String]) -> Result<BTreeSet<T>, String> {
    hexes
        .iter()
        .map(|h| h.parse::<T>().map_err(|_| format!("{h} is not a hash")))
        .collect()
}

impl Panel {
    fn lines_of(&self, i: usize) -> &str {
        match &self.fields[i] {
            Field::Lines { text, .. } => text,
            _ => "",
        }
    }

    fn set_lines(&mut self, i: usize, to: String) {
        if let Some(Field::Lines { text, .. }) = self.fields.get_mut(i) {
            *text = to;
        }
    }

    /// Tick or untick filetype `option` of group `group` of tree `i`, or
    /// the whole group (`None`).
    pub fn tick_tree(&mut self, i: usize, group: usize, option: Option<usize>, on: bool) {
        if let Some(Field::Tree { groups }) = self.fields.get_mut(i)
            && let Some(g) = groups.get_mut(group)
        {
            match option {
                Some(o) => {
                    if let Some(t) = g.ticked.get_mut(o) {
                        *t = on;
                    }
                }
                None => g.ticked.iter_mut().for_each(|t| *t = on),
            }
        }
        self.settle(i);
    }

    /// Show or hide group `group`'s filetypes.
    pub fn expand(&mut self, i: usize, group: usize, expanded: bool) {
        if let Some(Field::Tree { groups }) = self.fields.get_mut(i)
            && let Some(g) = groups.get_mut(group)
        {
            g.expanded = expanded;
        }
    }

    /// Press button `i`.
    pub fn press(&mut self, i: usize) -> Pressed {
        let Some(Field::Button(label)) = self.fields.get(i) else {
            return Pressed::Done;
        };
        match (self.kind, label.as_str()) {
            (Kind::Hash, "clean up text and guess hash type") => {
                let all: Vec<&str> = HASH_TYPES.iter().map(|(t, _)| *t).collect();
                let (hashes, suspected, bad) = parse_hashes(self.lines_of(2), &all);
                if !bad.is_empty() {
                    return Pressed::Warning(bad_lines_message(&bad, "\n\n"));
                }
                let Some(suspected) = suspected else {
                    return Pressed::Warning(
                        "Unfortunately, I cannot figure out which hash type those hashes are. \
                         Are they the wrong length, or a mix of lengths?"
                            .into(),
                    );
                };
                self.set_lines(2, hashes.join("\n"));
                self.choose_hash_type(suspected);
                Pressed::Done
            }
            (Kind::Hash, _) => {
                let all: Vec<&str> = HASH_TYPES.iter().map(|(t, _)| *t).collect();
                let (hashes, suspected, _) = parse_hashes(self.lines_of(2), &all);
                self.set_lines(2, hashes.join("\n"));
                if let Some(suspected) = suspected {
                    self.choose_hash_type(suspected);
                }
                Pressed::Done
            }
            (Kind::SimilarToData, "clear") => {
                self.set_lines(4, String::new());
                self.set_lines(5, String::new());
                Pressed::Done
            }
            (Kind::SimilarToData, _) => Pressed::Paste,
            _ => Pressed::Done,
        }
    }

    fn choose_hash_type(&mut self, hash_type: &str) {
        if let Some(t) = HASH_TYPES.iter().position(|(t, _)| *t == hash_type) {
            self.choose(5, t);
        }
    }

    /// Add a pasted image's hashes to a similar-to-data panel (each kept
    /// once).
    pub fn paste_hashes(&mut self, pixel: &Sha256, perceptual: &[PerceptualHash]) {
        for (i, new) in [
            (4, vec![pixel.to_hex()]),
            (5, perceptual.iter().map(PerceptualHash::to_hex).collect()),
        ] {
            let mut lines: Vec<String> = self.lines_of(i).lines().map(str::to_owned).collect();
            lines.extend(new);
            self.set_lines(i, dedupe(lines).join("\n"));
        }
    }

    /// What follows a change to field `changed` (`_UpdateControls`,
    /// `_RatingChanged`, `_MakeSureSecondaryIsLargeEnough`).
    pub(super) fn settle_special(&mut self, changed: usize) {
        match self.kind {
            Kind::RatingLike(_) | Kind::RatingNumerical(_) => {
                let rated_or_not = self.chosen(1) < 2;
                if changed == 2 && self.chosen(2) != 0 && rated_or_not {
                    let is = match self.kind {
                        Kind::RatingLike(_) => 2,
                        _ => 4,
                    };
                    self.choose_quietly(1, is);
                } else if changed == 1 && rated_or_not {
                    self.choose_quietly(2, 0);
                }
            }
            Kind::RatingAdvanced if [1, 2, 3, 5, 6, 7].contains(&changed) => {
                // the services searched amongst take in those searched
                let primary = self.specific_services(1);
                let secondary = self.specific_services(5);
                if !primary.is_subset(&secondary) {
                    self.choose_quietly(5, 1);
                    let all = secondary.union(&primary).copied().collect::<BTreeSet<_>>();
                    if let Some(Field::Ticks { ticked, .. }) = self.fields.get_mut(7) {
                        for (i, t) in ticked.iter_mut().enumerate() {
                            *t = all.contains(&i);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// The local rating services (by place) a specifier at `mode` names:
    /// those of its types, or those ticked.
    fn specific_services(&self, mode: usize) -> BTreeSet<usize> {
        let ticked = self.ticked(mode + 1);
        let by_service = self.ticked(mode + 2);
        if self.chosen(mode) == 1 {
            return by_service
                .iter()
                .enumerate()
                .filter_map(|(i, on)| on.then_some(i))
                .collect();
        }
        // (each service's type, by its option's "type: name")
        let Some(Field::Ticks { options, .. }) = self.fields.get(mode + 2) else {
            return BTreeSet::new();
        };
        options
            .iter()
            .enumerate()
            .filter(|(_, option)| {
                LOCAL_RATINGS
                    .iter()
                    .zip(&ticked)
                    .any(|(t, on)| *on && option.starts_with(&format!("{}: ", t.short_name())))
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// A specifier's services, as a predicate names them.
    fn selection(&self, mode: usize, context: &Context) -> ServiceSelection {
        if self.chosen(mode) == 0 {
            ServiceSelection::Types(
                LOCAL_RATINGS
                    .iter()
                    .zip(self.ticked(mode + 1))
                    .filter_map(|(t, on)| on.then_some(*t))
                    .collect(),
            )
        } else {
            let local = local_ratings(context);
            ServiceSelection::Keys(
                local
                    .iter()
                    .zip(self.ticked(mode + 2))
                    .filter(|(_, on)| *on)
                    .map(|(s, _)| s.key.clone())
                    .collect(),
            )
        }
    }

    /// The predicates the special panels make, or why they can't.
    pub(super) fn special_predicates(&self, context: &Context) -> Result<Vec<Predicate>, String> {
        let system = |p: SystemPredicate| Ok(vec![Predicate::System(p)]);
        let rating = |key: &ServiceKey, test: RatingTest| {
            system(SystemPredicate::Rating {
                service: ServiceRef::Key(key.clone()),
                test,
            })
        };
        let op = |choice: usize| match choice {
            2 => RatingOp::Greater,
            3 => RatingOp::Less,
            5 => RatingOp::Approx,
            _ => RatingOp::Equal,
        };
        match self.kind {
            Kind::Filetype => {
                let Some(Field::Tree { groups }) = self.fields.get(2) else {
                    return Ok(Vec::new());
                };
                let mimes = FILETYPE_TREE
                    .iter()
                    .zip(groups)
                    .flat_map(|((_, members), g)| {
                        members
                            .iter()
                            .zip(&g.ticked)
                            .filter_map(|(m, on)| on.then_some(*m))
                    });
                system(SystemPredicate::Filetype {
                    filetypes: FiletypeSet::new(mimes),
                    inclusive: self.chosen(1) == 0,
                })
            }
            Kind::Hash => {
                let hash_type = HASH_TYPES[self.chosen(5)].0;
                let text = self.lines_of(2);
                let (hashes, suspected, bad) = parse_hashes(text, &[hash_type]);
                if !hashes.is_empty() || !bad.is_empty() {
                    if suspected.is_none() {
                        let all: Vec<&str> = HASH_TYPES.iter().map(|(t, _)| *t).collect();
                        if let (_, Some(lenient), _) = parse_hashes(text, &all) {
                            return Err(format!(
                                "Hey, I think you have the wrong hash type set. You are set to \
                                 {hash_type} but I think you are {lenient}. Try clicking the \
                                 button to auto-detect."
                            ));
                        }
                    }
                    if !bad.is_empty() {
                        return Err(bad_lines_message(&bad, "\n"));
                    }
                    if suspected.is_none() {
                        return Err("Unfortunately, I cannot figure out which hash type those \
                                    hashes are. Are they the wrong length, or a mix of lengths?"
                            .into());
                    }
                }
                let hashes = match hash_type {
                    "md5" => FileHashes::Md5(parsed(&hashes)?),
                    "sha1" => FileHashes::Sha1(parsed(&hashes)?),
                    "sha512" => FileHashes::Sha512(parsed(&hashes)?),
                    _ => FileHashes::Sha256(parsed(&hashes)?),
                };
                system(SystemPredicate::Hash {
                    hashes,
                    inclusive: self.chosen(1) == 0,
                })
            }
            Kind::RatingAdvanced => {
                let primary = self.selection(1, context);
                let secondary = self.selection(5, context);
                let same = self.chosen(1) == self.chosen(5) && primary == secondary;
                let logic = match self.chosen(0) {
                    2 if !same => RatingLogic::Only { amongst: secondary },
                    // (only amongst what it is amongst is all)
                    0 | 2 => RatingLogic::All,
                    _ => RatingLogic::Any,
                };
                system(SystemPredicate::RatingAdvanced {
                    logic,
                    services: primary,
                    rated: self.chosen(8) == 0,
                })
            }
            Kind::RatingLike(index) => {
                let Some(service) = context.rating_services.get(index) else {
                    return Err("That rating service is gone.".into());
                };
                let test = match (self.chosen(1), self.chosen(2)) {
                    (0, _) => RatingTest::Rated,
                    (2, 1) => RatingTest::Liked,
                    (2, 2) => RatingTest::Disliked,
                    _ => RatingTest::NotRated,
                };
                rating(&service.key, test)
            }
            Kind::RatingNumerical(index) => {
                let Some(service) = context.rating_services.get(index) else {
                    return Err("That rating service is gone.".into());
                };
                let (out_of, allow_zero) = service.stars;
                let test = match (self.chosen(1), self.chosen(2)) {
                    (0, _) => RatingTest::Rated,
                    // (no stars: only "is" makes anything, no rating)
                    (1, _) | (4, 0) => RatingTest::NotRated,
                    (_, 0) => return Ok(Vec::new()),
                    (choice, star) => RatingTest::Stars {
                        op: op(choice),
                        stars: star as u64 - u64::from(allow_zero),
                        out_of,
                    },
                };
                rating(&service.key, test)
            }
            Kind::RatingIncDec(index) => {
                let Some(service) = context.rating_services.get(index) else {
                    return Err("That rating service is gone.".into());
                };
                let test = match self.chosen(1) {
                    0 => RatingTest::Count {
                        op: RatingOp::Greater,
                        value: 0,
                    },
                    1 => RatingTest::Count {
                        op: RatingOp::Equal,
                        value: 0,
                    },
                    choice => RatingTest::Count {
                        op: op(choice),
                        value: self.unsigned(2),
                    },
                };
                rating(&service.key, test)
            }
            Kind::SimilarToFiles => {
                let files = parsed::<Sha256>(&parse_hashes_of(self.lines_of(2), "sha256", 64)?)?;
                system(SystemPredicate::SimilarToFiles {
                    files,
                    max_distance: self.unsigned(4),
                })
            }
            Kind::SimilarToData => {
                let pixel_hashes =
                    parsed::<Sha256>(&parse_hashes_of(self.lines_of(4), "pixel", 64)?)?;
                let perceptual_hashes = parsed::<PerceptualHash>(&parse_hashes_of(
                    self.lines_of(5),
                    "perceptual",
                    16,
                )?)?;
                system(SystemPredicate::SimilarToData {
                    pixel_hashes,
                    perceptual_hashes,
                    max_distance: self.unsigned(7),
                })
            }
            _ => Ok(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_are_read_as_the_reference_reads_them() {
        let h = "03d67e1677d7723a590c345fb438c585cc818ffdad77cd8f2824f8c9e85e276b";
        let (hashes, suspected, bad) = parse_hashes(
            &format!(" 0x{} \n\nsha256:{h}", h.to_uppercase()),
            &["sha256"],
        );
        assert_eq!(hashes, [h]);
        assert_eq!(suspected, Some("sha256"));
        assert!(bad.is_empty());
        let (_, suspected, bad) = parse_hashes("abc\nzz\nmd5:00", &["sha256"]);
        assert_eq!(suspected, None);
        let reasons: Vec<&str> = bad.iter().map(|(w, _)| *w).collect();
        assert_eq!(
            reasons,
            [
                "Has an odd number of characters",
                "Includes non-hex characters",
                "Unknown hash type (unusual prefix)"
            ]
        );
    }

    #[test]
    fn lines_are_summed_up_as_the_reference_sums_them() {
        let lines = |n: usize| (0..n).map(|i| format!("line {i}")).collect::<Vec<_>>();
        assert_eq!(lines_summary(&lines(1), "lines"), "\"line 0\"");
        assert_eq!(lines_summary(&lines(2), "lines"), "\"line 0\", \"line 1\"");
        // (all of them while they fit in 48 characters)
        assert_eq!(
            lines_summary(&lines(5), "lines"),
            "\"line 0\", \"line 1\", \"line 2\", \"line 3\", \"line 4\""
        );
        assert_eq!(
            lines_summary(&lines(6), "lines"),
            "\"line 0\" & 5 other lines"
        );
        assert_eq!(
            lines_summary(&lines(9), "lines"),
            "\"line 0\" & 8 other lines"
        );
        assert_eq!(lines_summary(&["x".repeat(60)], "lines"), "1 lines");
        // (48 characters, quotes and all, fit, and 49 don't)
        assert_eq!(
            lines_summary(&["x".repeat(46)], "lines"),
            format!("\"{}\"", "x".repeat(46))
        );
        assert_eq!(lines_summary(&["x".repeat(47)], "lines"), "1 lines");
        let two = |second: usize| ["x".repeat(21), "y".repeat(second)];
        assert_eq!(
            lines_summary(&two(21), "lines"),
            format!("\"{}\", \"{}\"", "x".repeat(21), "y".repeat(21))
        );
        assert_eq!(
            lines_summary(&two(22), "lines"),
            format!("\"{}\" & 1 other lines", "x".repeat(21))
        );
    }
}
