//! The reference Incremental Tagging child: ordered additive tags and live text memory.
use hydrus_core::{HashId, Tag};
use hydrus_store::{Store, tag_editing::ManageTagsSettings};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Debug)]
pub struct IncrementalTagging {
    store: Arc<Store>,
    files: Vec<HashId>,
    current: BTreeMap<HashId, BTreeSet<String>>,
    pub namespace: String,
    pub prefix: String,
    pub suffix: String,
    pub start: i32,
    pub step: i32,
    pub reverse: bool,
}
impl IncrementalTagging {
    pub(crate) fn new(
        store: Arc<Store>,
        files: Vec<HashId>,
        current: BTreeMap<HashId, BTreeSet<String>>,
    ) -> Self {
        let settings: ManageTagsSettings =
            store.read(hydrus_store::settings::get).unwrap_or_default();
        let namespace = settings.incremental_namespace;
        let initial = files
            .first()
            .and_then(|file| current.get(file))
            .into_iter()
            .flatten()
            .filter_map(|tag| {
                let (n, subtag) = split(tag);
                (n == namespace && !subtag.is_empty() && subtag.chars().all(|c| c.is_ascii_digit()))
                    .then(|| subtag.parse::<i32>().ok())
                    .flatten()
            })
            .min()
            .unwrap_or(1)
            .clamp(-10_000_000, 10_000_000);
        Self {
            store,
            files,
            current,
            namespace,
            prefix: settings.incremental_prefix,
            suffix: settings.incremental_suffix,
            start: initial,
            step: 1,
            reverse: false,
        }
    }
    /// Text changes persist immediately even if the child or parent is cancelled.
    pub fn set_text(&mut self, field: usize, text: &str) -> Result<(), String> {
        let target = match field {
            0 => &mut self.namespace,
            1 => &mut self.prefix,
            2 => &mut self.suffix,
            _ => return Err("unknown incremental field".into()),
        };
        text.clone_into(target);
        let text = text.to_owned();
        self.store
            .write(move |ctx| {
                let mut settings: ManageTagsSettings = hydrus_store::settings::get(ctx.conn())?;
                match field {
                    0 => settings.incremental_namespace = text,
                    1 => settings.incremental_prefix = text,
                    2 => settings.incremental_suffix = text,
                    _ => unreachable!(),
                };
                hydrus_store::settings::set(ctx.conn(), &settings)
            })
            .map_err(|error| error.to_string())
    }
    pub fn set_numbers(&mut self, start: i32, step: i32, reverse: bool) -> Result<(), String> {
        if !(-10_000_000..=10_000_000).contains(&start) || !(-10_000..=10_000).contains(&step) {
            return Err("Start must be -10000000 to 10000000 and step -10000 to 10000.".into());
        }
        self.start = start;
        self.step = step;
        self.reverse = reverse;
        Ok(())
    }
    pub fn pairs(&self) -> Vec<(HashId, String)> {
        self.files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                let offset = if self.reverse {
                    self.files.len() - 1 - index
                } else {
                    index
                };
                let number = i128::from(self.start) + (offset as i128) * i128::from(self.step);
                let subtag = format!("{}{number}{}", self.prefix, self.suffix);
                let tag = if self.namespace.is_empty() {
                    subtag
                } else {
                    format!("{}:{subtag}", self.namespace)
                };
                (*file, tag)
            })
            .collect()
    }
    pub fn cleaned_pairs(&self) -> Result<Vec<(HashId, Tag)>, String> {
        self.pairs()
            .into_iter()
            .map(|(file, tag)| {
                Tag::new(&tag)
                    .map(|tag| (file, tag))
                    .ok_or_else(|| format!("\"{tag}\" is not a valid tag"))
            })
            .collect()
    }
    pub fn summary(&self) -> String {
        let pairs = self.pairs();
        let tags: Vec<_> = pairs.iter().map(|(_, tag)| tag.as_str()).collect();
        let tag_summary = if tags.len() <= 4 {
            tags.join(", ")
        } else if self.reverse {
            format!("{} … {}", tags[0], tags[tags.len() - 3..].join(", "))
        } else {
            format!("{} … {}", tags[..3].join(", "), tags[tags.len() - 1])
        };
        let mut already = 0;
        let mut disagree = 0;
        for (file, tag) in &pairs {
            let (_, subtag) = split(tag);
            let subtags: BTreeSet<_> = self
                .current
                .get(file)
                .into_iter()
                .flatten()
                .filter_map(|tag| {
                    let (n, s) = split(tag);
                    (n == self.namespace).then_some(s)
                })
                .collect();
            if subtags.contains(subtag) {
                already += 1;
            } else if !subtags.is_empty() {
                disagree += 1;
            }
        }
        let count = |n| hydrus_core::numbers::human_int(n as u64);
        let conflict = match (already, disagree) {
            (0, 0) => "No conflicts, this all looks fresh!".to_owned(),
            (_, 0) if already == self.files.len() => {
                "All the files already have these tags. This will make no changes.".to_owned()
            }
            (_, 0) => format!("{} files already have these tags.", count(already)),
            (0, _) => format!(
                "{} files already have different tags for this namespace. Are you sure you are lined up correct?",
                count(disagree)
            ),
            _ => format!(
                "{} files already have these tags, and {} files already have different tags for this namespace. Are you sure you are lined up correct?",
                count(already),
                count(disagree)
            ),
        };
        format!(
            "For the {} files, you are setting {tag_summary}.\n\n{conflict}",
            count(self.files.len())
        )
    }
}
fn split(tag: &str) -> (&str, &str) {
    tag.split_once(':').unwrap_or(("", tag))
}
