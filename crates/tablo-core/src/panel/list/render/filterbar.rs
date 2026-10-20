//! The filter controls and the unknown-filter warning.

use topcoat::{context::Cx, view::*};

use super::{
    super::{TableSignals, WiredTable},
    BAR_CLASS, Frame, QUIET_LINK_CLASS, live_link,
};
use crate::table::{TableState, filter::FilterInput, ignored_reason};

/// What a render shows of the table's filters.
pub(super) struct FilterViews<'a> {
    /// One control per filter, empty when the filter bar is hidden.
    pub(super) controls: Vec<BoxView<'a>>,
    /// Requested filters that produce no predicate, each with why, as `dom::ignored` names it.
    unapplied: Vec<(String, &'static str)>,
    /// Whether no requested filter produces a predicate.
    unfiltered: bool,
}

impl<M> WiredTable<M> {
    /// The filter controls and ignored filters a render of `state` shows, or the database failing
    /// a filter's check.
    pub(super) async fn filter_views<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
    ) -> topcoat::Result<FilterViews<'a>>
    where
        M: toasty::schema::Model,
    {
        let mut controls = Vec::new();
        if self.filter_bar_enabled() {
            for f in self.declared_filters() {
                let current = state.filters.get(f.name()).cloned().unwrap_or_default();
                let input =
                    FilterInput::new(f.name(), state.filter_param(f.name()), f.label(), current);
                controls.push(f.load_control(cx, input).await);
            }
        }
        let refused = self.refused_filters(cx, state).await?;
        let unapplied = self.unapplied_filters(cx, state, &refused);
        let unfiltered = !unapplied.is_empty() && self.filter_expr(cx, state, &refused).is_none();
        Ok(FilterViews {
            controls,
            unapplied,
            unfiltered,
        })
    }
}

impl Frame<'_> {
    /// Render the fail-visible banner for the requested filters that produce no predicate.
    pub(super) fn render_filter_warning<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        signals: &TableSignals,
        filters: &FilterViews<'_>,
    ) -> Option<BoxView<'a>> {
        let unapplied = &filters.unapplied;
        if unapplied.is_empty() {
            return None;
        }
        let entries: Vec<_> = unapplied
            .iter()
            .enumerate()
            .map(|(i, (pair, kind))| {
                let separator = if i == 0 { "" } else { ", " };
                let text = format!("{pair} ({})", ignored_reason(kind));
                (separator, pair.clone(), *kind, text)
            })
            .collect();
        let consequence = if filters.unfiltered {
            "showing unfiltered results"
        } else {
            "other filter(s) still apply"
        };
        let consequence = format!(" — {consequence}.");
        let clear = live_link(cx, state.without_filters(path), signals);
        Some(
            view! {
                cx =>
                <div
                    class="border-b border-border bg-destructive/10 px-4 py-2 text-sm text-destructive"
                    role="alert"
                >
                    "Ignored filter(s): "
                    for (separator, pair, kind, text) in entries {
                        (separator)
                        <span data-ignored=(kind) data-ignored-filter=(pair)>
                            (text)
                        </span>
                    }
                    (consequence)
                    " "
                    <a class="font-medium underline underline-offset-4" (clear)>
                        "Clear filters"
                    </a>
                </div>
            }
            .boxed(),
        )
    }

    /// Render the filter bar: the `controls`, one `f.<name>` control per filter, which the
    /// toolbar form reads.
    pub(super) fn render_filter_controls<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        signals: &TableSignals,
        controls: Vec<BoxView<'a>>,
    ) -> BoxView<'a> {
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
