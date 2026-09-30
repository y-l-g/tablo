//! The filter bar, live filter bar, and unknown-filter warning.

use tablo_ui::{ButtonSize, ButtonVariant, button};
use topcoat::{Result, context::Cx, runtime::Event, view::*};

use super::{
    super::{
        super::{
            filter::Filter,
            state::{TableSignals, TableState},
        },
        Table,
    },
    BAR_CLASS, QUIET_LINK_CLASS,
    toolbar::hidden_state_inputs,
};

/// A filter control's label, beside its control.
const FILTER_LABEL_CLASS: StaticClass =
    class!("flex items-center gap-2 text-sm font-medium whitespace-nowrap text-muted-foreground");

/// One filter control: a labelled `<select name="f.<name>">` (prefixed like the
/// table's other parameters, `param`) carrying the
/// `value`/`label` pairs, with the leading empty "All" option that clears the
/// filter.
///
/// The empty value is reserved for that clear-filter option. Every pair in
/// `options` renders verbatim, so a caller whose declared options can include
/// `""` supplies the label that option shows: [`Filter::Select`] passes
/// `"All"`, the label the empty value already carries, while
/// [`Filter::Variant`] passes the key itself.
///
/// The control is a real form field, so the GET form submits it as
/// `?f.<name>=<value>`; an empty value is no filter. `data-filter-name` is the
/// hook `filters.js` reads on a live table.
fn filter_select<'a>(
    cx: &'a Cx,
    label: &str,
    name: &str,
    param: String,
    options: Vec<(String, String)>,
    current: &str,
) -> BoxView<'a> {
    let label = label.to_string();
    let name = name.to_string();
    let aria = label.clone();
    let option_views: Vec<BoxView<'a>> = std::iter::once((String::new(), "All".to_string()))
        .chain(options)
        .map(|(value, text)| {
            let selected = current == value;
            crate::schema::option_view(cx, value, text, selected)
        })
        .collect();
    view! {
        cx =>
        <label class=(FILTER_LABEL_CLASS)>
            (label)
            tablo_ui::select(
                attrs: attributes! {
                    class="min-w-32"
                    name=(param)
                    data-filter-name=(name)
                    aria-label=(aria)
                },
                for option in option_views {
                    (option)
                }
            )
        </label>
    }
    .boxed()
}

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
        let mut controls: Vec<BoxView<'_>> = Vec::with_capacity(self.filters.len());
        for f in &self.filters {
            let current = state.filters.get(f.name()).cloned().unwrap_or_default();
            match f {
                Filter::Select(s) => {
                    // A declared empty option is the clear-filter value, so it
                    // renders as the "All" option: value `""`, label "All".
                    let options = s
                        .options()
                        .iter()
                        .map(|opt| {
                            let label = if opt.is_empty() { "All" } else { opt.as_str() };
                            (opt.clone(), label.to_string())
                        })
                        .collect();
                    controls.push(filter_select(
                        cx,
                        s.label_str(),
                        s.name(),
                        state.filter_param(s.name()),
                        options,
                        &current,
                    ));
                }
                Filter::Ternary(t) => {
                    let options = vec![
                        ("true".to_string(), "True".to_string()),
                        ("false".to_string(), "False".to_string()),
                    ];
                    controls.push(filter_select(
                        cx,
                        t.label_str(),
                        t.name(),
                        state.filter_param(t.name()),
                        options,
                        &current,
                    ));
                }
                Filter::Date(d) => {
                    let name = d.name().to_string();
                    let param = state.filter_param(&name);
                    let label = d.label_str().to_string();
                    let aria = label.clone();
                    // `<input type=date>` needs YYYY-MM-DD; truncate RFC3339.
                    let date_value = current.split('T').next().unwrap_or(&current).to_string();
                    controls.push(
                        view! {
                            cx =>
                            <label class=(FILTER_LABEL_CLASS)>
                                (label)
                                tablo_ui::input(
                                    attrs: attributes! {
                                        type="date"
                                        name=(param)
                                        data-filter-name=(name)
                                        value=(date_value)
                                        aria-label=(aria)
                                        class="w-auto!"
                                    }
                                )
                            </label>
                        }
                        .boxed(),
                    );
                }
                Filter::Variant(v) => {
                    let options = v
                        .options()
                        .iter()
                        .map(|(key, _)| (key.clone(), key.clone()))
                        .collect();
                    controls.push(filter_select(
                        cx,
                        v.label_str(),
                        v.name(),
                        state.filter_param(v.name()),
                        options,
                        &current,
                    ));
                }
            }
        }
        let form_attrs = attributes! {
            cx =>
            method="get"
            action=(action)
            class=(BAR_CLASS)
            data-filters-form=""
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
