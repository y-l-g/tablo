//! Generic resource list page + live-search host + table page loader.
//!
//! One generic handler drives every resource's list; the live variant owns
//! the interaction signals and fills the table through the `search` module's
//! shard. Shared table helpers (`wire_table_actions`, `table_error_view`)
//! keep the streamed page and the shard from drifting.

use topcoat::{
    Result,
    context::Cx,
    icon::icon,
    router::{Body, error::forbidden},
    runtime::Event,
    view::{BoxView, HoistView, ViewExt, attributes, internal::ThenView, suspense, view},
};

use super::gate::{gate, list_url};
use crate::{
    form::RecordForm,
    resource::{
        Resource, RowActions, TABLE_CARD_CLASS, Table, TableAction, TableChrome, TablePage,
        TableSignals, TableState, create_page_url, request_query,
    },
};

/// Retry link for a failed streamed table load.
///
/// A malformed `?after=`/`?before=` cursor, or a cursor the query's ordering
/// refuses, is the failure itself: retrying the identical URL loops forever, so drop
/// pagination from the link and keep the rest of the state
/// (search/sort/filters/grouping). Every other failure keeps pagination too
/// so a transient blip retries the same evidence.
pub(crate) fn retry_url_for_error(
    state: &TableState,
    error: &topcoat::Error,
    path: &str,
) -> String {
    if crate::error::TabloError::is_cursor(error) {
        state.without_cursor(path)
    } else {
        state.list_url(path)
    }
}

/// The action chrome a resource's declarations imply: the one derivation
/// [`wire_table_actions`] reads to decide which affordances the table it serves
/// carries. Each column follows what also governs its route — the delete
/// handlers check `can_delete_any`, only a record form registers the edit
/// route, and only a `view` schema serves the detail page — and is answered
/// once per request, so the streamed skeleton and the table agree.
pub(crate) fn declared_chrome<R: Resource>(cx: &Cx) -> TableChrome {
    TableChrome {
        delete: R::can_delete_any(cx),
        edit: <R::Form as RecordForm>::HAS_FORM,
        view: R::viewed(cx),
        actions: true,
    }
}

/// Delete/bulk/edit action wiring shared by the streamed list and the
/// live-search shard: the delete form posts to `{list url}/{id}/delete` and the
/// bulk bar to `{list url}/bulk-delete`, both derived from the panel
/// declaration (not the request path) so the URLs are right wherever the table
/// renders.
///
/// Each affordance renders only where [`declared_chrome`] allows it, so no
/// control ships whose route cannot answer it. The per-*record* gate rides the
/// same call: the
/// table's row policy pairs each action with exactly what its route checks —
/// `can_view` for View, `can_view` + `can_update` for Edit, `can_view` +
/// `can_delete` for Delete, and `can_view` + `can_run` for each custom action;
/// the bulk checkbox renders when bulk delete or any bulk custom action allows
/// the record. A row the predicate refuses
/// renders no link and no checkbox, while the handler keeps its
/// all-or-nothing check for a hand-crafted POST.
///
/// `live` selects the shard variant: the swapped region is everything except the
/// toolbar the page owns eagerly (the live host owns those slots, so a swap must
/// never nest invocations or duplicate inputs), hence the shard forces
/// `.hide_search().hide_filter_bar()` while the streamed page keeps the declared
/// table as-is. The filter bar joins the search toolbar there: a control rebuilt
/// by its own rerun loses focus.
pub(crate) fn wire_table_actions<R: Resource>(cx: &Cx, live: bool) -> Table<R::Model> {
    wire_table::<R>(cx, live, declared_chrome::<R>(cx))
}

/// [`wire_table_actions`] with the affordances `chrome` names, a subset of
/// [`declared_chrome`]: a read-only relation keeps only the View link.
pub(crate) fn wire_table<R: Resource>(cx: &Cx, live: bool, chrome: TableChrome) -> Table<R::Model> {
    let mut table = R::table(cx);
    if live {
        // The page draws the card around the hoisted bars and this output.
        table = table.hide_search().hide_filter_bar().unframed();
    }
    // `Cx` is Arc-backed and `Clone`, so the projection owns one: the policy
    // outlives the request borrow without copying request state.
    let policy_cx = cx.clone();
    table = table.row_actions(move |record| {
        // Read once: every route pairs its own predicate with `can_view`, so a
        // record that cannot be viewed allows no action.
        let view = R::can_view(&policy_cx, record);
        RowActions {
            view,
            edit: view && R::can_update(&policy_cx, record),
            delete: view && R::can_delete(&policy_cx, record),
        }
    });
    if chrome.delete {
        table = table
            .with_delete(list_url(cx, &R::slug()))
            .with_bulk_delete(true);
    }
    if chrome.edit {
        table = table.with_edit(list_url(cx, &R::slug()));
    }
    if chrome.view {
        table = table.with_view(list_url(cx, &R::slug()));
    }
    if chrome.actions {
        table = wire_custom_actions::<R>(cx, table);
    }
    table
}

