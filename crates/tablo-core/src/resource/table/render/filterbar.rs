//! The filter bar, live filter bar, and unknown-filter warning.

use tablo_ui::{ButtonSize, ButtonVariant, button, input as ui_input};
use topcoat::{Result, context::Cx, runtime::Event, view::*};

use super::{
    super::{
        super::{
            filter::Filter,
            state::{TableSignals, TableState},
        },
        NormalizedState, Table,
    },
    toolbar::hidden_state_inputs,
};

/// One filter control: a labelled `<select data-filter-name=…>` carrying the
/// `value`/`label` pairs, with the leading empty "All" option that clears the
/// filter.
///
/// The empty value is reserved for that clear-filter option. Every pair in
/// `options` renders verbatim, so a caller whose declared options can include
/// `""` supplies the label that option shows: [`Filter::Select`] passes
/// `"All"`, the label the empty value already carries, while
/// [`Filter::Variant`] passes the key itself.
///
/// The control has no `name`, so it never submits on its own; `filters.js`
/// composes it into the hidden `filters` transport.
fn filter_select<'a>(
    cx: &'a Cx,
    label: &str,
    name: &str,
    options: Vec<(String, String)>,
    current: &str,
) -> BoxView<'a> {
    let label = label.to_string();
    let name = name.to_string();
    let aria = label.clone();
    let current = current.to_string();
    view! {
        cx =>
        <label class="flex items-center gap-2 text-sm text-muted-foreground">
            (label)
            <select
                data-filter-name=(name)
                aria-label=(aria)
                class="flex h-9 rounded-md border border-border bg-background px-3 py-1 text-sm shadow-xs"
            >
                <option value="" selected=(current.is_empty())>"All"</option>
                for (value, text) in options {
                    <option value=(value.clone()) selected=(current == value)>
                        (text)
                    </option>
                }
            </select>
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
        state: &NormalizedState,
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
                    class="border-b border-destructive/30 bg-muted px-4 py-2 text-sm"
                    role="alert"
                >
                    (text)
                    " "
                    <a href=(clear) class="underline">"Clear filters"</a>
                </div>
            }
            .boxed(),
        )
    }

    /// The filter bar for a live table, rendered eagerly by the page that owns
    /// the signals — the counterpart of [`Self::render_live_search_bar`].
    ///
    /// Hoisting matters for focus: a `<select>` change writes the `filters`
    /// signal, and a bar rebuilt by that rerun would collapse the native popup
    /// and drop keyboard context. The table renders without the bar
    /// (`Table::filter_bar(false)`), so the control the user touched is never
    /// replaced.
    pub async fn render_live_filter_bar<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        signals: &TableSignals,
    ) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model,
    {
        // Called with raw page state: normalize so the no-JS
        // fallback form carries the same normalized values the GET path would.
        self.render_live_filter_bar_normalized(cx, &self.normalize_state(state), path, signals)
            .await
    }

    /// [`Self::render_live_filter_bar`] with the state already normalized
    /// the panel's live page renders the hoisted bar from the
    /// request's one normalized state.
    pub(crate) async fn render_live_filter_bar_normalized<'a>(
        &self,
        cx: &'a Cx,
        state: &NormalizedState,
        path: &str,
        signals: &TableSignals,
    ) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model,
    {
        self.render_filter_bar(cx, state, path, Some(signals)).await
    }

    /// The typed filter bar. For live tables (`signals`) the hidden `filters`
    /// transport is bound to the `filters` signal and `filters.js` dispatches
    /// a `change` into it instead of submitting, so the shard re-renders the
    /// table in place; the GET form stays as the no-JS fallback and `href`s
    /// remain real.
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
        let filters_display = state.filters_param().unwrap_or_default();
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
                ("q", q_hidden),
                ("sort", sort_hidden),
                ("dir", dir_hidden),
                ("group_by", group_hidden),
            ],
        );
        let clear_url = if !state.filters.is_empty() {
            Some(state.without_filters(path))
        } else {
            None
        };
        // One typed control per declared filter. Controls carry only
        // `data-filter-name` (no `name`, so they never submit on their own);
        // `filters.js` composes them into the hidden `filters` transport and
        // submits on change, rewriting it even when every control is
        // "All" so the stale value can never be resubmitted. The free-text
        // input and Apply button survive only inside `<noscript>` as the
        // no-JS fallback.
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
                        options,
                        &current,
                    ));
                }
                Filter::Date(d) => {
                    let name = d.name().to_string();
                    let label = d.label_str().to_string();
                    let aria = label.clone();
                    // `<input type=date>` needs YYYY-MM-DD; truncate RFC3339.
                    let date_value = current.split('T').next().unwrap_or(&current).to_string();
                    controls.push(
                        view! {
                            cx =>
                            <label
                                class="flex items-center gap-2 text-sm text-muted-foreground"
                            >
                                (label)
                                <input
                                    type="date"
                                    data-filter-name=(name)
                                    value=(date_value)
                                    aria-label=(aria)
                                    class="flex h-9 rounded-md border border-border bg-background px-3 py-1 text-sm shadow-xs"
                                >
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
            class="flex flex-wrap items-center gap-2 border-b border-border p-3"
            data-filters-form=""
            if signals.is_some() {
                data-filters-live=""
            }
        };
        // Live tables bind the transport to the `filters` signal: `filters.js`
        // composes and dispatches, the shard re-renders in place. Static
        // tables keep the server-rendered value the GET form submits.
        let transport_attrs = if let Some(signals) = signals {
            let (filters, cursor) = (signals.filters.clone(), signals.cursor.clone());
            let none = crate::resource::cursor_none();
            attributes! {
                cx =>
                name="filters"
                :value=$(filters.get())
                @change=$(|e: Event| {
                    filters.set(e.target.value);
                    cursor.set(none.clone());
                })
                data-filters-transport=""
            }
        } else {
            attributes! {
                cx =>
                name="filters"
                value=(filters_display.clone())
                data-filters-transport=""
            }
        };
        let clear_link: Option<BoxView<'a>> = clear_url.map(|url| {
            let attrs = match signals {
                Some(signals) => {
                    let (filters, cursor) = (signals.filters.clone(), signals.cursor.clone());
                    let none = crate::resource::cursor_none();
                    attributes! {
                        cx =>
                        href=(url.clone())
                        @click=$(|e: Event| {
                            e.prevent_default();
                            filters.set("".to_owned());
                            cursor.set(none.clone());
                        })
                    }
                }
                None => attributes! { cx => href=(url) },
            };
            view! {
                cx =>
                <a class="text-sm text-muted-foreground hover:text-foreground" (attrs)>
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
                    ui_input(
                        attrs: attributes! {
                            type="text"
                            name="filters"
                            value=(filters_display)
                            placeholder="filters e.g. status:published"
                            aria-label="Filter table (free text)"
                            class="w-64"
                        }
                    )
                    button(
                        variant: ButtonVariant::Secondary,
                        size: ButtonSize::Md,
                        attrs: attributes! { type="submit" },
                        "Apply filters"
                    )
                </noscript>
                <input type="hidden" (transport_attrs)>
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
