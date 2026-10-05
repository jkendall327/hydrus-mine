//! A real TagSortControl retains separate text/count orders while its type changes.
use hydrus_core::tag_sort::{TagGroupBy, TagSortType};
use hydrus_store::manage_tags_sort::Sort;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    pub value: Sort,
    text_ascending: bool,
    count_ascending: bool,
}
impl Control {
    pub fn new(value: Sort) -> Self {
        Self {
            text_ascending: value.order.sort_type == TagSortType::Count || value.order.ascending,
            count_ascending: value.order.sort_type == TagSortType::Count && value.order.ascending,
            value,
        }
    }
    /// Type, current order, grouping, and siblings/tags (zero selects siblings).
    pub fn choose(&mut self, part: usize, index: usize) {
        let sort = &mut self.value.order;
        match part {
            0 => {
                let Some(kind) =
                    [TagSortType::Tag, TagSortType::Subtag, TagSortType::Count].get(index)
                else {
                    return;
                };
                sort.sort_type = *kind;
                sort.ascending = if *kind == TagSortType::Count {
                    self.count_ascending
                } else {
                    self.text_ascending
                };
            }
            1 => {
                if index > 1 {
                    return;
                }
                if sort.sort_type == TagSortType::Count {
                    self.count_ascending = index == 1;
                    sort.ascending = self.count_ascending;
                } else {
                    self.text_ascending = index == 0;
                    sort.ascending = self.text_ascending;
                }
            }
            2 => {
                if let Some(group) = [
                    TagGroupBy::Nothing,
                    TagGroupBy::NamespaceAz,
                    TagGroupBy::NamespaceUser,
                ]
                .get(index)
                {
                    sort.group_by = *group;
                }
            }
            3 if index < 2 => self.value.use_siblings = index == 0,
            _ => {}
        }
    }
}
