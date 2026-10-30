//! Serves each resource's list page, and renders the [`WiredTable`] it and a related table show.

mod render;
mod shown;
mod signals;
mod wiring;

use std::sync::Arc;

use topcoat::{
    Result,
    context::Cx,
    icon::icon,
    router::{
        Body,
        error::{forbidden, see_other},
        request::original_method,
    },
    view::{BoxView, SuspenseMode, ViewExt, attributes, internal::ThenView, suspense, view},
};

pub(crate) use self::{
    render::{TAB_ACTIVE_CLASS, TAB_CLASS, TAB_IDLE_CLASS},
    shown::ShownList,
    signals::TableSignals,
    wiring::{RowActions, TableAction, TableChrome},
};
pub use self::{shown::list_query, wiring::WiredTable};
use super::{
    bar::{ActionBar, header_bar},
    forms::{parse_form_body, truthy},
    gate::{gate, landing_url},
    url::{columns_url, create_page_url, export_url, import_url},
};
use crate::{
    policy::Ability,
    resource::{Mounted, Places, Resource},
    table::{ColumnChoice, Table, TablePage, TableState},
    topcoat_compat::async_page,
};

/// The field each checked box of the column picker posts a column's name under.
pub(crate) const COLUMN_FIELD: &str = "column";

/// The field the column picker's Reset posts, restoring every column's default.
pub(crate) const RESET_FIELD: &str = "reset";

/// Returns the retry link for a failed table load, dropping pagination for cursor failures and
/// keeping it otherwise.
pub(crate) fn retry_url_for_error(
    state: &TableState,
    error: &topcoat::Error,
    path: &str,
) -> String {
    if crate::error::TabloError::is_cursor(error) {
        state.first_page(path)
    } else {
        state.list_url(path)
    }
}

/// Derives the action chrome a resource's declarations imply.
pub(crate) fn declared_chrome<R: Resource>(cx: &Cx, resource: &Mounted<R>) -> TableChrome {
    TableChrome {
        delete: resource.can(cx, Ability::DeleteAny),
        edit: resource.pages.edit,
        view: resource.viewed(),
        actions: true,
    }
}

/// `table` showing the columns the request's user chose for `R`'s list, its picker posting to the
/// list's column route while a signed-in user may keep a choice.
pub(crate) async fn choose_columns<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    table: WiredTable<R::Model>,
) -> WiredTable<R::Model> {
    if !table.has_toggleable_columns() {
        return table;
    }
    let choice = crate::auth::load_choice(cx, &resource.slug).await;
    let url = crate::auth::signed_in(cx).then(|| columns_url(&resource.url));
    table.with_column_choice(choice, url)
}

/// The column picker's POST, `{list}/-/columns`: stores the signed-in user's choice of `R`'s
/// toggleable columns, the boxes it checks under [`COLUMN_FIELD`], or the defaults with
/// [`RESET_FIELD`], and lands back where the picker was.
pub(crate) fn resource_columns<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        if !resource.can(cx, Ability::ViewAny) {
            return Err(forbidden().into());
        }
        let parts = parse_form_body(cx, body).await?;
        crate::csrf::verify(cx, parts.last(crate::csrf::FIELD_NAME))?;
        let choice = if parts.last(RESET_FIELD).is_some_and(truthy) {
            ColumnChoice::default()
        } else {
            let checked: Vec<&str> = parts
                .pairs
                .iter()
                .filter(|(name, _)| name == COLUMN_FIELD)
                .map(|(_, value)| value.as_str())
                .collect();
            ColumnChoice::checked(resource.table_for(cx).declared_columns(), &checked)
        };
        crate::auth::save_choice(cx, &resource.slug, &choice).await?;
        Err::<(), _>(see_other(landing_url(cx, &resource.url)).into())
    })
}

