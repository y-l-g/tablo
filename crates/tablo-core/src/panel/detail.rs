//! The detail page: `GET {prefix}/{slug}/{id}`, read-only.
//!
//! One record, rendered through [`Resource::view`] — the same `Schema` a form
//! uses, read the other way round. It lives beside the form handlers rather
//! than in `list.rs` because it is a record page: it loads through the same
//! tenant-scoped query (`find_by_key`), checks the same `can_view`
//! policy,
//! and answers the same 404 for an unknown or out-of-scope id.

use topcoat::{
    context::Cx,
    router::{Body, error::not_found, path_param_segment},
    view::{BoxView, HoistView, ViewExt, internal::ThenView, view},
};

use super::{
    actions::load_detail,
    gate::{gate, list_url},
    relations::render_relations,
};
use crate::{db::db, form::RecordForm, resource::Resource};

/// Detail page GET.
///
/// A resource that declares no [`view`](Resource::view) has no detail page:
/// the handler 404s rather than rendering an empty shell, which keeps
/// "declares nothing" and "no such page" the same answer for a hand-typed URL
/// and makes the row link's absence honest.
///
/// The record loads through the tenant-scoped query (`find_by_key` — the
/// tenancy half derived by the framework, — and the resource's own
/// soft-delete scope, ADR-0002), so an unknown id and an id outside the
/// request's scope get one answer, as everywhere else in the panel. `can_view`
/// on the loaded record is a 403 rather than a 404: the record exists and this
/// caller may not see it.
pub(crate) fn resource_view<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::new(async move {
        gate::<R>(cx)?;
        if !R::viewed(cx) {
            return Err(not_found().into());
        }
        let id = path_param_segment(cx, "id").to_string();
        let mut db = db(cx);
        let record = load_detail::<R>(cx, &mut db).await?;
        // The form's projection over `view_values`, so the page and the form
        // agree about what a field holds (ADR-0016); `NoForm` projects nothing.
        let mut values = R::view_values(cx, &record);
        values.extend(<R::Form as RecordForm>::hydrate(cx, &record));
        let body = R::view(cx)
            .render(cx, crate::schema::Source::view(&values))
            .await?;
        // What the `Schema` above cannot carry: free-form content read off the
        // record, then each relation's table, which runs its own query.
        let content = R::view_content(cx, &record);
        let relations = render_relations::<R>(cx, &record);
        // The record's own label titles the page when the resource declares
        // one. The fallback is the page's name plus the URL's record
        // key, which is what the route carries (the display key drives the list
        // and the record key the action URLs).
        let title = detail_title::<R>(cx, &record, &id);
        let back = list_url(cx, &R::slug());
        let public = R::public_url(cx, &record);
        Ok(view! {
            cx =>
            tablo_ui::page(
                tablo_ui::page_header(
                    tablo_ui::page_title((title))
                    <a
                        (crate::resource::runtime_link(cx, &back))
                        class="text-sm text-muted-foreground underline"
                    >
                        "Back to list"
                    </a>
                    if let Some(public) = public {
                        <a
                            href=(public)
                            class="text-sm text-muted-foreground underline"
                        >
                            "View public post"
                        </a>
                    }
                )
                tablo_ui::page_content(
                    <div class="flex flex-col gap-4">
                        (body)
                        if let Some(content) = content {
                            (content)
                        }
                        for relation in relations {
                            (relation)
                        }
                    </div>
                )
            )
        }
        .boxed())
    })))
}

/// The detail page's title: the record's label when the resource
/// declares one ([`Resource::record_label`]), else the page's name and the
/// URL's record key.
fn detail_title<R: Resource>(cx: &Cx, record: &R::Model, id: &str) -> String {
    R::record_label(cx, record).unwrap_or_else(|| format!("{} {id}", R::label()))
}

#[cfg(test)]
mod tests;
