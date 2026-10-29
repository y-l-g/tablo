//! URLs into the request's panel, so app code never spells a prefix.
//!
//! Each helper answers for the panel serving the request: the one whose
//! prefix the request is under, or the one it entered with
//! [`PanelHandle::enter`](crate::PanelHandle::enter).
//! A resource or page that panel does not register has no URL there, and the
//! helper returns `None`. The record helpers, [`record`], [`edit`], [`delete`], [`create`],
//! [`action`] and [`bulk_action`], also answer in a
//! [`PanelHandle::context`](crate::PanelHandle::context).
//!
//! ```rust,no_run
//! # use tablo_core::{NoForm, Page, Resource};
//! # use topcoat::{Result, context::Cx, view::View};
//! # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
//! # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
//! # struct PostResource;
//! # impl Resource for PostResource {
//! #     type Model = Post;
//! #     type Form = NoForm<Post>;
//! # }
//! # struct MediaLibraryPage;
//! # impl Page for MediaLibraryPage {
//! #     async fn render(_cx: &Cx) -> Result<impl View> { Ok(()) }
//! # }
//! # let cx: &Cx = todo!();
//! let posts = tablo_core::url::resource::<PostResource>(cx); // Some("/admin/posts")
//! let media = tablo_core::url::page::<MediaLibraryPage>(cx); // Some("/admin/media-library")
//! let home = tablo_core::url::panel(cx); // Some("/admin")
//! ```

use std::any::TypeId;

use topcoat::context::Cx;

use super::state::current;
pub(crate) use crate::naming::{
    CREATE_ROUTE_SEGMENT, DASH_ROUTE_SEGMENT, create_page_url, list_route,
};
use crate::{
    Page, RecordForm,
    resource::{Action, Mounted, Places, Resource, mounted},
    topcoat_compat::href,
};

/// The request's panel prefix, e.g. `/admin`.
pub fn panel(cx: &Cx) -> Option<String> {
    current(cx).map(|panel| panel.realm.prefix.clone())
}

/// `R`'s list URL in the request's panel, e.g. `/admin/posts`.
pub fn resource<R: Resource>(cx: &Cx) -> Option<String> {
    registered::<R>(cx)
}

/// `P`'s URL in the request's panel: `{prefix}/{slug}`, or the prefix itself
/// for the panel's [`home`](super::Panel::home) page.
pub fn page<P: Page>(cx: &Cx) -> Option<String> {
    registered::<P>(cx)
}

/// The URL to link or show a stored file at: `path` with a signature that expires, when it
/// names a file under a [`Panel::uploads_dir`](super::Panel::uploads_dir) of any panel on the
/// router, and `path` itself otherwise.
///
/// Call it only on a page that has checked the user may see the record storing `path`: the URL
/// opens the file for whoever holds it, for up to two hours.
///
/// ```rust,no_run
/// # let cx: &topcoat::context::Cx = todo!();
/// let src = tablo_core::url::file(cx, "/uploads/0b9c…-cover.png");
/// // "/uploads/0b9c…-cover.png?expires=…&signature=…"
/// ```
pub fn file(cx: &Cx, path: &str) -> String {
    let signed = !path.contains(['?', '#'])
        && topcoat::context::try_context::<super::state::Panels>(cx).is_some_and(|panels| {
            panels.0.iter().any(|panel| {
                panel
                    .signed_roots
                    .iter()
                    .any(|root| path.starts_with(root.as_str()))
            })
        });
    if signed {
        crate::upload::signed::sign(cx, path, crate::upload::signed::now())
    } else {
        path.to_owned()
    }
}

fn registered<T: 'static>(cx: &Cx) -> Option<String> {
    current(cx).and_then(|panel| panel.urls.get(&TypeId::of::<T>()).cloned())
}

/// `record`'s detail page in the context's panel, e.g. `/admin/posts/42`, or `None` when `R`
/// declares no detail.
///
/// This and the other record helpers answer in a
/// [`PanelHandle::context`](crate::PanelHandle::context) too, so a job or a test spells no record
/// path. They return `None` when the panel does not mount `R`, when `R`'s key has no URL form,
/// or when the panel serves no such route for `R`.
pub fn record<R: Resource>(cx: &Cx, record: &R::Model) -> Option<String> {
    keyed::<R>(cx, record, Mounted::viewed).map(|(list, key)| row_view_url(&list, &key))
}

/// `record`'s edit page, e.g. `/admin/posts/42/edit`, or `None` when `R` has no record form.
pub fn edit<R: Resource>(cx: &Cx, record: &R::Model) -> Option<String> {
    keyed::<R>(cx, record, |_| <R::Form as RecordForm>::HAS_FORM)
        .map(|(list, key)| row_edit_url(&list, &key))
}

