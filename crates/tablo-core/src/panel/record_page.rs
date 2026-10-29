//! A record's own pages beyond its detail and edit pages: its record pages,
//! `GET {prefix}/{slug}/{id}/{page}`, and its relation tabs, `GET {prefix}/{slug}/{id}/{related}`,
//! each the record's header and navigation over the page's body.

use topcoat::{
    context::Cx,
    icon::icon,
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
    record_nav::{RecordNavLink, record_navigation, render_record_navigation},
    relations::{relation_tab, render_relation},
    url::row_view_url,
};
use crate::{
    db::db,
    resource::{Mounted, Resource},
    topcoat_compat::async_page,
};

/// The last segment of the request's path: the record page or relation tab it names.
fn last_segment(cx: &Cx) -> String {
    let uri = uri(cx);
    uri.path()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

/// Renders the record page the request's last path segment names, 404ing for unknown ids and
/// 403ing a record the policy's `View` or the page's access refuses.
pub(crate) fn record_page_view<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        let slug = last_segment(cx);
        let Some(page) = resource.record_pages.iter().find(|page| page.slug == slug) else {
            return Err(not_found().into());
        };
        crate::csrf::ensure_token(cx);
        let record = load_record(cx, &resource, &mut db(cx)?).await?;
        if !page.admits(cx, &record) {
            return Err(forbidden().into());
        }
        let header = RecordHeader::of(cx, &resource, &record);
        let body = (page.render)(cx, record).await?;
        Ok(record_page(cx, header, body))
    })
}

/// Renders the relation tab the request's last path segment names, 404ing for unknown ids,
/// 403ing a record the policy's `View` refuses and a related resource the request may not list.
pub(crate) fn relation_tab_view<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        let slug = last_segment(cx);
        let Some((relation, child)) = relation_tab(cx, &resource, &slug) else {
            return Err(not_found().into());
        };
        crate::csrf::ensure_token(cx);
        let record = load_record(cx, &resource, &mut db(cx)?).await?;
        if !(child.listable)(cx) {
            return Err(forbidden().into());
        }
        let header = RecordHeader::of(cx, &resource, &record);
        let body = render_relation(cx, &resource, &record, relation).unwrap_or_else(|| ().boxed());
        Ok(record_page(cx, header, body))
    })
}

/// The header a record page or relation tab shows above its body.
struct RecordHeader {
    title: String,
    /// The detail page when the resource declares one, else the list.
    back: String,
    navigation: Vec<RecordNavLink>,
}

impl RecordHeader {
    fn of<R: Resource>(cx: &Cx, resource: &Mounted<R>, record: &R::Model) -> Self {
        let key = resource.table.key_of(record);
        Self {
            title: resource.record_title(cx, record, path_param_segment(cx, "id")),
            back: if resource.viewed() {
                row_view_url(&resource.url, &key)
            } else {
                resource.url.clone()
            },
            navigation: record_navigation(cx, resource, record),
        }
    }
}

/// Renders a record page or relation tab around its body.
fn record_page<'a>(cx: &'a Cx, header: RecordHeader, body: BoxView<'a>) -> BoxView<'a> {
    let RecordHeader {
        title,
        back,
        navigation,
    } = header;
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
            (render_record_navigation(cx, navigation))
            tablo_ui::page_content((body))
        )
    }
    .boxed()
}