/// Returns the panel's list table for `R`, with the request's row actions wired on, for a page
/// that owns its table; render it over the page's own query with
/// [`WiredTable::render`](WiredTable::render).
///
/// # Errors
///
/// A declaration error when the context's panel does not mount `R`.
pub fn wired_table<R: Resource>(
    cx: &topcoat::context::Cx,
) -> topcoat::Result<WiredTable<R::Model>> {
    let resource = crate::resource::require_mounted::<R>(cx)?;
    Ok(wire_table_actions(cx, &resource))
}

/// Wires `R`'s delete, bulk, edit, view, and custom actions onto the table.
pub(crate) fn wire_table_actions<R: Resource>(
    cx: &Cx,
    resource: &Arc<Mounted<R>>,
) -> WiredTable<R::Model> {
    wire_table(cx, resource, declared_chrome(cx, resource))
}

/// Wires `chrome`'s affordances onto `R`'s table.
pub(crate) fn wire_table<R: Resource>(
    cx: &Cx,
    resource: &Arc<Mounted<R>>,
    chrome: TableChrome,
) -> WiredTable<R::Model> {
    wire_table_over(cx, resource, resource.table_for(cx), chrome)
}

/// Wires `chrome`'s affordances onto `table`, a narrowing of `R`'s table for the request.
pub(crate) fn wire_table_over<R: Resource>(
    cx: &Cx,
    resource: &Arc<Mounted<R>>,
    table: Arc<Table<R::Model>>,
    chrome: TableChrome,
) -> WiredTable<R::Model> {
    let mut table = WiredTable::new(table);
    let (policy_cx, policy) = (cx.clone(), Arc::clone(resource));
    table = table.row_actions(move |record| {
        // Every route pairs its predicate with `View`; a trashed record is restored before it is
        // edited or deleted again.
        let view = policy.can(&policy_cx, Ability::View(record));
        let live = view && !policy.is_trashed(record);
        RowActions {
            view,
            edit: live && policy.can(&policy_cx, Ability::Update(record)),
            delete: live && policy.can(&policy_cx, Ability::Delete(record)),
        }
    });
    if chrome.delete {
        table = table
            .with_delete(resource.url.clone())
            .with_bulk_delete(true);
    }
    if chrome.edit {
        // A cell posts its field to the edit route, which refuses a key the user may not change.
        // A widget's table or a custom page may render it before anything else set the token.
        crate::csrf::ensure_token(cx);
        let mask = resource.form_mask(cx);
        table = table.with_edit(resource.url.clone()).editing_cells(
            move |key| mask.writes(key).is_none(),
            <R::Model as crate::ModelColumns>::version,
        );
    }
    if chrome.view {
        table = table.with_view(resource.url.clone());
    }
    if chrome.actions {
        table = wire_custom_actions(cx, resource, table);
    }
    table
}

/// Attaches the custom actions of `R` its policy allows `RunAny`, each gated per record by `View`
/// and the entry's per-record check: `Run` and the action's `can_run`, on a live record; a
/// soft-deleting resource's restore and force delete, on a trashed one.
fn wire_custom_actions<R: Resource>(
    cx: &Cx,
    resource: &Arc<Mounted<R>>,
    table: WiredTable<R::Model>,
) -> WiredTable<R::Model> {
    let wired: Vec<_> = resource
        .actions
        .entries()
        .iter()
        .filter(|action| resource.can(cx, action.resource_wide))
        .map(|action| {
            let action = *action;
            let (policy_cx, policy) = (cx.clone(), Arc::clone(resource));
            let (reason_cx, reasons) = (cx.clone(), Arc::clone(resource));
            TableAction {
                name: action.name,
                label: (action.label)().localize(cx),
                row: action.places.contains(Places::ROW),
                bulk: action.places.contains(Places::BULK),
                confirm: action.confirm,
                prompt: (action.prompt)(false).localize(cx),
                bulk_prompt: (action.prompt)(true).localize(cx),
                icon: (action.icon)(),
                destructive: action.destructive,
                input: action.input.takes_input.then_some(action.input.schema),
                trashed: action.trashed,
                allowed: Arc::new(move |record: &R::Model| {
                    policy.can(&policy_cx, Ability::View(record))
                        && action.allows(&policy, &policy_cx, record)
                }),
                disabled: Arc::new(move |record: &R::Model| {
                    if !reasons.can(&reason_cx, Ability::View(record)) {
                        return None;
                    }
                    let reason = action.disabled_on(&reasons, &reason_cx, record)?;
                    Some(reason.localize(&reason_cx))
                }),
            }
        })
        .collect();
    // A table wired with no action would still render the write form and its dialog.
    if wired.is_empty() {
        return table;
    }
    table.with_custom_actions(resource.url.clone(), wired)
}

