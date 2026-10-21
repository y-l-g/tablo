//! A table masked for one request: [`Table::without`].

use topcoat::context::Cx;

use super::Table;
use crate::lens::shows;

impl<M> Table<M> {
    /// Whether a column shows a field of a related record its resource hides from `cx`.
    pub(crate) fn has_masked(&self, cx: &Cx) -> bool {
        self.columns.iter().any(|column| column.masked(cx))
    }

    /// This table as `cx` reads it, without what reads one of the `hidden` fields: its columns,
    /// filters and grouping, and a default sort on a column it drops. A search, a sort or a filter
    /// naming one then matches nothing the table declares, as one naming an unknown column does.
    /// It drops a column showing a field of a related record its resource hides from `cx` too.
    pub(crate) fn without(&self, cx: &Cx, hidden: &[String]) -> Self {
        let mut table = self.clone();
        table
            .columns
            .retain(|column| crate::column::shows(cx, &**column, hidden));
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