/// Attach `R`'s custom actions to `table`, each gated per record by exactly
/// what its route checks: [`Resource::can_view`] and the action's
/// `can_run`.
fn wire_custom_actions<R: Resource>(cx: &Cx, table: Table<R::Model>) -> Table<R::Model> {
    let actions = R::actions();
    if actions.entries().is_empty() {
        return table;
    }
    let wired = actions
        .entries()
        .iter()
        .map(|action| {
            let can_run = action.can_run;
            let policy_cx = cx.clone();
            TableAction {
                name: action.name,
                label: (action.label)(),
                row: action.row,
                bulk: action.bulk,
                allowed: std::sync::Arc::new(move |record: &R::Model| {
                    R::can_view(&policy_cx, record) && can_run(&policy_cx, record)
                }),
            }
        })
        .collect();
    table.with_custom_actions(list_url(cx, &R::slug()), wired)
}

/// Branded in-region table failure shared by the streamed list and the
/// live-search shard: the trace line, the cursor-aware
/// retry link ([`retry_url_for_error`]), and the `ErrorState` render are one
/// copy so the three load sites cannot drift.
///
/// On a live table (`signals`) the retry stays in place instead of
/// navigating, and re-runs the request that failed with the query it failed
/// with: the click increments a retry token the shard reads, so the
/// rerun does not depend on the query signals changing — a write of an
/// unchanged value re-runs nothing. A cursor failure resets only the cursor,
/// the same reset its `href` spells out; every other failure keeps the whole
/// query. `href` stays as the no-JS fallback, so it retries the URL as it
/// stands.
pub(crate) fn table_error_view<'a, R: Resource>(
    cx: &'a Cx,
    state: &TableState,
    error: &topcoat::Error,
    path: &str,
    signals: Option<&TableSignals>,
) -> BoxView<'a> {
    tracing::error!(resource = R::slug(), error = %error, "table load failed");
    let retry_url = retry_url_for_error(state, error, path);
    let action: BoxView<'a> = match signals {
        Some(signals) => {
            // Retry re-runs the shard. The token is the rerun's only
            // cause: the shard reads it (declaring the dependency below), and
            // every click increments it, so the write always changes even when
            // the query signals already hold the values that failed.
            // Keyed by the list, like the table's own signals.
            let attempt = topcoat::runtime::signal(&cx.keyed(path), || 0u64);
            let cursor_error = crate::error::TabloError::is_cursor(error);
            let attrs = if cursor_error {
                // The retry URL drops the cursor: write its query, so the rerun
                // loads the first page of the same result set.
                let query = signals.query.clone();
                let next = crate::resource::query_of(&retry_url).to_string();
                attributes! {
                    cx =>
                    href=(retry_url)
                    data-retry-attempt=(attempt.get())
                    @click=$(|e: Event| {
                        e.prevent_default();
                        query.set(next.clone());
                        attempt.increment();
                    })
                }
            } else {
                attributes! {
                    cx =>
                    href=(retry_url)
                    data-retry-attempt=(attempt.get())
                    @click=$(|e: Event| {
                        e.prevent_default();
                        attempt.increment();
                    })
                }
            };
            view! { cx => <a (attrs)>"Retry"</a> }.boxed()
        }
        None => view! { cx => <a href=(retry_url)>"Retry"</a> }.boxed(),
    };
    view! {
        cx =>
        tablo_ui::error_state(
            title: format!("Couldn't load {}", R::navigation_label()),
            detail: "Something went wrong while loading the records.",
            action: Some(action.into()),
            attrs: attributes! { role="alert" }
        )
    }
    .boxed()
}