/// Renders the branded in-region table failure of the resource at `slug` with a cursor-aware
/// retry link.
pub(crate) fn table_error_view<'a>(
    cx: &'a Cx,
    slug: &str,
    plural_label: &crate::Text,
    state: &TableState,
    error: &topcoat::Error,
    path: &str,
) -> BoxView<'a> {
    tracing::error!(resource = slug, error = %error, "table load failed");
    let title = crate::t!(cx, "tablo-list-failed", label = plural_label);
    let retry_url = retry_url_for_error(state, error, path);
    view! {
        cx =>
        tablo_ui::error_state(
            title: title,
            detail: crate::t!(cx, "tablo-list-failed-description"),
            action: Some(
                view! {
                    cx =>
                    <a (crate::navigation::runtime_link(cx, &retry_url))>
                        (crate::t!(cx, "tablo-list-retry"))
                    </a>
                }
                    .boxed()
                    .into(),
            ),
            attrs: attributes! { role="alert" }
        )
    }
    .boxed()
}

/// Renders the list page header titled `plural_label`, with the header `actions`, the CSV export
/// link to `export_url`, an Import link to `import_url` and a Create link to `create_url`.
fn list_header<'a>(
    cx: &'a Cx,
    plural_label: &crate::Text,
    label: &crate::Text,
    actions: ActionBar,
    create_url: Option<String>,
    import_url: Option<String>,
    export_url: String,
) -> BoxView<'a> {
    let actions = actions.render(cx);
    let title = plural_label.clone();
    let create_label = crate::t!(cx, "tablo-form-create-title", label = label);
    let outline =
        tablo_ui::button_variants(tablo_ui::ButtonVariant::Outline, tablo_ui::ButtonSize::Md);
    view! {
        cx =>
        tablo_ui::page_header(
            tablo_ui::page_title((title))
            tablo_ui::page_actions(
                (actions)
                <a href=(export_url) class=(outline.clone())>
                    icon(data: tablo_ui::icons::DOWNLOAD)
                    (crate::t!(cx, "tablo-list-export"))
                </a>
                if let Some(url) = import_url {
                    <a (crate::navigation::runtime_link(cx, &url)) class=(outline)>
                        icon(data: tablo_ui::icons::UPLOAD)
                        (crate::t!(cx, "tablo-list-import"))
                    </a>
                }
                if let Some(url) = create_url {
                    <a
                        id="tablo-header-create"
                        (crate::navigation::runtime_link(cx, &url))
                        class=(tablo_ui::button_variants(
                            tablo_ui::ButtonVariant::Primary,
                            tablo_ui::ButtonSize::Md,
                        ))
                    >
                        icon(data: tablo_ui::icons::PLUS)
                        (create_label)
                    </a>
                }
            )
        )
    }
    .boxed()
}

