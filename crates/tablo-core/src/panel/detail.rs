//! The detail page: `GET {prefix}/{slug}/{id}`, the record's columns and its relation tables,
//! under a header with the record's actions and its navigation.

use topcoat::{
    context::Cx,
    icon::icon,
    router::{Body, error::not_found, path_param_segment},
    view::{BoxView, ViewExt, view},
};

use super::{
    actions::load_detail,
    bar::{ActionBar, record_bar},
    gate::gate,
    record_nav::{RecordNavLink, record_navigation, render_record_navigation},
    relations::render_relations,
};
use crate::{
    db::db,
    policy::Ability,
    resource::{Places, PublicLink, Resource},
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
        crate::csrf::ensure_token(cx);
        let id = path_param_segment(cx, "id").to_string();
        let mut db = db(cx)?;
        let record = load_detail(cx, &resource, &mut db).await?;
        let body = resource
            .detail
            .render_without(cx, &record, resource.field_mask(cx).hidden());
        let relations = render_relations(cx, &resource, &record);
        let title = resource.record_title(cx, &record, &id);
        let back = resource.url.clone();
        let public_link = resource.public_link(cx, &record);
        let uri = topcoat::router::request::uri(cx);
        let edit = (resource.pages.edit
            && !resource.is_trashed(&record)
            && resource.can(cx, Ability::Update(&record)))
        .then(|| format!("{}/{}", uri.path(), super::url::EDIT_ROUTE_SEGMENT));
        // An action lands back on the page as it was left: its related tables' state included.
        let here = uri
            .path_and_query()
            .map_or(uri.path(), |full| full.as_str());
        let actions = record_bar(cx, &resource, &record, Places::DETAIL, here);
        let navigation = record_navigation(cx, &resource, &record);
        Ok(detail_page(
            cx,
            DetailPage {
                title,
                back,
                public_link,
                actions,
                navigation,
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
    actions: ActionBar,
    navigation: Vec<RecordNavLink>,
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
        actions,
        navigation,
        edit,
        body,
        relations,
    } = page;
    let actions = actions.render(cx);
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
                        (crate::t!(cx, "tablo-detail-back"))
                    </a>
                    if let Some(link) = public_link {
                        <a href=(link.url) class=(outline.clone())>
                            icon(data: tablo_ui::icons::EXTERNAL_LINK)
                            (link.label.localize(cx))
                        </a>
                    }
                    (actions)
                    if let Some(url) = edit {
                        <a
                            (crate::navigation::runtime_link(cx, &url))
                            class=(tablo_ui::button_variants(
                                tablo_ui::ButtonVariant::Primary,
                                tablo_ui::ButtonSize::Md,
                            ))
                        >
                            icon(data: tablo_ui::icons::PENCIL)
                            (crate::t!(cx, "tablo-action-edit"))
                        </a>
                    }
                )
            )
            (render_record_navigation(cx, navigation))
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
