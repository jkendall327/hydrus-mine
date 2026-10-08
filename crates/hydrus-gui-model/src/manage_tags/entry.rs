//! Entering, removing and copying tags in Manage Tags, as the reference's
//! `_Panel` does it (`AddTags`, `EnterTags`, `RemoveTags`, `_RemoveTagsButton`,
//! `_Copy`; `oracle/fixtures/manage_tags_cog.json` records them): typed and
//! suggested entry only adds unless the cog's "allow remove" is on; entering a
//! tag some but not all of the files have asks what to do; removal confirms
//! unless the cog's confirmation is off.

use std::collections::BTreeSet;

use hydrus_core::{HashId, Tag};

use super::ManageTags;
use crate::write_tag_menu::{Action, CogSetting, Entry};

/// What entering tags does to the files: add them where missing, or delete
/// them where present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Verb {
    Add,
    Delete,
}

/// "What would you like to do?": one button per kind of change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub message: String,
    /// The buttons' texts, in the reference's order (add, then delete).
    pub choices: Vec<String>,
    /// The buttons' tooltips: what it does, then the files each tag affects.
    pub tooltips: Vec<String>,
    options: Vec<(Verb, BTreeSet<String>)>,
}

/// An entry that is done, or waits on the user's choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entered {
    Done,
    Ask(Prompt),
}

/// A removal that is done, or waits on a yes/no.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removal {
    Done,
    Confirm { message: String, tags: Vec<String> },
}

/// `ElideText`: more than `max` characters become `max - 1` and an ellipsis.
fn elide(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        let mut out: String = text.chars().take(max - 1).collect();
        out.push('…');
        out
    } else {
        text.to_owned()
    }
}

impl ManageTags {
    fn live_preferences(&self) -> hydrus_store::tag_editing::TagEditingSettings {
        self.store.read(hydrus_store::settings::get).unwrap_or_default()
    }

    /// Typed or suggested entry: only adds, unless the cog allows removal.
    pub fn add_tags(&mut self, tags: &[String], only_add: bool) -> Result<Entered, String> {
        let only_add = only_add || !self.live_preferences().allow_remove_on_input;
        self.enter_tags(tags, only_add, false)
    }

    /// A listed tag activated, or the tags a button or key removes.
    pub fn enter_tags(
        &mut self,
        tags: &[String],
        only_add: bool,
        only_remove: bool,
    ) -> Result<Entered, String> {
        let mut cleaned = BTreeSet::new();
        for typed in tags {
            let tag = Tag::new(typed).ok_or_else(|| format!("\"{typed}\" is not a valid tag"))?;
            cleaned.insert(tag.as_str().to_owned());
        }
        let counts = self.tags();
        let files = self.files.len();
        let mut choices: std::collections::BTreeMap<Verb, Vec<(String, usize)>> =
            std::collections::BTreeMap::new();
        for tag in &cleaned {
            let have = counts.get(tag).copied().unwrap_or(0);
            if !only_remove && have < files {
                choices.entry(Verb::Add).or_default().push((tag.clone(), files - have));
            }
            if !only_add && have > 0 {
                choices.entry(Verb::Delete).or_default().push((tag.clone(), have));
            }
        }
        match choices.len() {
            0 => Ok(Entered::Done),
            1 => {
                let (verb, counts) = choices.into_iter().next().expect("one choice");
                self.carry_out(verb, counts.into_iter().map(|(tag, _)| tag));
                Ok(Entered::Done)
            }
            _ => {
                let mut options = Vec::new();
                let (mut texts, mut tooltips) = (Vec::new(), Vec::new());
                for (verb, tag_counts) in choices {
                    let (word, tip) = match verb {
                        Verb::Add => ("add", "this adds the tags to this local tag domain"),
                        Verb::Delete => {
                            ("delete", "this deletes the tags from this local tag domain")
                        }
                    };
                    let tags: BTreeSet<String> =
                        tag_counts.iter().map(|(tag, _)| tag.clone()).collect();
                    texts.push(if let [(tag, count)] = tag_counts.as_slice() {
                        format!(
                            "{word} \"{}\" for {} files",
                            elide(tag, 64),
                            hydrus_core::numbers::human_int(*count as u64)
                        )
                    } else {
                        format!(
                            "{word} {} tags",
                            hydrus_core::numbers::human_int(tags.len() as u64)
                        )
                    });
                    let mut lines = vec![tip.to_owned()];
                    lines.extend(tag_counts.iter().take(25).map(|(tag, count)| {
                        format!(
                            "{tag} - {} files",
                            hydrus_core::numbers::human_int(*count as u64)
                        )
                    }));
                    if tag_counts.len() > 25 {
                        lines.push(format!("and {} others", tag_counts.len() - 25));
                    }
                    tooltips.push(lines.join("\n"));
                    options.push((verb, tags));
                }
                let message = if cleaned.len() > 1 {
                    format!(
                        "The file{} some of those tags, but not all, so there are different things you can do.",
                        if files > 1 { "s have" } else { " has" }
                    )
                } else {
                    format!(
                        "Of the {} files being managed, some have that tag, but not all of them do, so there are different things you can do.",
                        hydrus_core::numbers::human_int(files as u64)
                    )
                };
                Ok(Entered::Ask(Prompt {
                    message,
                    choices: texts,
                    tooltips,
                    options,
                }))
            }
        }
    }

    /// The user chose button `index` of `prompt`, or closed it (`None`).
    pub fn answer_prompt(&mut self, prompt: &Prompt, index: Option<usize>) {
        if let Some((verb, tags)) = index.and_then(|i| prompt.options.get(i)) {
            self.carry_out(*verb, tags.iter().cloned());
        }
    }

