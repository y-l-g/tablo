//! The detail page: `GET {prefix}/{slug}/{id}`, the record's columns and its relation tables.

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
    resource::{PublicLink, Resource},
    topcoat_compat::async_page,
};

/// Renders the detail page, 404ing without a declared detail page or for unknown ids and 403ing
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
        let body = resource.view.render(cx, &record);
        let relations = render_relations(cx, &resource, &record);
        let title = resource.record_title(&record, &id);
        let back = resource.url.clone();
        let public_link = resource.public_link(&record);
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
                public_link,
                edit,
                body,
                relations,
            },
        ))
    })
}

/// What the detail page shows, resolved from the record.
struct DetailPage<'a> {
    title: String,
    back: String,
    public_link: Option<PublicLink>,
    edit: Option<String>,
    body: BoxView<'a>,
    relations: Vec<BoxView<'a>>,
}

/// Renders the detail page around its resolved parts.
fn detail_page<'a>(cx: &'a Cx, page: DetailPage<'a>) -> BoxView<'a> {
    let DetailPage {
        title,
        back,
        public_link,
        edit,
        body,
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
                    if let Some(link) = public_link {
                        <a href=(link.url) class=(outline.clone())>
                            icon(data: tablo_ui::icons::EXTERNAL_LINK)
                            (link.label)
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
                    for relation in relations {
                        (relation)
                    }
                </div>
            )
        )
    }
    .boxed()
}
