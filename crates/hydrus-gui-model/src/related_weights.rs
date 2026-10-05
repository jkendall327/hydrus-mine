//! A detached related-weight draft, preserving the reference's namespace rules.
use hydrus_store::related_tags::Weights;

#[derive(Debug)]
pub struct Editor {
    pub weights: Weights,
    pub result: bool,
    pub selected: crate::list_selection::ListSelection<usize>,
}
pub const RESERVED: &str = "Sorry, you cannot re-add unnamespaced or namespaced!";
pub const DUPLICATE: &str = "Sorry, that namespace already exists!";
impl Editor {
    pub fn new(mut weights: Weights) -> Self {
        for rows in [&mut weights.search, &mut weights.result] {
            rows.sort_by_cached_key(|(slice, weight)| (Self::pretty(slice), *weight));
        }
        Self {
            weights,
            result: false,
            selected: crate::list_selection::ListSelection::default(),
        }
    }
    pub fn rows(&self) -> &[(String, u16)] {
        if self.result {
            &self.weights.result
        } else {
            &self.weights.search
        }
    }
    fn rows_mut(&mut self) -> &mut Vec<(String, u16)> {
        if self.result {
            &mut self.weights.result
        } else {
            &mut self.weights.search
        }
    }
    pub fn choose(&mut self, result: bool) {
        self.result = result;
        self.selected = crate::list_selection::ListSelection::default();
    }
    pub fn click(&mut self, index: usize, ctrl: bool, shift: bool) {
        let order: Vec<_> = (0..self.rows().len()).collect();
        self.selected.click(&order, index, ctrl, shift);
    }
    pub fn selection(&self) -> Vec<usize> {
        (0..self.rows().len())
            .filter(|index| self.selected.is_selected(*index))
            .collect()
    }
    pub fn namespace(&self, typed: &str) -> Result<String, String> {
        if matches!(typed, "" | ":") {
            return Err(RESERVED.into());
        }
        let slice = if typed.ends_with(':') {
            typed.to_owned()
        } else {
            format!("{typed}:")
        };
        if self.rows().iter().any(|(existing, _)| *existing == slice) {
            return Err(DUPLICATE.into());
        }
        Ok(slice)
    }
    pub fn add(&mut self, slice: String, weight: u16) {
        self.rows_mut().push((slice, weight));
        self.sort();
    }
    pub fn edit(&mut self, index: usize, weight: u16) {
        if let Some(row) = self.rows_mut().get_mut(index) {
            row.1 = weight;
        }
        self.sort();
    }
    pub fn can_delete(&self) -> bool {
        let selection = self.selection();
        !selection.is_empty()
            && selection
                .iter()
                .all(|&index| !matches!(self.rows()[index].0.as_str(), "" | ":"))
    }
    pub fn delete(&mut self) {
        if !self.can_delete() {
            return;
        }
        let selection = self.selection();
        let mut index = 0;
        self.rows_mut().retain(|_| {
            let keep = !selection.contains(&index);
            index += 1;
            keep
        });
        self.selected = crate::list_selection::ListSelection::default();
    }
    pub fn pretty(slice: &str) -> String {
        match slice {
            "" => "unnamespaced tags".into(),
            ":" => "namespaced tags".into(),
            _ => format!("'{}' tags", slice.strip_suffix(':').unwrap_or(slice)),
        }
    }
    pub fn sort(&mut self) {
        self.rows_mut()
            .sort_by_cached_key(|(slice, weight)| (Self::pretty(slice), *weight));
        self.selected = crate::list_selection::ListSelection::default();
    }
}
