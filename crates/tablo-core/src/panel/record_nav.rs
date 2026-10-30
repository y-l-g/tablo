//! A record's navigation: the tab bar its detail page, edit page, record pages and relation tabs
//! share, once its resource declares a record page or a relation tab.

use topcoat::{
    context::Cx,
    icon::{IconData, icon},
    view::{BoxView, ViewExt, class, view},
};

use super::{
    list::{TAB_ACTIVE_CLASS, TAB_CLASS, TAB_IDLE_CLASS},
    state::current,
    url::{record_page_url, row_edit_url, row_view_url},
};
use crate::{
    policy::Ability,
    resource::{Mounted, Resource},
    t,
};

/// One destination of a record's navigation.
pub(crate) struct RecordNavLink {
    label: String,
    icon: Option<IconData>,
    url: String,
    /// Whether the request is on this destination.
    current: bool,
}

/// The destinations of `record`'s navigation the request may open: its detail page, its edit
/// page, its record pages and its relation tabs. Empty when `resource` declares no record page
/// and no relation tab, whose records' pages keep their headers alone.
pub(crate) fn record_navigation<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    record: &R::Model,
) -> Vec<RecordNavLink> {
    let tabs = resource.relations.iter().any(|relation| relation.tab);
    if resource.record_pages.is_empty() && !tabs {
        return Vec::new();
    }
    let key = resource.table.key_of(record);
    let mut links = Vec::new();
    if resource.viewed() {
        let url = row_view_url(&resource.url, &key);
        links.push((t!(cx, "tablo-crumb-view"), None, url));
    }
    if resource.pages.edit
        && !resource.is_trashed(record)
        && resource.can(cx, Ability::Update(record))
    {
        let url = row_edit_url(&resource.url, &key);
        links.push((t!(cx, "tablo-crumb-edit"), None, url));
    }
    for page in &resource.record_pages {
        if page.admits(cx, record) {
            let url = record_page_url(&resource.url, &key, &page.slug);
            links.push((page.label.localize(cx), page.icon.clone(), url));
        }
    }
    if let Some(panel) = current(cx) {
        for relation in resource.relations.iter().filter(|relation| relation.tab) {
            let Some(child) = panel.children.get(&relation.child) else {
                continue;
            };
            if (child.listable)(cx) {
                let label = relation.label.as_ref().unwrap_or(&child.plural_label);
                let url = record_page_url(&resource.url, &key, &child.slug);
                links.push((label.localize(cx), None, url));
            }
        }
    }
    let here = topcoat::router::request::uri(cx).path().to_string();
    links
        .into_iter()
        .map(|(label, glyph, url)| RecordNavLink {
            label,
            icon: glyph,
            current: url == here,
            url,
        })
        .collect()
}

/// Renders `links` as the tab bar under a record's header; nothing without a link.
pub(crate) fn render_record_navigation(cx: &Cx, links: Vec<RecordNavLink>) -> BoxView<'_> {
    if links.is_empty() {
        return ().boxed();
    }
    view! {
        cx =>
        <nav
            aria-label=(t!(cx, "tablo-record-navigation"))
            class="overflow-x-auto border-b border-border pb-px"
        >
            <div class="-mb-px flex gap-4">
                for link in links {
                    <a
                        (crate::navigation::runtime_link(cx, &link.url))
                        data-record-nav=(link.url.clone())
                        aria-current=(link.current.then_some("page"))
                        class=(class!(
                            TAB_CLASS,
                            if link.current { TAB_ACTIVE_CLASS } else { TAB_IDLE_CLASS },
                        ))
                    >
                        if let Some(data) = link.icon {
                            icon(data: data)
                        }
                        (link.label)
                    </a>
                }
            </div>
        </nav>
    }
    .boxed()
}
