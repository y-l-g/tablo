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
    /// filters, tabs, groupings and row color, and a default sort on a column or field it drops. A
    /// search, a sort or a filter naming one then matches nothing the table declares, as one naming
    /// an unknown column does. It drops a column showing a field of a related record its resource
    /// hides from `cx` too.
    pub(crate) fn without(&self, cx: &Cx, hidden: &[String]) -> Self {
        let mut table = self.clone();
        table
            .columns
            .retain(|column| crate::column::shows(cx, &**column, hidden));
        table.filters.retain(|filter| shows(filter.reads(), hidden));
        table.tabs.retain(|tab| shows(tab.reads(), hidden));
        table
            .groupings
            .retain(|group| shows(Some(group.reads()), hidden));
        if table
            .default_field
            .as_ref()
            .is_some_and(|field| !shows(Some(field.reads::<M>()), hidden))
        {
            table.default_field = None;
        }
        if !shows(table.row_color.reads.clone(), hidden) {
            table.row_color = Default::default();
        }
        if table
            .default_sort
            .as_ref()
            .is_some_and(|sort| !table.orders_by(sort))
        {
            table.default_sort = None;
        }
        table
    }
}
