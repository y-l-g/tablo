//! The table's search: the predicate a term matches its searchable columns by.

use toasty::stmt::Expr;

use super::Table;

impl<M> Table<M> {
    /// Global search predicate across searchable columns.
    pub(crate) fn search_expr(&self, term: &str) -> Option<Expr<bool>>
    where
        M: toasty::schema::Model,
    {
        let t = term.trim();
        if t.is_empty() {
            return None;
        }
        let mut exprs = self.columns.iter().filter_map(|c| c.search_expr(t));
        let first = exprs.next()?;
        Some(exprs.fold(first, |acc, e| acc.or(e)))
    }

    /// Whether a column joins the table's search, whether or not the toolbar shows it.
    pub(crate) fn has_searchable_column(&self) -> bool
    where
        M: toasty::schema::Model,
    {
        self.columns.iter().any(|c| c.is_searchable())
    }

    /// Whether the declaration enables a search toolbar.
    pub(crate) fn search_enabled(&self) -> bool
    where
        M: toasty::schema::Model,
    {
        !self.hide_search && self.has_searchable_column()
    }
}
