//! Shared current/pasted/result chooser and durable favourite naming rules.
use crate::import_options_editor::{Kind, listed_kinds, summary};
use hydrus_core::import_options::{CallerType, ImportOptionsManager, ImportOptionsSlice};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Merge,
    FillIn,
    Replace,
}

#[derive(Debug, Clone)]
pub struct Overwrite {
    pub caller: CallerType,
    pub kinds: Vec<Kind>,
    pub current: ImportOptionsSlice,
    pub pasted: ImportOptionsSlice,
    pub keep_current: Vec<bool>,
    pub take_pasted: Vec<bool>,
}

impl Overwrite {
    pub fn new(
        caller: CallerType,
        simple: bool,
        current: ImportOptionsSlice,
        mut pasted: ImportOptionsSlice,
    ) -> Self {
        let kinds = listed_kinds(caller, simple, &current);
        if simple {
            for kind in Kind::ALL {
                if !listed_kinds(caller, true, &ImportOptionsSlice::default()).contains(&kind) {
                    kind.clear(&mut pasted);
                }
            }
        }
        let mut result = Self {
            caller,
            keep_current: vec![true; kinds.len()],
            take_pasted: vec![false; kinds.len()],
            kinds,
            current,
            pasted,
        };
        result.preset(Preset::Merge);
        result
    }
    pub fn differs(&self, kind: Kind) -> bool {
        let mut left = ImportOptionsSlice::default();
        let mut right = ImportOptionsSlice::default();
        kind.copy(&self.current, &mut left);
        kind.copy(&self.pasted, &mut right);
        left != right
    }
    pub fn preset(&mut self, preset: Preset) {
        self.keep_current.fill(true);
        self.take_pasted = self
            .kinds
            .iter()
            .map(|&kind| {
                let allowed = self.caller != CallerType::Global || kind.is_set(&self.pasted);
                allowed
                    && match preset {
                        Preset::Merge => kind.is_set(&self.pasted) && self.differs(kind),
                        Preset::FillIn => kind.is_set(&self.pasted) && !kind.is_set(&self.current),
                        Preset::Replace => self.differs(kind),
                    }
            })
            .collect();
    }
    pub fn tick(&mut self, pasted: bool, row: usize, checked: bool) {
        let Some(&kind) = self.kinds.get(row) else {
            return;
        };
        if self.caller == CallerType::Global && (!pasted || !kind.is_set(&self.pasted)) {
            return;
        }
        let values = if pasted {
            &mut self.take_pasted
        } else {
            &mut self.keep_current
        };
        values[row] = checked;
    }
    pub fn value(&self) -> ImportOptionsSlice {
        let mut result = ImportOptionsSlice::default();
        for (i, &kind) in self.kinds.iter().enumerate() {
            if self.keep_current[i] && !self.take_pasted[i] {
                kind.copy(&self.current, &mut result);
            }
            if self.take_pasted[i] {
                kind.copy(&self.pasted, &mut result);
            }
        }
        result
    }
    pub fn notice(&self) -> &'static str {
        if self.kinds.iter().all(|kind| !self.differs(*kind)) {
            "It looks like what was pasted/loaded would make no changes to what is already in!"
        } else if self.kinds.iter().any(|kind| !kind.is_set(&self.pasted)) {
            if self.caller == CallerType::Global {
                "You cannot make the Global set \"default\" anywhere, so those items in the paste/load have been disabled."
            } else {
                "Some pasted/loaded import options made no changes; these have been left unchecked."
            }
        } else {
            ""
        }
    }
    pub fn label(
        &self,
        slice: &ImportOptionsSlice,
        kind: Kind,
        incoming: bool,
        name: &dyn Fn(&str) -> String,
    ) -> String {
        if incoming && !self.differs(kind) {
            return if kind.is_set(slice) {
                "no changes"
            } else {
                "no changes (both default)"
            }
            .into();
        }
        let text = if kind.is_set(slice) {
            let summary = summary(kind, slice, name);
            format!(
                "> {}{}",
                kind.name(),
                if summary.is_empty() {
                    String::new()
                } else {
                    format!(": {summary}")
                }
            )
        } else {
            format!("{}: default", kind.name())
        };
        let first = text.lines().next().unwrap_or("");
        if first.chars().count() > 256 {
            format!("{}…", first.chars().take(255).collect::<String>())
        } else {
            first.into()
        }
    }
}

/// Add/edit naming matches the reference manager; callers commit in one transaction.
pub fn save_favourite(
    manager: &mut ImportOptionsManager,
    original: Option<&str>,
    name: &str,
    value: ImportOptionsSlice,
) -> String {
    if let Some(original) = original {
        manager.favourites.retain(|(name, _)| name != original);
    }
    let names = manager
        .favourites
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut actual = name.to_owned();
    let mut index = 1;
    while names.contains(actual.as_str()) {
        actual = format!("{name} ({index})");
        index += 1;
    }
    manager.favourites.push((actual.clone(), value));
    actual
}
