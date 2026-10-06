//! The filter bar, live filter bar, and unknown-filter warning.

use tablo_ui::{ButtonSize, ButtonVariant, button};
use topcoat::{Result, context::Cx, runtime::Event, view::*};

use super::{super::WiredTable, BAR_CLASS, QUIET_LINK_CLASS, toolbar::hidden_state_inputs};
use crate::table::{
    filter::FilterInput,
    state::{TableSignals, TableState},
};

impl<M> WiredTable<M> {
    /// Render the fail-visible banner for requested filters that produce no predicate.
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

    /// Render the filter bar for a live table from the page that owns the signals.
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

    /// Render the typed filter bar as a GET form of `f.<name>` controls.
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
