//! The filter menu and the unknown-filter warning.

use tablo_ui::{
    ButtonSize, ButtonVariant, FloatingAlign, FloatingAnchor, FloatingSide, button_variants,
    dropdown_menu_trigger, floating_variants,
};
use topcoat::{context::Cx, icon::icon, view::*};

use super::{
    super::{TableSignals, WiredTable},
    Frame, QUIET_LINK_CLASS, live_link, table_dom_id,
};
use crate::{
    t,
    table::{TableState, filter::FilterInput, ignored_reason},
};

/// What a render shows of the table's filters.
pub(super) struct FilterViews<'a> {
    /// One control per filter, empty when the filter menu is hidden.
    pub(super) controls: Vec<BoxView<'a>>,
    /// How many declared filters the request sets, which the filter menu's trigger counts.
    pub(super) active: usize,
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
        let mut active = 0;
        if self.filter_menu_enabled() {
            for f in self.declared_filters() {
                let current = state.filters.get(f.name()).cloned().unwrap_or_default();
                active += usize::from(!current.is_empty());
                let input = FilterInput::new(
                    f.name(),
                    state.filter_param(f.name()),
                    f.label().localize(cx),
                    current,
                );
                controls.push(f.load_control(cx, input).await);
            }
        }
        let refused = self.refused_filters(cx, state).await?;
        let unapplied = self.unapplied_filters(cx, state, &refused);
        let unfiltered = !unapplied.is_empty() && self.filter_expr(cx, state, &refused).is_none();
        Ok(FilterViews {
            controls,
            active,
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
                let text = t!(
                    cx,
                    "tablo-filter-ignored-entry",
                    filter = pair.clone(),
                    reason = ignored_reason(kind),
                );
                (separator, pair.clone(), *kind, text)
            })
            .collect();
        let consequence = t!(
            cx,
            "tablo-filter-ignored-consequence",
            unfiltered = if filters.unfiltered { "true" } else { "false" },
        );
        let clear = live_link(cx, state.without_filters(path), signals);
        Some(
            view! {
                cx =>
                <div
                    class="border-b border-border bg-destructive/10 px-4 py-2 text-sm text-destructive"
                    role="alert"
                >
                    (t!(cx, "tablo-filter-ignored"))
                    " "
                    for (separator, pair, kind, text) in entries {
                        (separator)
                        <span data-ignored=(kind) data-ignored-filter=(pair)>
                            (text)
                        </span>
                    }
                    (consequence)
                    " "
                    <a class="font-medium underline underline-offset-4" (clear)>
                        (t!(cx, "tablo-list-clear-filters"))
                    </a>
                </div>
            }
            .boxed(),
        )
    }

    /// Render the filter menu: a trigger counting the `active` filters, and the popover holding
    /// the `controls`, one `f.<name>` control per filter. The popover renders inside the toolbar
    /// form, which reads the controls, so a change in it reruns the table; a popover keeps its
    /// open state across the rerun.
    pub(super) fn render_filter_menu<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        signals: &TableSignals,
        (controls, active): (Vec<BoxView<'a>>, usize),
    ) -> (BoxView<'a>, BoxView<'a>) {
        let menu = table_dom_id(state, "filters");
        let reset = (!state.filters.is_empty())
            .then(|| live_link(cx, state.without_filters(path), signals));
        let count = (active > 0).then(|| active.to_string());
        let target = menu.clone();
        let label = t!(cx, "tablo-list-filters");
        let trigger_label = label.clone();
        let trigger = view! {
            cx =>
            dropdown_menu_trigger(
                target: &target,
                attrs: attributes! {
                    class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))
                },
                icon(data: tablo_ui::icons::LIST_FILTER)
                (trigger_label)
                if let Some(count) = count {
                    <span class=(FILTER_COUNT_CLASS)>(count)</span>
                }
            )
        }
        .boxed();
        let panel = view! {
            cx =>
            <div
                id=(menu)
                popover="auto"
                aria-label=(label.clone())
                class=(class!(
                    floating_variants(
                        FloatingAnchor::Invoker,
                        FloatingSide::Bottom,
                        FloatingAlign::End,
                    ),
                    FILTER_PANEL_CLASS,
                ))
            >
                <div
                    class="flex h-11 items-center justify-between border-b border-border px-4"
                >
                    <p class="text-sm font-semibold">(label)</p>
                    if let Some(reset) = reset {
                        <a class=(QUIET_LINK_CLASS) (reset)>
                            (t!(cx, "tablo-list-reset-filters"))
                        </a>
                    }
                </div>
                <div class="grid grid-cols-1 gap-4 p-4">
                    for control in controls {
                        (control)
                    }
                </div>
            </div>
        }
        .boxed();
        (trigger, panel)
    }
}

/// The filter menu's popover, under its trigger.
const FILTER_PANEL_CLASS: StaticClass = class!(
    "w-80 max-w-[calc(100vw-2rem)] rounded-xl border border-border bg-popover",
    "text-popover-foreground shadow-sm outline-none",
);

/// The trigger's count of set filters.
const FILTER_COUNT_CLASS: StaticClass = class!(
    "-mr-1 inline-flex h-5 min-w-5 items-center justify-center rounded-full bg-primary px-1.5",
    "text-xs font-medium tabular-nums text-primary-foreground",
);
