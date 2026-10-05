//! Renders each [`Relation`](crate::resource::Relation) of a record page's resource as the related
//! resource's table narrowed to the record.

use std::{future::Future, pin::Pin, sync::Arc};

use toasty::stmt::Expr;
use topcoat::{
    Result,
    context::Cx,
    icon::icon,
    router::error::{bad_request, forbidden},
    view::{BoxView, ViewExt, internal::ThenView, suspense, view},
};

use super::{
    gate::{enforce_tenant, gate, panel_prefix},
    list::{declared_chrome, load_scoped_page, table_error_view, wire_table},
    search::{RelationRequest, relation_search_invocation},
    state::current,
};
use crate::{
    form::RecordForm,
    navigation::runtime_link,
    policy::Ability,
    resource::{Mounted, Resource, mounted},
    table::{
        RETURN_PARAM, TABLE_CARD_CLASS, Table, TableChrome, TableSignals, TableState,
        create_page_url, request_query,
    },
    topcoat_compat::async_page,
};

/// A registered resource as the child of other resources' relations.
pub(crate) struct Child {
    pub(crate) slug: String,
    pub(crate) plural_label: String,
    /// Renders the child's table narrowed to one owner.
    pub(crate) render: for<'a> fn(&'a Cx, BoundRelation) -> BoxView<'a>,
}

/// A child's live-search loader over the rows a request's owner scope admits; `None` when the
/// request names no owner.
pub(crate) type ChildSearchFn =
    for<'a> fn(
        &'a Cx,
        RelationRequest,
        TableSignals,
        Option<Expr<bool>>,
    ) -> Pin<Box<dyn Future<Output = Result<BoxView<'a>>> + Send + 'a>>;

/// A relation resolved against one owner record.
pub(crate) struct BoundRelation {
    /// The slug of the resource that owns the record.
    pub(crate) parent: String,
    /// The child's slug: the prefix of the table's URL parameters.
    pub(crate) key: String,
    /// The section title.
    pub(crate) label: String,
    /// The child's rows that belong to the owner.
    pub(crate) scope: Expr<bool>,
    /// The child's form key for the owner, and the owner's value for it.
    pub(crate) seed: (String, String),
    /// The path of the page that renders the table.
    pub(crate) page: String,
    /// Whether the page only shows the rows.
    pub(crate) read_only: bool,
}

/// Renders the relation tables of `resource`'s record `owner`.
pub(crate) fn render_relations<'a, R: Resource>(
    cx: &'a Cx,
    resource: &Mounted<R>,
    owner: &R::Model,
    read_only: bool,
) -> Vec<BoxView<'a>> {
    let Some(panel) = current(cx) else {
        return Vec::new();
    };
    let page = topcoat::router::request::uri(cx).path().to_string();
    resource
        .relations
        .iter()
        .filter_map(|relation| {
            let child = panel.children.get(&relation.child)?;
            let (scope, value) = relation.bind(owner);
            Some((child.render)(
                cx,
                BoundRelation {
                    parent: resource.slug.clone(),
                    key: child.slug.clone(),
                    label: relation
                        .label
                        .clone()
                        .unwrap_or_else(|| child.plural_label.clone()),
                    scope,
                    seed: (relation.foreign_key.clone(), value),
                    page: page.clone(),
                    read_only,
                },
            ))
        })
        .collect()
}

/// Derives one relation table's action chrome, view-only on a read-only page.
pub(crate) fn relation_chrome<C: Resource>(
    cx: &Cx,
    resource: &Mounted<C>,
    read_only: bool,
) -> TableChrome {
    let declared = declared_chrome(cx, resource);
    if read_only {
        TableChrome {
            view: declared.view,
            ..TableChrome::default()
        }
    } else {
        declared
    }
}

/// Renders one relation's section as `C`'s list table over the rows the owner holds.
pub(crate) fn relation_table<C: Resource>(cx: &Cx, relation: BoundRelation) -> BoxView<'_> {
    async_page(async move {
        let Some(resource) = mounted::<C>(cx) else {
            return Ok(().boxed());
        };
        if enforce_tenant(cx, &resource).is_err() || !resource.can(cx, Ability::ViewAny) {
            return Ok(().boxed());
        }
        let chrome = relation_chrome(cx, &resource, relation.read_only);
        let table = wire_table(cx, &resource, false, chrome);
        let state = table.normalize_state(&TableState::from_cx_prefixed(cx, &relation.key));
        let BoundRelation {
            ref seed,
            ref page,
            read_only,
            ..
        } = relation;
        let table = table.returning_to(state.list_url(page));
        let create_url =
            (!read_only && <C::Form as RecordForm>::HAS_FORM && resource.can(cx, Ability::Create))
                .then(|| {
                    let query = form_urlencoded::Serializer::new(String::new())
                        .append_pair(&seed.0, &seed.1)
                        .append_pair(RETURN_PARAM, &state.list_url(page))
                        .finish();
                    format!("{}?{query}", create_page_url(&resource.url))
                });
        if table.is_live_search() {
            return relation_table_live(cx, resource, table, state, relation, create_url).await;
        }
        let BoundRelation {
            key,
            label,
            scope,
            page,
            ..
        } = relation;
        let body = match load_scoped_page(cx, &resource, &table, &state, scope).await {
            Ok(rows) => table.render_with_state(cx, rows, &state, &page).await?,
            Err(error) => table_error_view(cx, &resource, &state, &error, &page, None),
        };
        let header = relation_header(cx, &resource, label, create_url);
        Ok(view! {
            cx =>
            <section class="flex flex-col gap-3" data-relation=(key)>
                (header)
                (body)
            </section>
        }
        .boxed())
    })
}