/// The list page header: the resource's title and the Create entry point
/// (Filament's List page `CreateAction`), a real link so no-JS keeps
/// working, gated on `can_create`. The POST handler enforces the same policy.
///
/// A resource with no record form ([`RecordForm::HAS_FORM`]) has no create
/// route, so its header links to none even when a request-scoped `can_create`
/// allows it and the build-time check could not see it.
fn list_header<'a, R: Resource>(cx: &'a Cx, title: &str, list_path: &str) -> BoxView<'a> {
    let title = title.to_string();
    let create_url = (<R::Form as RecordForm>::HAS_FORM && R::can_create(cx))
        .then(|| create_page_url(list_path));
    let create_label = format!("Create {}", R::label());
    view! {
        cx =>
        tablo_ui::page_header(
            tablo_ui::page_title((title))
            if let Some(url) = create_url {
                tablo_ui::page_actions(
                    <a
                        (crate::resource::runtime_link(cx, &url))
                        class=(tablo_ui::button_variants(
                            tablo_ui::ButtonVariant::Primary,
                            tablo_ui::ButtonSize::Md,
                        ))
                    >
                        icon(data: tablo_ui::icons::PLUS)
                        (create_label)
                    </a>
                )
            }
        )
    }
    .boxed()
}

/// The list page every declared [`Resource`] gets at `{prefix}/{slug}`.
///
/// One generic handler drives all resources: resolve the [`TableState`] from
/// the URL, scope through the tenant-scoped query,
/// apply the table's search/sort/pagination declarations, render through
/// `Resource::table`. The page title is the resource's navigation label.
///
/// The page streams: shell and header go out with the first content, while the
/// table body (toolbar/filter/bulk/pager included) loads inside a `suspense`
/// region that swaps in the skeleton → table without any client-side fetching
/// (GH #98: the skeleton is thead + placeholders only, so chrome pops in with
/// the swap by design).
pub(crate) fn resource_list<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::new(async move {
        gate::<R>(cx)?;
        if !R::can_view_any(cx) {
            return Err(forbidden().into());
        }
        // Ensure the CSRF cookie before streaming starts: streamed
        // children can only read it via current_token.
        crate::csrf::ensure_token(cx);
        let state = TableState::from_cx(cx);
        let table = wire_table_actions::<R>(cx, false);
        let title = R::navigation_label();
        let list_path = list_url(cx, &R::slug());
        if table.is_live_search() {
            return Ok(resource_list_live::<R>(cx, table, state, title, list_path));
        }

        // The skeleton table (`Table::render_skeleton`) streams
        // while the rows load below.
        // The load catches its own errors: post-stream
        // the status line is fixed, so a failed load must render the branded
        // ErrorState inside the region instead of truncating the body.
        // Pre-stream failures (e.g. the skeleton itself) still propagate and
        // map onto the response status. (For children that partially stream
        // before failing, topcoat's `error_boundary` is the replace-in-place
        // seam.)
        //
        // One normalization per request: the skeleton, the load,
        // the render and the retry link all read the state this page parsed,
        // so it normalizes here and every seam below takes the proof (GH
        // #153: the retry link must not echo an unknown `?group_by=`).
        let state = table.normalize_state(&state);
        let skeleton = table.render_skeleton(cx, &state).await?;
        let header = list_header::<R>(cx, &title, &list_path);
        let lazy_rows = ThenView::new(async move {
            let rendered = async {
                let page = load_table_page::<R>(cx, &table, &state).await?;
                table.render_with_state(cx, page, &state, &list_path).await
            };
            match rendered.await {
                Ok(view) => Ok(view),
                Err(error) => Ok(table_error_view::<R>(cx, &state, &error, &list_path, None)),
            }
        });

        Ok(view! {
            cx =>
            tablo_ui::page(
                (header)
                tablo_ui::page_content(
                    suspense(fallback: skeleton, (lazy_rows.boxed()))
                )
            )
        }
        .boxed())
    })))
}