/// The route that deletes `record`, e.g. `/admin/posts/42/delete`, which takes `confirm=1`.
pub fn delete<R: Resource>(cx: &Cx, record: &R::Model) -> Option<String> {
    keyed::<R>(cx, record, |_| true).map(|(list, key)| delete_action_url(&list, &key))
}

/// `R`'s create page, e.g. `/admin/posts/-/create`, or `None` when `R` serves none.
pub fn create<R: Resource>(cx: &Cx) -> Option<String> {
    mounted::<R>(cx)
        .filter(|mounted| mounted.creates)
        .map(|mounted| create_page_url(&mounted.url))
}

/// The route the action `A` runs on `record` at, e.g. `/admin/posts/42/-/actions/publish`, or
/// `None` when `R` does not declare `A` on a record: on its rows, detail or edit page.
pub fn action<R: Resource, A: Action<R>>(cx: &Cx, record: &R::Model) -> Option<String> {
    keyed::<R>(cx, record, |mounted| {
        declares::<R, A>(mounted, Places::RECORD)
    })
    .map(|(list, key)| row_action_url(&list, &key, A::NAME))
}

/// The route the action `A` runs on a selection at, e.g. `/admin/posts/-/actions/publish`, which
/// takes the selected keys in its `ids` field, or `None` when `R` does not declare `A` in its
/// bulk bar.
pub fn bulk_action<R: Resource, A: Action<R>>(cx: &Cx) -> Option<String> {
    mounted::<R>(cx)
        .filter(|mounted| declares::<R, A>(mounted, Places::BULK))
        .map(|mounted| bulk_action_url(&mounted.url, A::NAME))
}

/// `R`'s list URL and `record`'s key, when `R` is mounted, its rows have routes, and `serves`
/// holds for it.
fn keyed<R: Resource>(
    cx: &Cx,
    record: &R::Model,
    serves: impl FnOnce(&Mounted<R>) -> bool,
) -> Option<(String, String)> {
    let mounted = mounted::<R>(cx)?;
    (mounted.table.is_addressable() && serves(&*mounted))
        .then(|| (mounted.url.clone(), mounted.table.key_of(record)))
}

/// Whether `R` declares `A` in one of `places`: the action routes refuse it elsewhere.
fn declares<R: Resource, A: Action<R>>(mounted: &Mounted<R>, places: Places) -> bool {
    mounted
        .actions
        .find(A::NAME)
        .is_some_and(|action| action.places.intersects(places))
}

// The panel's URL scheme: the path segments its route table registers and the builders its
// links use, so the routes `register` serves and the links a page emits are spelled once.
// `TableState` owns a list link's query parameters.

/// The record placeholder the route table registers: `{id}`.
///
/// A route *pattern*, not a URL — the link helpers below take the encoded
/// primary key instead.
pub(crate) const RECORD_ROUTE_PARAM: &str = "{id}";

/// Path segment of the list's CSV export, under [`DASH_ROUTE_SEGMENT`].
pub(crate) const EXPORT_ROUTE_SEGMENT: &str = "export";

/// Path segment of the list's CSV import page, under [`DASH_ROUTE_SEGMENT`].
pub(crate) const IMPORT_ROUTE_SEGMENT: &str = "import";

/// Path segment of the import's template file, under the import page.
pub(crate) const TEMPLATE_ROUTE_SEGMENT: &str = "template";

/// Path segment of the record form's option search, under [`DASH_ROUTE_SEGMENT`].
pub(crate) const OPTIONS_ROUTE_SEGMENT: &str = "options";

/// Path segment of a row's edit page.
pub(crate) const EDIT_ROUTE_SEGMENT: &str = "edit";

/// Path segment of a row's delete POST.
pub(crate) const DELETE_ROUTE_SEGMENT: &str = "delete";

/// Path segment of the bulk-delete POST, under [`DASH_ROUTE_SEGMENT`].
pub(crate) const BULK_DELETE_ROUTE_SEGMENT: &str = "bulk-delete";

/// Path segment of the column picker's POST, under [`DASH_ROUTE_SEGMENT`].
pub(crate) const COLUMNS_ROUTE_SEGMENT: &str = "columns";

/// Path segment before a custom action's name, on a row
/// (`{prefix}/{key}/-/actions/{name}`) and on the list
/// (`{list}/-/actions/{name}`).
pub(crate) const ACTIONS_ROUTE_SEGMENT: &str = "actions";

/// The action-name placeholder the route table registers: `{action}`.
pub(crate) const ACTION_ROUTE_PARAM: &str = "{action}";

/// Path segment before a record's relation, whose table posts its attach and detach under
/// `{list}/{key}/-/relations/{relation}`.
pub(crate) const RELATIONS_ROUTE_SEGMENT: &str = "relations";

/// The relation placeholder the route table registers: `{relation}`, the related resource's slug.
pub(crate) const RELATION_ROUTE_PARAM: &str = "{relation}";