/// Renders a live-search relation's section with its bars hoisted above the streamed region.
async fn relation_table_live<C: Resource>(
    cx: &Cx,
    resource: Arc<Mounted<C>>,
    table: Table<C::Model>,
    state: TableState,
    relation: BoundRelation,
    create_url: Option<String>,
) -> Result<BoxView<'_>>
where
    C::Model: toasty::schema::Model + Send + Sync + 'static,
{
    let BoundRelation {
        parent,
        key,
        label,
        seed,
        page,
        read_only,
        scope: _,
    } = relation;
    // Keyed by page and relation key so one relation's search never filters the next.
    let signals = TableState::signals_for(
        &cx.keyed(format!("{page}#{key}").as_str()),
        &request_query(cx),
    );
    let host = if table.search_enabled() {
        Some(
            table
                .render_live_search_bar(cx, &state, &page, &signals)
                .await?,
        )
    } else {
        None
    };
    let filter_bar = if table.filter_bar_enabled() {
        Some(
            table
                .render_live_filter_bar(cx, &state, &page, &signals)
                .await?,
        )
    } else {
        None
    };
    let table = table.hide_search().hide_filter_bar().unframed();
    let skeleton = table.render_skeleton(cx, &state).await?;
    let delete_dialog = table.render_delete_dialog(cx, &state).await?;
    let header = relation_header(cx, &resource, label, create_url);
    let invocation_key = key.clone();
    let lazy_rows = ThenView::new(async move {
        Ok::<_, topcoat::Error>(relation_search_invocation(
            cx,
            &parent,
            &invocation_key,
            RelationRequest {
                seed: seed.1,
                page,
                read_only,
            },
            signals,
        ))
    });

    Ok(view! {
        cx =>
        <section class="flex flex-col gap-3" data-relation=(key)>
            (header)
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
        </section>
    }
    .boxed())
}

/// Renders a relation section's heading row with its create-child link.
fn relation_header<'a, C: Resource>(
    cx: &'a Cx,
    resource: &Mounted<C>,
    label: String,
    create_url: Option<String>,
) -> BoxView<'a> {
    let create_label = format!("New {}", resource.label);
    view! {
        cx =>
        <div class="flex items-center justify-between gap-4">
            <h2 class="text-lg font-semibold tracking-tight text-foreground">
                (label)
            </h2>
            if let Some(url) = create_url {
                <a
                    (runtime_link(cx, &url))
                    class=(tablo_ui::button_variants(
                        tablo_ui::ButtonVariant::Outline,
                        tablo_ui::ButtonSize::Sm,
                    ))
                >
                    icon(data: tablo_ui::icons::PLUS)
                    (create_label)
                </a>
            }
        </div>
    }
    .boxed()
}

/// Serves a live relation table's search: `C`'s table over the rows `scope` admits.
pub(crate) fn relation_search<C: Resource>(
    cx: &Cx,
    request: RelationRequest,
    signals: TableSignals,
    scope: Option<Expr<bool>>,
) -> Pin<Box<dyn Future<Output = Result<BoxView<'_>>> + Send + '_>> {
    Box::pin(async move {
        let resource = gate::<C>(cx)?;
        if !resource.can(cx, Ability::ViewAny) {
            return Err(forbidden().into());
        }
        let prefix = panel_prefix(cx);
        if request
            .page
            .strip_prefix(prefix.as_str())
            .is_none_or(|rest| !rest.is_empty() && !rest.starts_with(['/', '?']))
        {
            return Err(bad_request("unknown relation page").into());
        }
        let scope = scope.ok_or_else(|| bad_request("unknown relation owner"))?;
        let chrome = relation_chrome(cx, &resource, request.read_only);
        let table = wire_table(cx, &resource, true, chrome);
        let mut state = TableState::from_query_prefixed(&signals.query.get(), &resource.slug);
        state.delete = None;
        state.open = None;
        let state = table.normalize_state(&state);
        let table = table.returning_to(state.list_url(&request.page));
        let retry_signals = signals.clone();
        let rendered = async {
            let rows = load_scoped_page(cx, &resource, &table, &state, scope).await?;
            table
                .render_live(cx, rows, &state, &request.page, signals)
                .await
        };
        match rendered.await {
            Ok(view) => Ok(view),
            Err(error) => Ok(table_error_view(
                cx,
                &resource,
                &state,
                &error,
                &request.page,
                Some(&retry_signals),
            )),
        }
    })
}
