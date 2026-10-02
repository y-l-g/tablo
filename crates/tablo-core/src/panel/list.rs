//! Serves each resource's list page and its live-search host.

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
    policy::{Ability, can},
    resource::{
        Resource, RowActions, TABLE_CARD_CLASS, Table, TableAction, TableChrome, TablePage,
        TableSignals, TableState, create_page_url, declared, request_query,
    },
};

/// Returns the retry link for a failed table load, dropping pagination for cursor failures and
/// keeping it otherwise.
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

/// Derives the action chrome a resource's declarations imply.
pub(crate) fn declared_chrome<R: Resource>(cx: &Cx) -> TableChrome {
    TableChrome {
        delete: can::<R>(cx, Ability::DeleteAny),
        edit: <R::Form as RecordForm>::HAS_FORM,
        view: declared::<R>(cx).viewed(),
        actions: true,
    }
}

/// Wires `R`'s delete, bulk, edit, view, and custom actions onto the table; `live` selects the
/// shard variant.
pub(crate) fn wire_table_actions<R: Resource>(cx: &Cx, live: bool) -> Table<R::Model> {
    wire_table::<R>(cx, live, declared_chrome::<R>(cx))
}

/// Wires `chrome`'s affordances onto `R`'s table.
pub(crate) fn wire_table<R: Resource>(cx: &Cx, live: bool, chrome: TableChrome) -> Table<R::Model> {
    let mut table = declared::<R>(cx).table.clone();
    if live {
        table = table.hide_search().hide_filter_bar().unframed();
    }
    let policy_cx = cx.clone();
    table = table.row_actions(move |record| {
        // Every route pairs its predicate with `View`.
        let view = can::<R>(&policy_cx, Ability::View(record));
        RowActions {
            view,
            edit: view && can::<R>(&policy_cx, Ability::Update(record)),
            delete: view && can::<R>(&policy_cx, Ability::Delete(record)),
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

/// Attaches `R`'s custom actions to `table`, each gated per record by `View` and the action's
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
                    can::<R>(&policy_cx, Ability::View(record)) && can_run(&policy_cx, record)
                }),
            }
        })
        .collect();
    table.with_custom_actions(list_url(cx, &R::slug()), wired)
}

/// Renders the branded in-region table failure with a cursor-aware retry link.
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
            // Keyed by the list, like the table's own signals.
            let attempt = topcoat::runtime::signal(&cx.keyed(path), || 0u64);
            let cursor_error = crate::error::TabloError::is_cursor(error);
            let attrs = if cursor_error {
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

/// Renders the list page header with the resource's title and its Create link.
fn list_header<'a, R: Resource>(cx: &'a Cx, title: &str, list_path: &str) -> BoxView<'a> {
    let title = title.to_string();
    let create_url = (<R::Form as RecordForm>::HAS_FORM && can::<R>(cx, Ability::Create))
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

/// Serves the list page every declared [`Resource`] gets at `{prefix}/{slug}`.
pub(crate) fn resource_list<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::new(async move {
        gate::<R>(cx)?;
        if !can::<R>(cx, Ability::ViewAny) {
            return Err(forbidden().into());
        }
        crate::csrf::ensure_token(cx);
        let state = TableState::from_cx(cx);
        let table = wire_table_actions::<R>(cx, false);
        let title = R::navigation_label();
        let list_path = list_url(cx, &R::slug());
        if table.is_live_search() {
            return Ok(resource_list_live::<R>(cx, table, state, title, list_path));
        }

        // Normalizes once per request (GH #153).
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

/// Serves the live list page for `Table::live_search` tables.
pub(crate) fn resource_list_live<R: Resource>(
    cx: &Cx,
    table: Table<R::Model>,
    state: TableState,
    title: String,
    list_path: String,
) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::new(async move {
        // Keyed by the list so one list's search never filters the next.
        let signals = TableState::signals_for(&cx.keyed(list_path.as_str()), &request_query(cx));
        // Normalizes once per request.
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
        let filter_bar = if table.filter_bar_enabled() {
            Some(
                table
                    .render_live_filter_bar(cx, &state, &list_path, &signals)
                    .await?,
            )
        } else {
            None
        };
        let table = table.hide_search().hide_filter_bar().unframed();
        let skeleton = table.render_skeleton(cx, &state).await?;
        let delete_dialog = table.render_delete_dialog(cx, &state).await?;
        let header = list_header::<R>(cx, &title, &list_path);
        let lazy_rows = ThenView::new(async move {
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

/// Loads `R`'s list page for `state` from the tenant-scoped query.
pub(crate) async fn load_table_page<R: Resource>(
    cx: &Cx,
    table: &Table<R::Model>,
    state: &TableState,
) -> Result<TablePage<R::Model>> {
    TablePage::load(cx, table, crate::resource::scoped_query::<R>(cx)?, state).await
}

/// Loads [`load_table_page`] over the rows `scope` admits.
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