/// The related record's placeholder the route table registers: `{related}`.
pub(crate) const RELATED_ROUTE_PARAM: &str = "{related}";

/// A record's relation to the resource at `slug`, which its writes post under:
/// `{list}/{key}/-/relations/{slug}`.
pub(crate) fn relation_url(list: &str, key: &str, slug: &str) -> String {
    format!(
        "{list}/{}/{DASH_ROUTE_SEGMENT}/{RELATIONS_ROUTE_SEGMENT}/{slug}",
        href::encode_path_segment(key)
    )
}

/// The row's `Edit` link: `{prefix}/{key}/edit`.
pub(crate) fn row_edit_url(prefix: &str, key: &str) -> String {
    format!(
        "{prefix}/{}/{EDIT_ROUTE_SEGMENT}",
        href::encode_path_segment(key)
    )
}

/// The row's `View` link: `{prefix}/{key}` — the detail page.
pub(crate) fn row_view_url(prefix: &str, key: &str) -> String {
    format!("{prefix}/{}", href::encode_path_segment(key))
}

/// The row delete form's POST target: `{prefix}/{key}/delete`.
pub(crate) fn delete_action_url(prefix: &str, key: &str) -> String {
    format!(
        "{prefix}/{}/{DELETE_ROUTE_SEGMENT}",
        href::encode_path_segment(key)
    )
}

/// A row action's POST target: `{prefix}/{key}/-/actions/{name}`.
pub(crate) fn row_action_url(prefix: &str, key: &str, name: &str) -> String {
    format!(
        "{prefix}/{}/{DASH_ROUTE_SEGMENT}/{ACTIONS_ROUTE_SEGMENT}/{name}",
        href::encode_path_segment(key)
    )
}

/// A bulk or header action's POST target: `{url}/-/actions/{name}` under a list or a page.
pub(crate) fn bulk_action_url(list_path: &str, name: &str) -> String {
    format!("{list_path}/{DASH_ROUTE_SEGMENT}/{ACTIONS_ROUTE_SEGMENT}/{name}")
}

/// The option search of the input of the action `name` under a list or a page:
/// `{url}/-/actions/{name}/options`.
pub(crate) fn action_options_url(url: &str, name: &str) -> String {
    format!(
        "{}/options",
        bulk_action_url(url.trim_end_matches('/'), name)
    )
}

/// The query parameter marking a page as framed by a creatable choice's dialog: the page renders
/// without the panel's shell, and a create lands on [`picked_url`] instead of the list.
pub(crate) const PICK_PARAM: &str = "pick";

/// The route segment of the page a create in pick mode lands on: `{list_path}/-/picked`.
pub(crate) const PICKED_ROUTE_SEGMENT: &str = "picked";

/// Where a create in pick mode lands: `{list_path}/-/picked?pick=1&key={key}`.
pub(crate) fn picked_url(list_path: &str, key: &str) -> String {
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair(PICK_PARAM, "1")
        .append_pair("key", key)
        .finish();
    format!("{}?{query}", list_route(list_path, PICKED_ROUTE_SEGMENT))
}

/// Whether the request's page is framed by a creatable choice's dialog.
pub(crate) fn picking(cx: &Cx) -> bool {
    topcoat::router::request::original_uri(cx)
        .query()
        .is_some_and(|query| {
            form_urlencoded::parse(query.as_bytes())
                .any(|(key, value)| key == PICK_PARAM && value == "1")
        })
}

/// The list page's export link: `{list_path}/-/export`.
pub(crate) fn export_url(list_path: &str) -> String {
    list_route(list_path, EXPORT_ROUTE_SEGMENT)
}

/// The list page's import link: `{list_path}/-/import`.
pub(crate) fn import_url(list_path: &str) -> String {
    list_route(list_path, IMPORT_ROUTE_SEGMENT)
}

/// The import page's template link: `{list_path}/-/import/template`.
pub(crate) fn import_template_url(list_path: &str) -> String {
    format!("{}/{TEMPLATE_ROUTE_SEGMENT}", import_url(list_path))
}

/// The column picker's POST target: `{list_path}/-/columns`.
pub(crate) fn columns_url(list_path: &str) -> String {
    list_route(list_path, COLUMNS_ROUTE_SEGMENT)
}

/// The bulk form's POST target: `{list_path}/-/bulk-delete`.
pub(crate) fn bulk_delete_url(list_path: &str) -> String {
    list_route(list_path, BULK_DELETE_ROUTE_SEGMENT)
}

/// The query parameter naming the page a write lands on after it commits,
/// in place of the resource's list: `?return=/admin/posts/1`. The panel
/// honours it only for a path under its own prefix.
pub(crate) const RETURN_PARAM: &str = "return";

/// `url`, which carries no query, with `?return={target}`.
pub(crate) fn with_return(url: &str, target: &str) -> String {
    format!("{url}?{RETURN_PARAM}={}", href::encode_query_value(target))
}
