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
mod tests {
    use std::collections::HashMap;

    use topcoat::context::CxTestBuilder;

    use super::{
        super::core::tests::{
            Driver, Task, filters_state, last_link_named, status_table, vehicule_filter,
        },
        *,
    };
    use crate::{
        TablePage, TableState, TextColumn,
        resource::{DateFilter, SelectFilter, Sort, TernaryFilter},
    };

    #[tokio::test]
    async fn filter_widgets_render_typed_controls() {
        let cx = CxTestBuilder::new().build();
        let table_task1 = Table::<Task>::r#for(&cx)
            .key(|t| t.id.to_string())
            .columns(TextColumn::r#for(Task::fields().title(), |t| {
                t.title.clone()
            }))
            .filters((
                SelectFilter::r#for(
                    Task::fields().status(),
                    vec!["draft".to_string(), "published".to_string()],
                ),
                TernaryFilter::r#for(Task::fields().featured()),
                DateFilter::r#for(Task::fields().created_at()),
            ));
        let page: TablePage<Task> = Vec::new().into();
        // State with an active select value pre-selects it.
        let mut filters = HashMap::new();
        filters.insert("status".to_string(), "published".to_string());
        let state = TableState {
            filters,
            ..TableState::default()
        };
        let html = table_task1
            .render_with_state(&cx, page, &state, "/admin/tasks")
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            html.contains("data-filters-form"),
            "missing filters form in {html}"
        );
        for name in ["status", "featured", "created_at"] {
            assert!(
                html.contains(&format!("data-filter-name=\"{name}\"")),
                "missing control for {name} in {html}"
            );
        }
        // Select options + current selection.
        assert!(
            html.contains("draft") && html.contains("published"),
            "missing select options in {html}"
        );
        assert!(
            html.contains("value=\"published\" selected")
                || html.contains("value=\"published\" selected=\"\""),
            "published should be selected in {html}"
        );
        // Ternary + date controls.
        assert!(
            html.contains("value=\"true\"") && html.contains("value=\"false\""),
            "missing ternary options in {html}"
        );
        assert!(
            html.contains("type=\"date\""),
            "missing date input in {html}"
        );
        // The hidden transport carries the composed value for auto-apply; the
        // free-text input + Apply button survive only as the `<noscript>`
        // fallback.
        assert!(
            html.contains("data-filters-transport")
                && html.contains("name=\"filters\"")
                && html.contains("status:published"),
            "missing hidden filters transport in {html}"
        );
        assert!(
            html.contains("<noscript>") && html.contains("Apply filters"),
            "missing no-JS filter fallback in {html}"
        );
    }

    #[tokio::test]
    async fn variant_filter_renders_select_control() {
        let cx = CxTestBuilder::new().build();
        let table_driver1 = Table::<Driver>::r#for(&cx)
            .key(|d| d.id.to_string())
            .columns(TextColumn::r#for(Driver::fields().name(), |d| {
                d.name.clone()
            }))
            .filters(vehicule_filter());
        let page: TablePage<Driver> = Vec::new().into();
        let html = table_driver1
            .render_with_state(&cx, page, &TableState::default(), "/admin/drivers")
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            html.contains("data-filter-name=\"vehicule\""),
            "missing variant control in {html}"
        );
        assert!(
            html.contains("Auto") && html.contains("Moto"),
            "missing variant options in {html}"
        );
    }

    #[tokio::test]
    async fn empty_with_filters_shows_filtered_message() {
        let cx = CxTestBuilder::new().build();
        let table_task2 = Table::<Task>::r#for(&cx)
            .key(|t| t.id.to_string())
            .columns(TextColumn::r#for(Task::fields().title(), |t| {
                t.title.clone()
            }))
            .filters(SelectFilter::r#for(
                Task::fields().status(),
                vec!["draft".to_string()],
            ));
        let mut filters = HashMap::new();
        filters.insert("status".to_string(), "draft".to_string());
        let state = TableState {
            filters,
            ..TableState::default()
        };
        let html = table_task2
            .render_with_state(&cx, Vec::new().into(), &state, "/admin/tasks")
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            html.contains("No results for these filters"),
            "filter-only empty must be distinct in {html}"
        );
        assert!(
            html.contains("Clear filters"),
            "filter-only empty needs a clear link in {html}"
        );
    }

    #[tokio::test]
    async fn unknown_filter_warns_on_an_empty_page_too() {
        // GH #93 follow-up: the zero-rows branch returned before the warning
        // banner rendered, so a typo'd filter looked like an honest "no
        // results" on an empty table.
        let cx = CxTestBuilder::new().build();
        let html = status_table(&cx)
            .render_with_state(
                &cx,
                Vec::new().into(),
                &filters_state(&[("stauts", "published")]),
                "/admin/tasks",
            )
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            html.contains("role=\"alert\"") && html.contains("stauts:published"),
            "empty page must still warn about ignored filters, got {html}"
        );
    }

    #[tokio::test]
    async fn empty_clear_links_preserve_the_untouched_state() {
        // The empty-state link rebuilds the URL from the full state, clearing
        // only the dimension it names: `group_by` survives, and with a search
        // and filters active the "Clear search" link leaves the filters alone.
        let cx = CxTestBuilder::new().build();
        let tbl = Table::<Task>::r#for(&cx)
            .key(|t| t.id.to_string())
            .columns(TextColumn::r#for(Task::fields().title(), |t| t.title.clone()).sortable())
            .filters(SelectFilter::r#for(
                Task::fields().status(),
                vec!["published".to_string()],
            ))
            .group_by("status", |t| t.status.clone());
        let state = TableState {
            search: Some("Hello".to_string()),
            filters: HashMap::from([("status".to_string(), "published".to_string())]),
            sort: Some(Sort {
                column: "title".to_string(),
                descending: true,
            }),
            group_by: Some("status".to_string()),
            ..TableState::default()
        };
        let html = tbl
            .render_with_state(&cx, Vec::new().into(), &state, "/admin/tasks")
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        let clear = last_link_named(&html, "Clear search");
        assert!(
            clear.contains("sort=title"),
            "clear search must keep sort: {clear}"
        );
        assert!(
            clear.contains("dir=desc"),
            "clear search must keep dir: {clear}"
        );
        assert!(
            clear.contains("filters="),
            "clear search must keep filters: {clear}"
        );
        assert!(
            clear.contains("group_by=status"),
            "clear search must keep group_by: {clear}"
        );
        assert!(!clear.contains("q="), "clear search must drop q: {clear}");

        let state = TableState {
            filters: HashMap::from([("status".to_string(), "published".to_string())]),
            sort: Some(Sort {
                column: "title".to_string(),
                descending: true,
            }),
            group_by: Some("status".to_string()),
            ..TableState::default()
        };
        let html = tbl
            .render_with_state(&cx, Vec::new().into(), &state, "/admin/tasks")
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        // The filter bar renders a "Clear filters" link earlier in the page;
        // the empty-cell one is the subject here.
        let clear = last_link_named(&html, "Clear filters");
        assert!(
            clear.contains("sort=title"),
            "clear filters must keep sort: {clear}"
        );
        assert!(
            clear.contains("group_by=status"),
            "clear filters must keep group_by: {clear}"
        );
        assert!(
            !clear.contains("filters="),
            "clear filters must drop filters: {clear}"
        );
    }

    #[tokio::test]
    async fn unknown_filter_renders_alert_banner_and_keeps_200() {
        // the list keeps a 200 but warns instead of lying about
        // "these filters".
        let cx = CxTestBuilder::new().build();
        let tbl = status_table(&cx);
        let rows = vec![Task {
            id: uuid::Uuid::nil(),
            title: "Hello".to_string(),
            status: "published".to_string(),
            featured: false,
            created_at: jiff::Timestamp::now(),
        }];
        let html = tbl
            .render_with_state(
                &cx,
                rows.into(),
                &filters_state(&[("stauts", "published")]),
                "/admin/tasks",
            )
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            html.contains("role=\"alert\"") && html.contains("stauts:published"),
            "typo filter must warn, got {html}"
        );

        let rows = vec![Task {
            id: uuid::Uuid::nil(),
            title: "Hello".to_string(),
            status: "published".to_string(),
            featured: false,
            created_at: jiff::Timestamp::now(),
        }];
        let html = tbl
            .render_with_state(
                &cx,
                rows.into(),
                &filters_state(&[("status", "published")]),
                "/admin/tasks",
            )
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            !html.contains("role=\"alert\""),
            "valid filter must not warn, got {html}"
        );
    }
}
