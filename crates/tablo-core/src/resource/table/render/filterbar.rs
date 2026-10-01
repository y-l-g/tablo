//! The filter bar, live filter bar, and unknown-filter warning.

use tablo_ui::{ButtonSize, ButtonVariant, button};
use topcoat::{Result, context::Cx, runtime::Event, view::*};

use super::{
    super::{
        super::{
            filter::FilterInput,
            state::{TableSignals, TableState},
        },
        Table,
    },
    BAR_CLASS, QUIET_LINK_CLASS,
    toolbar::hidden_state_inputs,
};

impl<M> Table<M> {
    /// The fail-visible filter banner: requested filters that produced
    /// no predicate render as a `role=alert` banner; the list keeps a 200 while
    /// the export refuses with 400 (see `resource_export`).
    ///
    /// No false tail: when other filters still apply, "unfiltered" would be a
    /// lie — a malformed segment can ride alongside valid ones.
    /// Conversely an invalid-only request applies nothing, so "other filter(s)"
    /// would be the lie — the consequence keys off applied
    /// predicates, not raw entries.
    pub(super) fn render_filter_warning<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
    ) -> Option<BoxView<'a>>
    where
        M: toasty::schema::Model,
    {
        let unapplied = self.unapplied_filters(state);
        if unapplied.is_empty() {
            return None;
        }
        let detail = unapplied
            .iter()
            .map(|(pair, reason)| format!("{pair} ({reason})"))
            .collect::<Vec<_>>()
            .join(", ");
        let consequence = if self.filter_expr(state).is_none() {
            "showing unfiltered results"
        } else {
            "other filter(s) still apply"
        };
        let text = format!("Ignored filter(s): {detail} — {consequence}.");
        let clear = state.without_filters(path);
        Some(
            view! {
                cx =>
                <div
                    class="border-b border-border bg-destructive/10 px-4 py-2 text-sm text-destructive"
                    role="alert"
                >
                    (text)
                    " "
                    <a href=(clear) class="font-medium underline underline-offset-4">
                        "Clear filters"
                    </a>
                </div>
            }
            .boxed(),
        )
    }

    /// The filter bar for a live table, rendered eagerly by the page that owns
    /// the signals — the counterpart of [`Self::render_live_search_bar`].
    ///
    /// Hoisting matters for focus: a `<select>` change writes the `query`
    /// signal, and a bar rebuilt by that rerun would collapse the native popup
    /// and drop keyboard context. The table renders without the bar
    /// (`Table::hide_filter_bar`), so the control the user touched is never
    /// replaced.
    pub(crate) async fn render_live_filter_bar<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        signals: &TableSignals,
    ) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model,
    {
        self.render_filter_bar(cx, state, path, Some(signals)).await
    }

    /// The typed filter bar: a GET form whose controls are the `f.<name>`
    /// parameters. `filters.js` submits it on change. For live tables
    /// (`signals`) a hidden transport is bound to the `query` signal instead,
    /// and `filters.js` rewrites the query's filter parameters in it, so the
    /// shard re-renders the table in place; the form stays the no-JS fallback
    /// and `href`s remain real.
    pub(super) async fn render_filter_bar<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        signals: Option<&TableSignals>,
    ) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model,
    {
        // No `filters.is_empty()` early return: the caller (`render_inner`)
        // already guards on `show_filters`, so an empty bar is unreachable.
        let action = path.to_string();
        let sort_hidden = state.sort.as_ref().map(|s| s.column.clone());
        let dir_hidden = state.sort.as_ref().map(|s| {
            if s.descending {
                "desc".to_string()
            } else {
                "asc".to_string()
            }
        });
        let q_hidden = state.search.clone();
        let group_hidden = state.group_by.clone();
        let hidden = hidden_state_inputs(
            cx,
            vec![
                (state.param("q"), q_hidden),
                (state.param("sort"), sort_hidden),
                (state.param("dir"), dir_hidden),
                (state.param("group_by"), group_hidden),
            ],
        );
        let clear_url = if !state.filters.is_empty() {
            Some(state.without_filters(path))
        } else {
            None
        };
        // One typed control per declared filter, each a real `f.<name>` field.
        // `filters.js` submits on change; the Apply button survives only
        // inside `<noscript>` as the no-JS path.
        // The query prefix a relation's parameters carry (`comments.`), so
        // `filters.js` rewrites the table's own filters and cursor instead of
        // the list's. Absent on a page-owned list, whose parameters are bare.
        let query_prefix = state.prefix.as_deref().map(|prefix| format!("{prefix}."));
        let mut controls: Vec<BoxView<'_>> = Vec::with_capacity(self.filters.len());
        for f in &self.filters {
            let current = state.filters.get(f.name()).cloned().unwrap_or_default();
            let input =
                FilterInput::new(f.name(), state.filter_param(f.name()), f.label(), current);
            controls.push(f.control(cx, input));
        }
        let form_attrs = attributes! {
            cx =>
            method="get"
            action=(action)
            class=(BAR_CLASS)
            data-filters-form=""
            data-query-prefix=(query_prefix)
            if signals.is_some() {
                data-filters-live=""
            }
        };
        // Live tables bind a transport to the `query` signal: `filters.js`
        // rewrites the query's filter parameters in it and dispatches, and the
        // shard re-renders in place. It has no `name`, so the GET form never
        // submits it.
        let transport: Option<BoxView<'a>> = signals.map(|signals| {
            let query = signals.query.clone();
            view! {
                cx =>
                <input
                    type="hidden"
                    :value=$(query.get())
                    @change=$(|e: Event| query.set(e.target.value))
                    data-filters-transport=""
                >
            }
            .boxed()
        });
        // A plain link: the bar is rendered once, so on a live table
        // `filters.js` clears the filters from the transport's current query
        // rather than writing this page-load URL over newer state.
        let clear_link: Option<BoxView<'a>> = clear_url.map(|url| {
            view! {
                cx =>
                <a class=(QUIET_LINK_CLASS) href=(url) data-filters-clear="">
                    "Clear filters"
                </a>
            }
            .boxed()
        });
        Ok(view! {
            cx =>
            <form (form_attrs)>
                (hidden)
                for ctl in controls {
                    (ctl)
                }
                <noscript>
                    button(
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Md,
                        attrs: attributes! { type="submit" },
                        "Apply filters"
                    )
                </noscript>
                if let Some(transport) = transport {
                    (transport)
                }
                if let Some(link) = clear_link {
                    (link)
                }
            </form>
        }
        .boxed())
    }
}
#[cfg(test)]
mod tests;