/// Serves the list page every declared [`Resource`] gets at `{prefix}/{slug}`.
///
/// The page reads the table's state from its signals, so a change in the browser reruns it in
/// place; a rerun waits for the rows instead of flashing the skeleton.
pub(crate) fn resource_list<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        if !resource.can(cx, Ability::ViewAny) {
            return Err(forbidden().into());
        }
        crate::csrf::ensure_token(cx);
        // Normalizes once per request.
        let table = choose_columns(cx, &resource, wire_table_actions(cx, &resource)).await;
        let (signals, state) = table.browser_state(cx);
        ShownList::record::<R>(cx, &state);
        // The bulk bar offers what the rows the trash control shows can take.
        let table = table
            .taking(resource.list_trashed(&state))
            .selecting_matching();
        // A write lands back on the list as the reader left it.
        let back = state.without_cursor(&resource.url);
        let mut actions = header_bar(cx, &resource.url, &resource.header_actions, |action| {
            resource.can(
                cx,
                Ability::RunHeader {
                    action: action.name,
                },
            )
        });
        let table = if back == resource.url {
            table
        } else {
            actions = actions.returning_to(&back);
            table.returning_to(back)
        };
        let skeleton = table.render_skeleton(cx, &state).await?;
        let creates = resource.pages.create && resource.can_create(cx);
        let create_url = creates.then(|| create_page_url(&resource.url));
        let import_url = (creates && resource.imports).then(|| import_url(&resource.url));
        let export_url = state.first_page(&export_url(&resource.url));
        let header = list_header(
            cx,
            &resource.plural_label,
            &resource.label,
            actions,
            create_url,
            import_url,
            export_url,
        );
        let widgets = super::widgets::rendered_grid(cx, &resource.list_widgets).await?;
        let mode = rerun_suspense_mode(cx);
        let lazy_rows = ThenView::new(async move {
            let list_path = &resource.url;
            let rendered = async {
                let page = load_table_page(cx, &resource, &table, &state).await?;
                table
                    .render_page(cx, page, &state, list_path, &signals)
                    .await
            };
            match rendered.await {
                Ok(view) => Ok(view),
                Err(error) => Ok(table_error_view(
                    cx,
                    &resource.slug,
                    &resource.plural_label,
                    &state,
                    &error,
                    list_path,
                )),
            }
        });
        Ok(list_page(
            cx,
            ListPage {
                header,
                widgets,
                skeleton,
                mode,
                rows: lazy_rows.boxed(),
            },
        ))
    })
}

/// What the list page shows, resolved from the resource.
struct ListPage<'a> {
    header: BoxView<'a>,
    /// The [`ResourceDef::list_widget`](crate::ResourceDef::list_widget) grid.
    widgets: BoxView<'a>,
    /// What shows while the rows load.
    skeleton: BoxView<'a>,
    mode: SuspenseMode,
    rows: BoxView<'a>,
}

/// Renders the list page: `header` over the widgets and the `rows`, behind `skeleton` while they
/// load.
fn list_page<'a>(cx: &'a Cx, page: ListPage<'a>) -> BoxView<'a> {
    let ListPage {
        header,
        widgets,
        skeleton,
        mode,
        rows,
    } = page;
    view! {
        cx =>
        tablo_ui::page(
            (header)
            tablo_ui::page_content(
                <div class="flex flex-col gap-4">
                    (widgets)
                    suspense(fallback: skeleton, mode: mode, (rows))
                </div>
            )
        )
    }
    .boxed()
}

/// How a list's `suspense` loads: streamed behind its skeleton on the first render, and waited
/// for on a rerun, which updates the page in place.
fn rerun_suspense_mode(cx: &Cx) -> SuspenseMode {
    if original_method(cx) == http::Method::POST {
        SuspenseMode::Wait
    } else {
        SuspenseMode::Stream
    }
}

/// Loads `R`'s list page for `state` from the tenant-scoped query, admitting the records its
/// trash control selects.
pub(crate) async fn load_table_page<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    table: &Table<R::Model>,
    state: &TableState,
) -> Result<TablePage<R::Model>> {
    TablePage::load(cx, table, resource.list_query(cx, state)?, state).await
}

/// Loads [`load_table_page`] over the rows `scope` admits.
pub(crate) async fn load_scoped_page<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    table: &Table<R::Model>,
    state: &TableState,
    scope: toasty::stmt::Expr<bool>,
) -> Result<TablePage<R::Model>> {
    let query = resource.list_query(cx, state)?.filter(scope);
    TablePage::load(cx, table, query, state).await
}

#[cfg(test)]
mod tests;
