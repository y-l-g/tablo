//! Relation tables on a record page: each
//! [`Relation`](crate::resource::Relation) of the page's resource,
//! rendered as the related resource's list table narrowed to the record.

use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, internal::ThenView, view},
};

use super::{
    gate::{enforce_tenant, list_url},
    list::{load_scoped_page, table_error_view, wire_table_actions},
};
use crate::{
    form::RecordForm,
    resource::{BoundRelation, RETURN_PARAM, Resource, TableState, create_page_url, runtime_link},
};

/// The relation tables of `R`'s record `owner`, for the page at the request's
/// path: one section per [`Resource::relations`] entry, in declaration order.
pub(crate) fn render_relations<'a, R: Resource>(cx: &'a Cx, owner: &R::Model) -> Vec<BoxView<'a>> {
    let page = topcoat::router::request::uri(cx).path().to_string();
    R::relations()
        .iter()
        .map(|relation| relation.render(cx, owner, &page))
        .collect()
}

/// One relation's section: `C`'s list table over the rows the owner holds,
/// its URL parameters keyed by the relation, every write it starts returning
/// to this page, and a create link that opens `C`'s form with the owner chosen.
///
/// `C`'s own gates apply: a tenant `C` requires, `can_view_any` for the
/// section at all, and the per-row policy on the row actions. A live-search
/// table renders its server-side form here; the live shard serves `C`'s list.
pub(crate) fn relation_table<C: Resource>(cx: &Cx, relation: BoundRelation) -> BoxView<'_> {
    Box::pin(ThenView::new(async move {
        enforce_tenant::<C>(cx)?;
        if !C::can_view_any(cx) {
            return Ok(().boxed());
        }
        let BoundRelation {
            key,
            label,
            scope,
            seed,
            page,
        } = relation;
        let table = wire_table_actions::<C>(cx, false);
        let state = table.normalize_state(&TableState::from_cx_keyed(cx, &key));
        // A write returns to the page as the table shows it, without a
        // dialog left open on a row the write removed.
        let table = table.returning_to(state.list_url(&page));
        let create_url = (<C::Form as RecordForm>::HAS_FORM && C::can_create(cx)).then(|| {
            let query = form_urlencoded::Serializer::new(String::new())
                .append_pair(&seed.0, &seed.1)
                .append_pair(RETURN_PARAM, &state.list_url(&page))
                .finish();
            format!("{}?{query}", create_page_url(&list_url(cx, &C::slug())))
        });
        let body = match load_scoped_page::<C>(cx, &table, &state, scope).await {
            Ok(rows) => table.render_with_state(cx, rows, &state, &page).await?,
            Err(error) => table_error_view::<C>(cx, &state, &error, &page, None),
        };
        let create_label = format!("New {}", C::label());
        Ok(view! {
            cx =>
            <section class="flex flex-col gap-3" data-relation=(key)>
                <div class="flex items-center justify-between gap-4">
                    <h2 class="text-base font-semibold">(label)</h2>
                    if let Some(url) = create_url {
                        <a
                            (runtime_link(cx, &url))
                            class=(tablo_ui::button_variants(
                                tablo_ui::ButtonVariant::Outline,
                                tablo_ui::ButtonSize::Sm,
                            ))
                        >
                            (create_label)
                        </a>
                    }
                </div>
                (body)
            </section>
        }
        .boxed())
    }))
}
