//! Renders each [`Relation`](crate::resource::Relation) of a record page's resource as the related resource's table narrowed to the record.

use topcoat::{
    Result,
    context::Cx,
    icon::icon,
    view::{BoxView, HoistView, ViewExt, internal::ThenView, suspense, view},
};

use super::{
    gate::{enforce_tenant, list_url},
    list::{declared_chrome, load_scoped_page, table_error_view, wire_table},
    search::RelationRequest,
};
use crate::{
    form::RecordForm,
    policy::{Ability, can},
    resource::{
        BoundRelation, RETURN_PARAM, Resource, TABLE_CARD_CLASS, Table, TableChrome, TableState,
        create_page_url, declared, request_query, runtime_link,
    },
};

/// Renders the relation tables of `R`'s record `owner`.
pub(crate) fn render_relations<'a, R: Resource>(
    cx: &'a Cx,
    owner: &R::Model,
    read_only: bool,
) -> Vec<BoxView<'a>> {
    let page = topcoat::router::request::uri(cx).path().to_string();
    declared::<R>(cx)
        .relations
        .iter()
        .map(|relation| relation.render(cx, owner, &page, read_only, &R::slug()))
        .collect()
}

/// Derives one relation table's action chrome, view-only on a read-only page.
pub(crate) fn relation_chrome<C: Resource>(cx: &Cx, read_only: bool) -> TableChrome {
    let declared = declared_chrome::<C>(cx);
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
    Box::pin(HoistView::new(ThenView::new(async move {
        if enforce_tenant::<C>(cx).is_err() || !can::<C>(cx, Ability::ViewAny) {
            return Ok(().boxed());
        }
        let table = wire_table::<C>(cx, false, relation_chrome::<C>(cx, relation.read_only));
        let state = table.normalize_state(&TableState::from_cx_prefixed(cx, &relation.key));
        let BoundRelation {
            ref seed,
            ref page,
            read_only,
            ..
        } = relation;
        let table = table.returning_to(state.list_url(page));
        let create_url =
            (!read_only && <C::Form as RecordForm>::HAS_FORM && can::<C>(cx, Ability::Create))
                .then(|| {
                    let query = form_urlencoded::Serializer::new(String::new())
                        .append_pair(&seed.0, &seed.1)
                        .append_pair(RETURN_PARAM, &state.list_url(page))
                        .finish();
                    format!("{}?{query}", create_page_url(&list_url(cx, &C::slug())))
                });
        if table.is_live_search() {
            return relation_table_live::<C>(cx, table, state, relation, create_url).await;
        }
        let BoundRelation {
            key,
            label,
            scope,
            page,
            ..
        } = relation;
        let body = match load_scoped_page::<C>(cx, &table, &state, scope).await {
            Ok(rows) => table.render_with_state(cx, rows, &state, &page).await?,
            Err(error) => table_error_view::<C>(cx, &state, &error, &page, None),
        };
        let header = relation_header::<C>(cx, label, create_url);
        Ok(view! {
            cx =>
            <section class="flex flex-col gap-3" data-relation=(key)>
                (header)
                (body)
            </section>
        }
        .boxed())
    })))
}

/// Renders a live-search relation's section with its bars hoisted above the streamed region.
async fn relation_table_live<C: Resource>(
    cx: &Cx,
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
    let header = relation_header::<C>(cx, label, create_url);
    let invocation_key = key.clone();
    let lazy_rows = ThenView::new(async move {
        let retry_signals = signals.clone();
        let rendered = table
            .render_live_relation_invocation(
                cx,
                &parent,
                &invocation_key,
                RelationRequest {
                    seed: seed.1.clone(),
                    page: page.clone(),
                    read_only,
                },
                signals,
            )
            .await;
        match rendered {
            Ok(view) => Ok(view),
            Err(error) => Ok(table_error_view::<C>(
                cx,
                &state,
                &error,
                &page,
                Some(&retry_signals),
            )),
        }
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
fn relation_header<C: Resource>(cx: &Cx, label: String, create_url: Option<String>) -> BoxView<'_> {
    let create_label = format!("New {}", C::label());
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
