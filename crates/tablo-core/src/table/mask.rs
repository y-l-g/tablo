//! A table masked for one request: [`Table::without`].

use super::Table;
use crate::lens::shows;

impl<M> Table<M> {
    /// This table without what reads one of the `hidden` fields: its columns, filters and
    /// grouping, and a default sort on a column it drops. A search, a sort or a filter naming
    /// one then matches nothing the table declares, as one naming an unknown column does.
    pub(crate) fn without(&self, hidden: &[String]) -> Self {
        let mut table = self.clone();
        table.columns.retain(|column| shows(column.reads(), hidden));
        table.filters.retain(|filter| shows(filter.reads(), hidden));
        if table
            .group_by
            .as_ref()
            .is_some_and(|group| !shows(Some(group.binding.reads::<M>()), hidden))
        {
            table.group_by = None;
        }
        if table
            .default_sort
            .as_ref()
            .is_some_and(|sort| table.sortable_column(&sort.column).is_none())
        {
            table.default_sort = None;
        }
        table
    }
}
