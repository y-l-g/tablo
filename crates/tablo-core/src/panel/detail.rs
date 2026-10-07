//! The detail page: `GET {prefix}/{slug}/{id}`, the record's fields and its relation tables.

use topcoat::{
    context::Cx,
    icon::icon,
    router::{Body, error::not_found, path_param_segment},
    view::{BoxView, ViewExt, view},
};

use super::{actions::load_detail, gate::gate, relations::render_relations};
use crate::{
    db::db,
    form::RecordForm,
    policy::Ability,
    resource::{Mounted, Resource},
    topcoat_compat::async_page,
};

/// Renders the detail page, 404ing without a declared view or for unknown ids and 403ing
/// view-denied records.
pub(crate) fn resource_view<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        if !resource.viewed() {
            return Err(not_found().into());
        }
        let id = path_param_segment(cx, "id").to_string();
        let mut db = db(cx);
        let record = load_detail(cx, &resource, &mut db).await?;
        // Projects the form over `view_values`; see `Resource::view_values`.
        let mut values = R::view_values(cx, &record);
        values.extend(<R::Form as RecordForm>::hydrate(cx, &record));
        let body = resource
            .view()
            .render(cx, crate::schema::Source::view(&values))
            .await?;
        let content = R::view_content(cx, &record);
        let relations = render_relations(cx, &resource, &record);
        let title = detail_title(cx, &resource, &record, &id);
        let back = resource.url.clone();
        let public = R::public_url(cx, &record);
        let edit = (<R::Form as RecordForm>::HAS_FORM
            && resource.can(cx, Ability::Update(&record)))
        .then(|| {
            format!(
                "{}/{}",
                topcoat::router::request::uri(cx).path(),
                crate::table::EDIT_ROUTE_SEGMENT
            )
        });
        Ok(detail_page(
            cx,
            DetailPage {
                title,
                back,
                public,
                edit,
                body,
                content,
                relations,
            },
        ))
    })
}

/// What the detail page shows, resolved from the record.
struct DetailPage<'a> {
    title: String,
    back: String,
    public: Option<String>,
    edit: Option<String>,
    body: BoxView<'a>,
    content: Option<BoxView<'a>>,
    relations: Vec<BoxView<'a>>,
}

/// Renders the detail page around its resolved parts.
fn detail_page<'a>(cx: &'a Cx, page: DetailPage<'a>) -> BoxView<'a> {
    let DetailPage {
        title,
        back,
        public,
        edit,
        body,
        content,
        relations,
    } = page;
    let outline =
        tablo_ui::button_variants(tablo_ui::ButtonVariant::Outline, tablo_ui::ButtonSize::Md);
    view! {
        cx =>
        tablo_ui::page(
            tablo_ui::page_header(
                tablo_ui::page_title((title))
                tablo_ui::page_actions(
                    <a
                        (crate::navigation::runtime_link(cx, &back))
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
                            (crate::navigation::runtime_link(cx, &url))
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
    .boxed()
}

/// Builds the detail title from the record label, else the page name and record key.
fn detail_title<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    record: &R::Model,
    id: &str,
) -> String {
    R::record_label(cx, record).unwrap_or_else(|| format!("{} {id}", resource.label))
}

#[cfg(test)]
mod tests;
