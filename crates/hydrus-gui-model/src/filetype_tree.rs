//! The reference's tree of filetypes to tick (`OptionsPanelMimesTree`):
//! the filetypes by group (image, animation, video and so on), a group
//! ticking or unticking all of its, as file filtering import options'
//! "allowed filetypes" use it. The ticked are kept as filetype codes, a
//! group's code standing for all of its filetypes (as the client's
//! defaults have them) until one of them is unticked.

use std::collections::BTreeSet;

use hydrus_core::mime::Mime;

use crate::predicate_editors::{FILETYPE_TREE, TreeGroup};

/// The filetypes `codes` stand for: their own, and a group's members.
fn specific(codes: &BTreeSet<u8>) -> BTreeSet<u8> {
    let mut out = BTreeSet::new();
    for &code in codes {
        match FILETYPE_TREE.iter().find(|(group, _)| group.code() == code) {
            Some((_, members)) => out.extend(members.iter().map(|m| m.code())),
            None => {
                out.insert(code);
            }
        }
    }
    out
}

/// The tree's groups with `codes` ticked, those of `expanded` showing
/// their filetypes.
pub fn groups(codes: &BTreeSet<u8>, expanded: &[bool]) -> Vec<TreeGroup> {
    let ticked = specific(codes);
    FILETYPE_TREE
        .iter()
        .enumerate()
        .map(|(g, (group, members))| TreeGroup {
            name: group.human_name().to_owned(),
            options: members.iter().map(|m| m.human_name().to_owned()).collect(),
            ticked: members.iter().map(|m| ticked.contains(&m.code())).collect(),
            expanded: expanded.get(g).copied().unwrap_or(false),
        })
        .collect()
}

/// `codes` with group `group`'s filetype `option` (or, with none, all of
/// the group's) ticked or unticked; as specific filetypes (`GetValue`).
pub fn tick(codes: &BTreeSet<u8>, group: usize, option: Option<usize>, on: bool) -> BTreeSet<u8> {
    let mut codes = specific(codes);
    let Some((_, members)) = FILETYPE_TREE.get(group) else {
        return codes;
    };
    let changed: Vec<&Mime> = match option {
        Some(o) => members.get(o).into_iter().collect(),
        None => members.iter().collect(),
    };
    for m in changed {
        if on {
            codes.insert(m.code());
        } else {
            codes.remove(&m.code());
        }
    }
    codes
}
