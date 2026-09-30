//! Relation tables on a record page: each
//! [`Relation`](crate::resource::Relation) of the page's resource,
//! rendered as the related resource's list table narrowed to the record.

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
    resource::{
        BoundRelation, RETURN_PARAM, Resource, TABLE_CARD_CLASS, Table, TableChrome, TableState,
        create_page_url, request_query, runtime_link,
    },
};

/// The relation tables of `R`'s record `owner`, for the page at the request's
/// path: one section per [`Resource::relations`] entry, in declaration order.
/// A `read_only` page — the detail page — shows the rows without write
/// actions, as Filament's view page does; the edit page carries them.
pub(crate) fn render_relations<'a, R: Resource>(
    cx: &'a Cx,
    owner: &R::Model,
    read_only: bool,
) -> Vec<BoxView<'a>> {
    let page = topcoat::router::request::uri(cx).path().to_string();
    R::relations()
        .iter()
        .map(|relation| relation.render(cx, owner, &page, read_only, &R::slug()))
        .collect()
}

/// The action chrome of one relation's table: the resource's declared chrome,
/// or view-only on a read-only page — the detail page shows the rows without
/// write actions, as Filament's view page does; the edit page carries them.
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

/// One relation's section: `C`'s list table over the rows the owner holds,
/// its URL parameters prefixed with the relation's key, and — unless the
/// page is read-only — its write actions returning to this page and a create
/// link that opens `C`'s form with the owner chosen.
///
/// `C`'s own gates apply: a request that lacks a tenant `C` requires, or that
/// `C::can_view_any` refuses, gets no section, and the row actions keep their
/// per-row policy. A live-search table renders its search and filter bars
/// eagerly above the streamed region while the relation shard invocation
/// fills the table below, so sort, search, filters and pagination re-render
/// in place with focus and scroll surviving; the GET form stays inside
/// `<noscript>` as the no-JS fallback.
pub(crate) fn relation_table<C: Resource>(cx: &Cx, relation: BoundRelation) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::new(async move {
        if enforce_tenant::<C>(cx).is_err() || !C::can_view_any(cx) {
            return Ok(().boxed());
        }
        let BoundRelation {
            parent,
            key,
            label,
            scope,
            seed,
            page,
            read_only,
        } = relation;
        let table = wire_table::<C>(cx, false, relation_chrome::<C>(cx, read_only));
        let state = table.normalize_state(&TableState::from_cx_prefixed(cx, &key));
        // A write returns to the page as the table shows it, without a
        // dialog left open on a row the write removed.
        let table = table.returning_to(state.list_url(&page));
        let create_url = (!read_only && <C::Form as RecordForm>::HAS_FORM && C::can_create(cx))
            .then(|| {
                let query = form_urlencoded::Serializer::new(String::new())
                    .append_pair(&seed.0, &seed.1)
                    .append_pair(RETURN_PARAM, &state.list_url(&page))
                    .finish();
                format!("{}?{query}", create_page_url(&list_url(cx, &C::slug())))
            });
        if table.is_live_search() {
            return Ok(relation_table_live::<C>(
                cx,
                table,
                state,
                BoundRelation {
                    parent,
                    key,
                    label,
                    scope,
                    seed,
                    page,
                    read_only,
                },
                create_url,
            )
            .await?
            .boxed());
        }
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

/// A live-search relation's section: the same header and create link as the
/// static section, the search and filter bars hoisted above the streamed
/// region, and the relation shard invocation below.
///
/// The bars sit outside the swapped region for the same focus rationale as
/// the live list page: a `<select>` change re-renders the table, and a
/// control inside the swapped region would lose focus and collapse its popup
/// mid-change. The signals are keyed by page and relation key, so several
/// relations on one page never share a query or a selection.
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
    // Seeded with the request's query as written: the shard parses its own
    // prefixed parameters out of it and normalizes them as this page did.
    //
    // Keyed by page and relation key: runtime navigation carries every signal
    // the next page shares with this one, and one call site would otherwise
    // give every relation on the page the same ids — one relation's search
    // would filter the next.
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
    // The filter bar is hoisted next to the search host: a
    // `<select>` change re-renders the table, and a control inside the
    // swapped region would lose focus and collapse its popup mid-change.
    let filter_bar = if table.filter_bar_enabled() {
        Some(
            table
                .render_live_filter_bar(cx, &state, &page, &signals)
                .await?,
        )
    } else {
        None
    };
    // The shard's table renders neither bar, so neither does the placeholder
    // it replaces.
    // Nor does it draw a card: the section's card holds the bars and the
    // table together.
    let table = table.hide_search().hide_filter_bar().unframed();
    let skeleton = table.render_skeleton(cx, &state).await?;
    // The delete confirmation dialog is not part of the swapped table
    // region: a keystroke starts a new result set and must never carry
    // (or re-open) a dialog, so the live section renders it eagerly once.
    let delete_dialog = table.render_delete_dialog(cx, &state).await?;
    let header = relation_header::<C>(cx, label, create_url);
    // The view below moves `key` into the section marker; the streamed rows
    // only borrow it, so they take a clone.
    let invocation_key = key.clone();
    let lazy_rows = ThenView::new(async move {
        // The retry link inside the table writes the same signals the
        // toolbar does, so a bad cursor recovers in place.
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
            // The section draws the table's card (`TABLE_CARD_CLASS`) so the
            // hoisted bars share it with the table they drive, as on the live
            // list page.
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

/// A relation section's heading row: the relation's label and, when the
/// page allows it, the link that creates a child seeded with this owner.
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
