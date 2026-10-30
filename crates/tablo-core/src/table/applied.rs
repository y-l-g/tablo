//! Which of a request's filters and tab a table applies: the predicate they build, and the
//! ones it ignores, which the list's banner names and the export and a selection refuse.

use toasty::stmt::Expr;
use topcoat::context::Cx;

use super::{Table, TableState};
use crate::dom::ignored;

impl<M> Table<M> {
    /// The names of the requested filters whose value [`Filter::admits`] refuses: values naming
    /// records the user may not see, which [`Self::filter_expr`] and
    /// [`Self::unapplied_filters`] treat as invalid.
    ///
    /// # Errors
    ///
    /// The database failing a filter's check.
    pub(crate) async fn refused_filters(
        &self,
        cx: &topcoat::context::Cx,
        state: &TableState,
    ) -> topcoat::Result<Vec<String>> {
        let mut refused = Vec::new();
        for f in &self.filters {
            if let Some(value) = state.filters.get(f.name())
                && !f.internals().admits(cx, value).await?
            {
                refused.push(f.name().to_string());
            }
        }
        Ok(refused)
    }

    /// Filter predicate for the current `TableState`, without the `refused` filters.
    pub(crate) fn filter_expr(
        &self,
        cx: &Cx,
        state: &TableState,
        refused: &[String],
    ) -> Option<Expr<bool>>
    where
        M: toasty::schema::Model,
    {
        let mut exprs = Vec::new();
        for f in &self.filters {
            if let Some(v) = state.filters.get(f.name())
                && !refused.iter().any(|name| name == f.name())
                && let Some(e) = f.to_expr(cx, v)
            {
                exprs.push(e);
            }
        }
        if exprs.is_empty() {
            None
        } else {
            let mut iter = exprs.into_iter();
            let first = iter.next().unwrap();
            Some(iter.fold(first, |acc, e| acc.and(e)))
        }
    }

    /// Requested filters that produce no predicate, each spelled as its own parameters
    /// (`f.status=draft&f.status=review`), beside one entry for a `?tab=` naming no tab and one
    /// for dropped parameters. A `refused`
    /// filter reads as an invalid value, as one naming no record does, so the answer does not
    /// tell a hidden record from a missing one. Each names why as a [`crate::dom::ignored`] kind,
    /// which [`ignored_reason`] words.
    pub(crate) fn unapplied_filters(
        &self,
        cx: &Cx,
        state: &TableState,
        refused: &[String],
    ) -> Vec<(String, &'static str)>
    where
        M: toasty::schema::Model,
    {
        let mut out = Vec::new();
        for (key, value) in &state.filters {
            match self.filters.iter().find(|f| f.name() == key) {
                None => out.push((state.spell_filter(key, value), ignored::UNKNOWN)),
                Some(f)
                    if refused.iter().any(|name| name == key)
                        || (f.to_expr(cx, value).is_none() && !f.is_noop_value(value)) =>
                {
                    out.push((state.spell_filter(key, value), ignored::INVALID))
                }
                Some(_) => {}
            }
        }
        if let Some(tab) = self.unknown_tab(state) {
            out.push((state.spell_tab(tab), ignored::UNKNOWN));
        }
        if state.filters_dropped {
            out.push(("dropped filters".to_string(), ignored::DROPPED));
        }
        out.sort();
        out
    }
}

/// The wording of a [`crate::dom::ignored`] kind, which the list's banner and the export's
/// refusal show.
pub(crate) fn ignored_reason(kind: &str) -> crate::Text {
    match kind {
        ignored::UNKNOWN => crate::text!("tablo-filter-ignored-unknown"),
        ignored::DROPPED => crate::text!(
            "tablo-filter-ignored-dropped",
            filters = super::state::MAX_FILTERS,
            values = super::state::MAX_FILTER_VALUES,
            bytes = super::state::MAX_FILTER_LEN,
        ),
        _ => crate::text!("tablo-filter-ignored-invalid"),
    }
}
