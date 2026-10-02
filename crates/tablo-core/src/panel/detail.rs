//! The detail page: `GET {prefix}/{slug}/{id}`, read-only.

use topcoat::{
    context::Cx,
    icon::icon,
    router::{Body, error::not_found, path_param_segment},
    view::{BoxView, HoistView, ViewExt, internal::ThenView, view},
};

use super::{
    actions::load_detail,
    gate::{gate, list_url},
    relations::render_relations,
};
use crate::{
    db::db,
    form::RecordForm,
    policy::{Ability, can},
    resource::{Resource, declared},
};

/// Renders the detail page, 404ing without a declared view or for unknown ids and 403ing view-denied records.
pub(crate) fn resource_view<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::new(async move {
        gate::<R>(cx)?;
        let declared = declared::<R>(cx);
        if !declared.viewed() {
            return Err(not_found().into());
        }
        let id = path_param_segment(cx, "id").to_string();
        let mut db = db(cx);
        let record = load_detail::<R>(cx, &mut db).await?;
        // Projects the form over `view_values` (ADR-0016).
        let mut values = R::view_values(cx, &record);
        values.extend(<R::Form as RecordForm>::hydrate(cx, &record));
        let body = declared
            .view
            .render(cx, crate::schema::Source::view(&values))
            .await?;
        let content = R::view_content(cx, &record);
        let relations = render_relations::<R>(cx, &record, true);
        let title = detail_title::<R>(cx, &record, &id);
        let back = list_url(cx, &R::slug());
        let public = R::public_url(cx, &record);
        let edit = (<R::Form as RecordForm>::HAS_FORM && can::<R>(cx, Ability::Update(&record)))
            .then(|| {
                format!(
                    "{}/{}",
                    topcoat::router::request::uri(cx).path(),
                    crate::resource::EDIT_ROUTE_SEGMENT
                )
            });
        let outline =
            tablo_ui::button_variants(tablo_ui::ButtonVariant::Outline, tablo_ui::ButtonSize::Md);
        Ok(view! {
            cx =>
            tablo_ui::page(
                tablo_ui::page_header(
                    tablo_ui::page_title((title))
                    tablo_ui::page_actions(
                        <a
                            (crate::resource::runtime_link(cx, &back))
                            class=(outline.clone())
                        >
                            icon(data: tablo_ui::icons::ARROW_LEFT)
                            "Back to list"
                        </a>
                        if let Some(public) = public {
                            <a href=(public) class=(outline.clone())>
                                icon(data: tablo_ui::icons::EXTERNAL_LINK)
                                "View public post"
                            </a>
                        }
                        if let Some(url) = edit {
                            <a
                                (crate::resource::runtime_link(cx, &url))
                                class=(tablo_ui::button_variants(
                                    tablo_ui::ButtonVariant::Primary,
                                    tablo_ui::ButtonSize::Md,
                                ))
                            >
                                icon(data: tablo_ui::icons::PENCIL)
                                "Edit"
                            </a>
                        }
                    )
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

/// Builds the detail title from the record label, else the page name and record key.
fn detail_title<R: Resource>(cx: &Cx, record: &R::Model, id: &str) -> String {
    R::record_label(cx, record).unwrap_or_else(|| format!("{} {id}", R::label()))
}

#[cfg(test)]
mod tests;