/// Live list page for `Table::live_search` tables: the page owns the
/// interaction signals (`query`, the list's URL query, and `bulk`, the
/// selection) and renders the search and filter bars eagerly above the
/// streamed region while the `table_search` shard invocation fills the table
/// below — one table per response, so rows can never duplicate. Search, sort,
/// filters, and pagination write the query, so they re-render only the
/// invocation output, morphing in place with focus and scroll surviving.
pub(crate) fn resource_list_live<R: Resource>(
    cx: &Cx,
    table: Table<R::Model>,
    state: TableState,
    title: String,
    list_path: String,
) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::new(async move {
        // Seeded with the request's query as written: the shard parses and
        // normalizes it as this page did.
        //
        // Keyed by the list: runtime navigation carries every signal the next
        // page shares with this one, and one call site would otherwise give
        // every resource's list the same ids — one list's search would filter
        // the next.
        let signals = TableState::signals_for(&cx.keyed(list_path.as_str()), &request_query(cx));
        // One normalization per request: the toolbar, the hoisted
        // filter bar, the skeleton, the dialog and the retry link all read
        // the state this page parsed, so it normalizes here and every seam
        // below takes the proof (: no unknown `?group_by=` in a link).
        let state = table.normalize_state(&state);
        let host = if table.search_enabled() {
            Some(
                table
                    .render_live_search_bar(cx, &state, &list_path, &signals)
                    .await?,
            )
        } else {
            None
        };
        // The filter bar is hoisted next to the search host: a
        // `<select>` change re-renders the table, and a control inside the
        // swapped region would lose focus and collapse its popup mid-change.
        let filter_bar = if table.filter_bar_enabled() {
            Some(
                table
                    .render_live_filter_bar(cx, &state, &list_path, &signals)
                    .await?,
            )
        } else {
            None
        };
        // The shard's table renders neither bar (`wire_table_actions(cx,
        // true)`), so neither does the placeholder it replaces.
        // Nor does it draw a card: the page's card holds the bars and the
        // table together.
        let table = table.hide_search().hide_filter_bar().unframed();
        let skeleton = table.render_skeleton(cx, &state).await?;
        // The delete confirmation dialog is not part of the swapped table
        // region: a keystroke starts a new result set and must never carry
        // (or re-open) a dialog, so the live page renders it eagerly once.
        let delete_dialog = table.render_delete_dialog(cx, &state).await?;
        let header = list_header::<R>(cx, &title, &list_path);
        let lazy_rows = ThenView::new(async move {
            // The retry link inside the table writes the same signals the
            // toolbar does, so a bad cursor recovers in place.
            let retry_signals = signals.clone();
            let rendered = table.render_live_invocation(cx, &list_path, signals).await;
            match rendered {
                Ok(view) => Ok(view),
                Err(error) => Ok(table_error_view::<R>(
                    cx,
                    &state,
                    &error,
                    &list_path,
                    Some(&retry_signals),
                )),
            }
        });

        Ok(view! {
            cx =>
            tablo_ui::page(
                (header)
                tablo_ui::page_content(
                    // The page draws the table's card (`TABLE_CARD_CLASS`) so the
                    // hoisted bars share it with the table they drive.
                    <div class=(TABLE_CARD_CLASS)>
                        if let Some(host) = host {
                            (host)
                        }
                        if let Some(bar) = filter_bar {
                            (bar)
                        }
                        suspense(fallback: skeleton, (lazy_rows.boxed()))
                    </div>
                    if let Some(dialog) = delete_dialog {
                        (dialog)
                    }
                )
            )
        })
    })))
}

/// Load `R`'s list page for `state` from the tenant-scoped
/// [`scoped_query`](crate::resource::scoped_query) — the data-loading half of
/// [`resource_list`] and the live shard, kept separate so the page shell can
/// stream before it. [`TablePage::load`] applies the table's declaration and
/// the relations its columns include.
pub(crate) async fn load_table_page<R: Resource>(
    cx: &Cx,
    table: &Table<R::Model>,
    state: &TableState,
) -> Result<TablePage<R::Model>> {
    TablePage::load(cx, table, crate::resource::scoped_query::<R>(cx)?, state).await
}

/// [`load_table_page`] over the rows `scope` also admits: a relation table's
/// rows that belong to its owner.
pub(crate) async fn load_scoped_page<R: Resource>(
    cx: &Cx,
    table: &Table<R::Model>,
    state: &TableState,
    scope: toasty::stmt::Expr<bool>,
) -> Result<TablePage<R::Model>> {
    let query = crate::resource::scoped_query::<R>(cx)?.filter(scope);
    TablePage::load(cx, table, query, state).await
}

#[cfg(test)]
mod tests;
