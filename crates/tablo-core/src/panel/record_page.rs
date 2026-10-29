//! A resource's record pages: `GET {prefix}/{slug}/{id}/{page}`, the record's header over the
//! page's own body, and the links the record's detail page carries to them.

use topcoat::{
    context::Cx,
    icon::{IconData, icon},
    router::{
        Body,
        error::{forbidden, not_found},
        path_param_segment,
        request::uri,
    },
    view::{BoxView, ViewExt, view},
};

use super::{
    actions::load_record,
    gate::gate,
    url::{record_page_url, row_view_url},
};
use crate::{
    db::db,
    resource::{Mounted, Resource},
    topcoat_compat::async_page,
};

/// Renders the record page the request's last path segment names, 404ing for unknown ids and
/// 403ing a record the policy's `View` or the page's access refuses.
pub(crate) fn record_page_view<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        let uri = uri(cx);
        let slug = uri.path().rsplit('/').next().unwrap_or_default();
        let Some(page) = resource.record_pages.iter().find(|page| page.slug == slug) else {
            return Err(not_found().into());
        };
        crate::csrf::ensure_token(cx);
        let id = path_param_segment(cx, "id").to_string();
        let record = load_record(cx, &resource, &mut db(cx)?).await?;
        if !page.admits(cx, &record) {
            return Err(forbidden().into());
        }
        let title = resource.record_title(cx, &record, &id);
        let back = if resource.viewed() {
            row_view_url(&resource.url, &resource.table.key_of(&record))
        } else {
            resource.url.clone()
        };
        let body = (page.render)(cx, record).await?;
        Ok(record_page(cx, title, back, body))
    })
}

/// Renders a record page around its body.
fn record_page<'a>(cx: &'a Cx, title: String, back: String, body: BoxView<'a>) -> BoxView<'a> {
    let outline =
        tablo_ui::button_variants(tablo_ui::ButtonVariant::Outline, tablo_ui::ButtonSize::Md);
    view! {
        cx =>
        tablo_ui::page(
            tablo_ui::page_header(
                tablo_ui::page_title((title))
                tablo_ui::page_actions(
                    <a (crate::navigation::runtime_link(cx, &back)) class=(outline)>
                        icon(data: tablo_ui::icons::ARROW_LEFT)
                        (crate::t!(cx, "tablo-detail-back"))
                    </a>
                )
            )
            tablo_ui::page_content((body))
        )
    }
    .boxed()
}

/// A link to one record page.
pub(super) struct RecordPageLink {
    label: String,
    icon: Option<IconData>,
    url: String,
}

/// The links to the record pages of `record` the request may open.
pub(super) fn record_page_links<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    record: &R::Model,
) -> Vec<RecordPageLink> {
    let key = resource.table.key_of(record);
    resource
        .record_pages
        .iter()
        .filter(|page| page.admits(cx, record))
        .map(|page| RecordPageLink {
            label: page.label.localize(cx),
            icon: page.icon.clone(),
            url: record_page_url(&resource.url, &key, &page.slug),
        })
        .collect()
}

/// Renders `links` as buttons for a record's page header.
pub(super) fn render_record_page_links(cx: &Cx, links: Vec<RecordPageLink>) -> BoxView<'_> {
    let outline =
        tablo_ui::button_variants(tablo_ui::ButtonVariant::Outline, tablo_ui::ButtonSize::Md);
    view! {
        cx =>
        for link in links {
            <a
                (crate::navigation::runtime_link(cx, &link.url))
                class=(outline.clone())
                data-record-page=(link.url.clone())
            >
                if let Some(data) = link.icon {
                    icon(data: data)
                }
                (link.label)
            </a>
        }
    }
    .boxed()
}
