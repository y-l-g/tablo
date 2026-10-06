//! The filter controls and the unknown-filter warning.

use topcoat::{context::Cx, view::*};

use super::{super::WiredTable, BAR_CLASS, QUIET_LINK_CLASS, live_link};
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
        signals: &TableSignals,
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
        let clear = live_link(cx, state.without_filters(path), signals);
        Some(
            view! {
                cx =>
                <div
                    class="border-b border-border bg-destructive/10 px-4 py-2 text-sm text-destructive"
                    role="alert"
                >
                    (text)
                    " "
                    <a class="font-medium underline underline-offset-4" (clear)>
                        "Clear filters"
                    </a>
                </div>
            }
            .boxed(),
        )
    }

    /// Render the filter bar: one `f.<name>` control per filter, which the toolbar form reads.
    pub(super) fn render_filter_controls<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        signals: &TableSignals,
    ) -> BoxView<'a>
    where
        M: toasty::schema::Model,
    {
        let controls: Vec<BoxView<'_>> = self
            .filters
            .iter()
            .map(|f| {
                let current = state.filters.get(f.name()).cloned().unwrap_or_default();
                f.control(
                    cx,
                    FilterInput::new(f.name(), state.filter_param(f.name()), f.label(), current),
                )
            })
            .collect();
        let clear = (!state.filters.is_empty())
            .then(|| live_link(cx, state.without_filters(path), signals));
        view! {
            cx =>
            <div class=(BAR_CLASS)>
                for control in controls {
                    (control)
                }
                if let Some(clear) = clear {
                    <a class=(QUIET_LINK_CLASS) (clear)>"Clear filters"</a>
                }
            </div>
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests;
