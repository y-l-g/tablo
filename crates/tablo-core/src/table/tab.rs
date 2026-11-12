//! List tabs: the [`ListTab`]s a table offers above its rows, the one `?tab=` picks, and the
//! counts their badges show.

use derive_where::derive_where;
use toasty::stmt::{Expr, IntoExpr, List, Path, Query};
use topcoat::{Result, context::Cx};

use super::{Table, TableState};
use crate::{DeclarationErrorKind, lens::Reads, schema::Options, values::FormScalar};

/// One tab a table offers above its rows: a preset predicate the reader picks with
/// `?tab=<key>`, beside the search and the filters it combines with.
///
/// The first tab is the one a list without `?tab=` shows, so a list whose first tab narrows its
/// rows opens on that tab. A `?tab=` naming no tab the table declares is never dropped silently:
/// the list shows its warning banner, and the export and a selection of every matching record
/// refuse it, as they do an unknown filter.
///
/// A tab narrows what the list shows; it is not access control. A tab whose query reads a field
/// [`field_access`](crate::ResourceDef::field_access) hides from the user does not show to them:
/// the panel reads the fields off the query.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String, published: bool }
/// use tablo_core::{ListTab, Table, TextColumn, lens};
///
/// Table::new(TextColumn::new(lens!(Post.title)))
///     .tab(ListTab::new("all", "All"))
///     .tab(
///         ListTab::new("drafts", "Drafts")
///             .query(Post::fields().published().eq(false))
///             .badge(),
///     );
/// ```
#[derive_where(Clone)]
pub struct ListTab<M> {
    key: String,
    label: crate::Text,
    /// The rows the tab admits; `None` admits every row.
    query: Option<Expr<bool>>,
    /// Whether the tab shows how many rows it holds.
    badge: bool,
    _model: std::marker::PhantomData<fn() -> M>,
}

impl<M> ListTab<M> {
    /// A tab admitting every row, whose `?tab=` value is `key` and whose link reads `label`.
    pub fn new(key: impl Into<String>, label: impl Into<crate::Text>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            query: None,
            badge: false,
            _model: std::marker::PhantomData,
        }
    }

    /// Admit only the rows `predicate` selects.
    pub fn query(mut self, predicate: Expr<bool>) -> Self {
        self.query = Some(predicate);
        self
    }

    /// Show how many rows the tab holds under the list's search and filters: one count query
    /// per badged tab, each time the list renders.
    pub fn badge(mut self) -> Self {
        self.badge = true;
        self
    }

    /// The `?tab=` value.
    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    /// The link's label.
    pub(crate) fn label(&self) -> crate::Text {
        self.label.clone()
    }

    /// Whether the tab shows its count.
    pub(crate) fn has_badge(&self) -> bool {
        self.badge
    }

    /// The fields the tab's query reads off a row; a tab with no query reads nothing.
    pub(crate) fn reads(&self) -> Reads<M>
    where
        M: toasty::schema::Model,
    {
        self.query
            .as_ref()
            .map_or_else(Reads::nothing, Reads::of_predicate)
    }
}

impl<M> Table<M> {
    /// Offer `tab` above the rows, after the declared ones.
    pub fn tab(mut self, tab: ListTab<M>) -> Self {
        self.tabs.push(tab);
        self
    }

    /// Offer a tab admitting every row, keyed `all`, then a tab per option of the [`Options`]
    /// field `lens` binds, in declaration order, after the declared ones. An option's tab is keyed
    /// by its value and labelled by its label, admits the rows holding it, and shows their count.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo_core::Options)]
    /// # enum Status { Draft, Published }
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String, status: Status }
    /// use tablo_core::{Table, TextColumn, lens};
    ///
    /// // `?tab=all`, `?tab=draft` and `?tab=published`.
    /// Table::new(TextColumn::new(lens!(Post.title))).option_tabs(lens!(Post.status));
    /// ```
    ///
    /// An option whose value the field's type does not parse is a declaration error, since its
    /// tab would admit no row.
    pub fn option_tabs<T>(mut self, lens: impl Into<Path<M, T>>) -> Self
    where
        T: Options + FormScalar + IntoExpr<T>,
    {
        let path: Path<M, T> = lens.into();
        self.tabs
            .push(ListTab::new("all", crate::text!("tablo-list-tab-all")));
        for (value, label) in T::options() {
            match T::parse_form(&value) {
                Ok(option) => self.tabs.push(
                    ListTab::new(value, label)
                        .query(path.clone().eq(option))
                        .badge(),
                ),
                Err(_) => self
                    .misdeclared
                    .push(DeclarationErrorKind::UnparsedTabOption { value }),
            }
        }
        self
    }

    /// The declared tabs, in order.
    pub(crate) fn declared_tabs(&self) -> &[ListTab<M>] {
        &self.tabs
    }

    /// The tab `state` shows: the one its `?tab=` names, else the first. `None` when the table
    /// declares no tab, or `?tab=` names none it declares.
    pub(crate) fn active_tab(&self, state: &TableState) -> Option<&ListTab<M>> {
        match &state.tab {
            Some(key) => self.tabs.iter().find(|tab| tab.key == *key),
            None => self.tabs.first(),
        }
    }

    /// `state`'s `?tab=` when it names no tab the table declares.
    pub(crate) fn unknown_tab<'s>(&self, state: &'s TableState) -> Option<&'s str> {
        state
            .tab
            .as_deref()
            .filter(|key| self.tabs.iter().all(|tab| tab.key != *key))
    }

    /// Narrow `query` to the rows the tab `state` shows admits.
    pub(super) fn narrow_tab(&self, query: Query<List<M>>, state: &TableState) -> Query<List<M>>
    where
        M: toasty::schema::Model,
    {
        match self.active_tab(state).and_then(|tab| tab.query.clone()) {
            Some(predicate) => query.filter(predicate),
            None => query,
        }
    }

    /// How many rows of `query` each badged tab holds under `state`'s search and filters, but the
    /// `refused` filters: one count query per badged tab, by key, in declaration order.
    ///
    /// # Errors
    ///
    /// The database failing a count.
    pub(crate) async fn tab_counts(
        &self,
        cx: &Cx,
        query: &Query<List<M>>,
        state: &TableState,
        refused: &[String],
    ) -> Result<Vec<(String, u64)>>
    where
        M: toasty::schema::Model,
    {
        let mut counts = Vec::new();
        if !self.tabs.iter().any(ListTab::has_badge) {
            return Ok(counts);
        }
        let mut db = crate::db::db(cx)?;
        let narrowed = self.narrow_untabbed(cx, &db, query.clone(), state, refused);
        for tab in self.tabs.iter().filter(|tab| tab.badge) {
            let rows = match &tab.query {
                Some(predicate) => narrowed.clone().filter(predicate.clone()),
                None => narrowed.clone(),
            };
            let count = crate::toasty_compat::count(rows, &mut db)
                .await
                .map_err(crate::error::unavailable)?;
            counts.push((tab.key.clone(), count));
        }
        Ok(counts)
    }

    /// What is wrong with the declared tabs: two sharing a key.
    pub(super) fn tab_errors(&self) -> Vec<DeclarationErrorKind> {
        let mut seen = std::collections::HashSet::with_capacity(self.tabs.len());
        self.tabs
            .iter()
            .filter(|tab| !seen.insert(tab.key.as_str()))
            .map(|tab| DeclarationErrorKind::DuplicateTab {
                key: tab.key.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