    fn carry_out(&mut self, verb: Verb, tags: impl IntoIterator<Item = String>) {
        let selected = self.selected_tags();
        for tag in tags {
            let have: BTreeSet<HashId> = self
                .current_tags()
                .get(&tag)
                .cloned()
                .unwrap_or_default();
            for file in self.files.clone() {
                match verb {
                    Verb::Add if !have.contains(&file) => self.stage_mapping(&tag, file, true),
                    Verb::Delete if have.contains(&file) => self.stage_mapping(&tag, file, false),
                    _ => {}
                }
            }
        }
        self.input.set_context_tags(self.tags().into_keys());
        self.select_tags(&selected);
    }

    /// Remove tags (the remove button, the delete key): confirmed first
    /// unless the cog says not to.
    pub fn remove_tags(&mut self, tags: &[String]) -> Result<Removal, String> {
        if tags.is_empty() {
            return Ok(Removal::Done);
        }
        if self.live_preferences().confirm_remove {
            let message = if tags.len() < 10 {
                let mut message = String::from("Are you sure you want to remove these tags:\n\n");
                message.push_str(
                    &tags
                        .iter()
                        .map(|tag| elide(tag, 64))
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                message
            } else {
                format!(
                    "Are you sure you want to remove these {} tags?",
                    hydrus_core::numbers::human_int(tags.len() as u64)
                )
            };
            return Ok(Removal::Confirm {
                message,
                tags: tags.to_vec(),
            });
        }
        self.enter_tags(tags, false, true).map(|_| Removal::Done)
    }

    /// The yes of a removal's confirmation.
    pub fn confirm_removal(&mut self, tags: &[String]) -> Result<(), String> {
        self.enter_tags(tags, false, true).map(|_| ())
    }

    /// "remove all/selected tags": the selected tags that can be removed, or,
    /// with none selected, every removable tag.
    pub fn remove_button(&mut self) -> Result<Removal, String> {
        let removable = self.tags();
        let selected = self.selected_tags();
        let mut tags: Vec<String> = if selected.is_empty() {
            removable.into_keys().collect()
        } else {
            selected
                .into_iter()
                .filter(|tag| removable.contains_key(tag))
                .collect()
        };
        hydrus_core::sort::human_sort(&mut tags);
        self.remove_tags(&tags)
    }

    /// The copy button: the selected tags, or all of them, one per line, and
    /// the notice shown. `None` when there is nothing to copy.
    pub fn copy_button(&self) -> Option<(String, String)> {
        let mut tags = self.selected_tags();
        if tags.is_empty() {
            tags = self.tags().into_keys().collect();
        }
        if tags.is_empty() {
            return None;
        }
        hydrus_core::sort::human_sort(&mut tags);
        let notice = format!(
            "Copied {} tags!",
            hydrus_core::numbers::human_int(tags.len() as u64)
        );
        Some((tags.join("\n"), notice))
    }

    /// The cog menu, as the reference's has it (the moderator item is for
    /// remote repositories, which are out of scope).
    pub fn cog_entries(&self) -> Vec<Entry> {
        let o = self.live_preferences();
        vec![
            Entry::Check(
                "allow remove/petition result on tag input for already existing tag".into(),
                Action::Cog(CogSetting::AllowRemoveOnInput(!o.allow_remove_on_input)),
                o.allow_remove_on_input,
            ),
            Entry::Check(
                "confirm remove/petition tags on explicit delete actions".into(),
                Action::Cog(CogSetting::ConfirmRemove(!o.confirm_remove)),
                o.confirm_remove,
            ),
            Entry::Check(
                "select the first tag result with actual count".into(),
                Action::Cog(CogSetting::SelectFirstWithCount(!o.select_first_with_count)),
                o.select_first_with_count,
            ),
            Entry::Separator,
            Entry::Item("migrate tags for these files".into(), Action::MigrateTags),
        ]
    }

    /// The waiting changes on the service chosen: (added?, tag, files),
    /// ordered by tag and then adds before deletes.
    pub fn staged_changes(&self) -> Vec<(bool, String, Vec<HashId>)> {
        let mut out = Vec::new();
        for (tag, changes) in &self.staged[self.service] {
            for add in [true, false] {
                let files: Vec<HashId> = changes
                    .iter()
                    .filter_map(|(file, value)| (*value == add).then_some(*file))
                    .collect();
                if !files.is_empty() {
                    out.push((add, tag.clone(), files));
                }
            }
        }
        out
    }

    /// The tags selected in the list.
    pub fn selected_tags(&self) -> Vec<String> {
        let rows = self.display_rows();
        self.tag_selection
            .selected_order()
            .iter()
            .filter_map(|&i| rows.get(i))
            .filter(|row| !row.parent_row)
            .map(|row| row.tag.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Which listed rows are selected.
    pub fn tag_selection_mask(&self) -> Vec<bool> {
        let rows = self.display_rows();
        (0..rows.len())
            .map(|i| self.tag_selection.is_selected(i))
            .collect()
    }

    /// A listed tag clicked (ctrl and shift extend the selection).
    pub fn click_tag(&mut self, row: usize, ctrl: bool, shift: bool) {
        let order: Vec<usize> = (0..self.display_rows().len()).collect();
        self.tag_selection.click(&order, row, ctrl, shift);
    }

    fn select_tags(&mut self, tags: &[String]) {
        let rows: Vec<usize> = self
            .display_rows()
            .iter()
            .enumerate()
            .filter(|(_, row)| !row.parent_row && tags.contains(&row.tag))
            .map(|(i, _)| i)
            .collect();
        self.tag_selection.select_many(&rows);
    }
}
