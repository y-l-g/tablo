//! Serves each resource's list page and its live-search host.

use std::sync::Arc;

use topcoat::{
    Result,
    context::Cx,
    icon::icon,
    router::{Body, error::forbidden},
    runtime::Event,
    view::{BoxView, ViewExt, attributes, internal::ThenView, suspense, view},
};

use super::{gate::gate, search::list_search_invocation};
use crate::{
    form::RecordForm,
    policy::Ability,
    resource::{Mounted, Resource},
    table::{
        RowActions, TABLE_CARD_CLASS, Table, TableAction, TableChrome, TablePage, TableSignals,
        TableState, create_page_url, request_query,
    },
    topcoat_compat::async_page,
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
pub(crate) fn declared_chrome<R: Resource>(cx: &Cx, resource: &Mounted<R>) -> TableChrome {
    TableChrome {
        delete: resource.can(cx, Ability::DeleteAny),
        edit: <R::Form as RecordForm>::HAS_FORM,
        view: resource.viewed(),
        actions: true,
    }
}

/// Wires `R`'s delete, bulk, edit, view, and custom actions onto the table; `live` selects the
/// shard variant.
pub(crate) fn wire_table_actions<R: Resource>(
    cx: &Cx,
    resource: &Arc<Mounted<R>>,
    live: bool,
) -> Table<R::Model> {
    wire_table(cx, resource, live, declared_chrome(cx, resource))
}

/// Wires `chrome`'s affordances onto `R`'s table.
pub(crate) fn wire_table<R: Resource>(
    cx: &Cx,
    resource: &Arc<Mounted<R>>,
    live: bool,
    chrome: TableChrome,
) -> Table<R::Model> {
    let mut table = resource.table.clone();
    if live {
        table = table.hide_search().hide_filter_bar().unframed();
    }
    let (policy_cx, policy) = (cx.clone(), Arc::clone(resource));
    table = table.row_actions(move |record| {
        // Every route pairs its predicate with `View`.
        let view = policy.can(&policy_cx, Ability::View(record));
        RowActions {
            view,
            edit: view && policy.can(&policy_cx, Ability::Update(record)),
            delete: view && policy.can(&policy_cx, Ability::Delete(record)),
        }
    });
    if chrome.delete {
        table = table
            .with_delete(resource.url.clone())
            .with_bulk_delete(true);
    }
    if chrome.edit {
        table = table.with_edit(resource.url.clone());
    }
    if chrome.view {
        table = table.with_view(resource.url.clone());
    }
    if chrome.actions {
        table = wire_custom_actions(cx, resource, table);
    }
    table
}

/// Attaches `R`'s custom actions to `table`, each gated per record by `View` and the action's
/// `can_run`.
fn wire_custom_actions<R: Resource>(
    cx: &Cx,
    resource: &Arc<Mounted<R>>,
    table: Table<R::Model>,
) -> Table<R::Model> {
    if resource.actions.entries().is_empty() {
        return table;
    }
    let wired = resource
        .actions
        .entries()
        .iter()
        .map(|action| {
            let can_run = action.can_run;
            let (policy_cx, policy) = (cx.clone(), Arc::clone(resource));
            TableAction {
                name: action.name,
                label: (action.label)(cx),
                row: action.row,
                bulk: action.bulk,
                allowed: Arc::new(move |record: &R::Model| {
                    policy.can(&policy_cx, Ability::View(record)) && can_run(&policy_cx, record)
                }),
            }
        })
        .collect();
    table.with_custom_actions(resource.url.clone(), wired)
}

/// Renders the branded in-region table failure with a cursor-aware retry link.
pub(crate) fn table_error_view<'a, R: Resource>(
    cx: &'a Cx,
    resource: &Mounted<R>,
    state: &TableState,
    error: &topcoat::Error,
    path: &str,
    signals: Option<&TableSignals>,
) -> BoxView<'a> {
    tracing::error!(resource = resource.slug, error = %error, "table load failed");
    let title = format!("Couldn't load {}", resource.plural_label);
    let retry_url = retry_url_for_error(state, error, path);
    let action: BoxView<'a> = match signals {
        Some(signals) => {
            // Keyed by the list, like the table's own signals.
            let attempt = topcoat::runtime::signal(&cx.keyed(path), || 0u64);
            let cursor_error = crate::error::TabloError::is_cursor(error);
            let attrs = if cursor_error {
                let query = signals.query.clone();
                let next = crate::table::query_of(&retry_url).to_string();
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
            title: title,
            detail: "Something went wrong while loading the records.",
            action: Some(action.into()),
            attrs: attributes! { role="alert" }
        )
    }
    .boxed()
}

/// Renders the list page header with the resource's title and its Create link.
fn list_header<'a, R: Resource>(cx: &'a Cx, resource: &Mounted<R>) -> BoxView<'a> {
    let title = resource.plural_label.clone();
    let create_url = (<R::Form as RecordForm>::HAS_FORM && resource.can(cx, Ability::Create))
        .then(|| create_page_url(&resource.url));
    let create_label = format!("Create {}", resource.label);
    view! {
        cx =>
        tablo_ui::page_header(
            tablo_ui::page_title((title))
            if let Some(url) = create_url {
                tablo_ui::page_actions(
                    <a
                        (crate::navigation::runtime_link(cx, &url))
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
    async_page(async move {
        let resource = gate::<R>(cx)?;
        if !resource.can(cx, Ability::ViewAny) {
            return Err(forbidden().into());
        }
        crate::csrf::ensure_token(cx);
        let state = TableState::from_cx(cx);
        let table = wire_table_actions(cx, &resource, false);
        if table.is_live_search() {
            return Ok(resource_list_live(cx, resource, table, state));
        }

        // Normalizes once per request (GH #153).
        let state = table.normalize_state(&state);
        let skeleton = table.render_skeleton(cx, &state).await?;
        let header = list_header(cx, &resource);
        let lazy_rows = ThenView::new(async move {
            let list_path = &resource.url;
            let rendered = async {
                let page = load_table_page(cx, &resource, &table, &state).await?;
                table.render_with_state(cx, page, &state, list_path).await
            };
            match rendered.await {
                Ok(view) => Ok(view),
                Err(error) => Ok(table_error_view(
                    cx, &resource, &state, &error, list_path, None,
                )),
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
    })
}

/// Serves the live list page for `Table::live_search` tables.
pub(crate) fn resource_list_live<R: Resource>(
    cx: &Cx,
    resource: Arc<Mounted<R>>,
    table: Table<R::Model>,
    state: TableState,
) -> BoxView<'_> {
    async_page(async move {
        let list_path = resource.url.clone();
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
        let header = list_header(cx, &resource);
        let lazy_rows = ThenView::new(async move {
            Ok::<_, topcoat::Error>(list_search_invocation(cx, &list_path, signals))
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
    })
}

/// Loads `R`'s list page for `state` from the tenant-scoped query.
pub(crate) async fn load_table_page<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    table: &Table<R::Model>,
    state: &TableState,
) -> Result<TablePage<R::Model>> {
    TablePage::load(cx, table, resource.scoped_query(cx)?, state).await
}

/// Loads [`load_table_page`] over the rows `scope` admits.
pub(crate) async fn load_scoped_page<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    table: &Table<R::Model>,
    state: &TableState,
    scope: toasty::stmt::Expr<bool>,
) -> Result<TablePage<R::Model>> {
    let query = resource.scoped_query(cx)?.filter(scope);
    TablePage::load(cx, table, query, state).await
}

#[cfg(test)]
mod tests;
