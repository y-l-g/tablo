//! The handlers behind every registered [`Page`]: its `GET`, and its header actions' `POST`; and
//! the page as the request's panel mounted it, with the header buttons it renders.

use std::{any::TypeId, sync::Arc};

use topcoat::{
    context::Cx,
    router::{
        Body, RouteFuture,
        error::{forbidden, not_found},
        path_param_segment,
    },
    view::{BoxView, ViewExt},
};

use super::actions::{input_options, run_header};
use crate::{Page, page::MountedPage, topcoat_compat::async_page};

/// `P` as the request's panel mounted it, if it did.
pub(crate) fn mounted_page<P: Page>(cx: &Cx) -> Option<&Arc<MountedPage>> {
    super::state::current(cx)?
        .mounted_pages
        .get(&TypeId::of::<P>())
}

/// The header buttons of the page `P` the request may run, for its
/// [`page_actions`](tablo_ui::page_actions):
///
/// ```rust
/// # use tablo_core::{HeaderAction, Page, PageDef};
/// # use topcoat::{Result, context::Cx, view::{View, view}};
/// # struct ClearCache;
/// # impl HeaderAction for ClearCache {
/// #     type Input = ();
/// #     const NAME: &'static str = "clear-cache";
/// #     async fn run(_: &Cx, _: (), _: &mut dyn toasty::Executor) -> Result<tablo_core::Outcome, tablo_core::WriteError> { Ok(tablo_core::Outcome::done()) }
/// # }
/// struct MaintenancePage;
///
/// impl Page for MaintenancePage {
///     fn declare() -> PageDef {
///         PageDef::new().header_action::<ClearCache>()
///     }
///
///     async fn render(cx: &Cx) -> Result<impl View> {
///         Ok(view! {
///             cx =>
///             tablo_ui::page(
///                 tablo_ui::page_header(
///                     tablo_ui::page_title("Maintenance")
///                     tablo_ui::page_actions((tablo_core::header_action_buttons::<Self>(cx)))
///                 )
///             )
///         })
///     }
/// }
/// ```
///
/// It renders nothing outside a request to `P`'s panel or for a request
/// [`PageDef::access`](crate::PageDef::access) refuses, and no button for an action whose
/// [`can_run`](crate::HeaderAction::can_run) refuses the request.
pub fn header_action_buttons<P: Page>(cx: &Cx) -> BoxView<'_> {
    let Some(page) = mounted_page::<P>(cx).filter(|page| page.admits(cx)) else {
        return ().boxed();
    };
    let Some(url) = super::url::page::<P>(cx) else {
        return ().boxed();
    };
    super::header_bar(cx, &url, &page.header_actions, |_| true).render(cx)
}

/// `P` as the request's panel mounted it, once the auth re-check every panel handler runs and the
/// page's [`PageDef::access`](crate::PageDef::access) admit the request.
fn admitted<P: Page>(cx: &Cx) -> topcoat::Result<&Arc<MountedPage>> {
    crate::auth::guard(cx)?;
    let page = mounted_page::<P>(cx).ok_or_else(not_found)?;
    if !page.admits(cx) {
        return Err(forbidden().into());
    }
    Ok(page)
}

/// `GET` for a registered page: [`admitted`], then the page's own body.
pub(super) fn page_handler<P: Page>(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        admitted::<P>(cx)?;
        P::render(cx).await
    })
}

/// `POST {url}/-/actions/{name}` for a registered page: the same checks as its `GET`, then the
/// header action's [`can_run`](crate::HeaderAction::can_run), landing back on the page.
pub(super) fn page_action<P: Page>(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let page = admitted::<P>(cx)?;
        let Some(action) = page
            .header_actions
            .find(path_param_segment(cx, "action"))
            .copied()
        else {
            return Err(not_found().into());
        };
        if !(action.can_run)(cx) {
            return Err(forbidden().into());
        }
        let Some(home) = crate::panel::url::page::<P>(cx) else {
            return Err(not_found().into());
        };
        run_header(cx, body, action, &home, None).await
    })
}

/// `GET {url}/-/actions/{name}/options` for a registered page: the option search of a header
/// action's input, behind the same checks as the action's `POST`.
pub(super) fn page_action_options<P: Page>(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let page = admitted::<P>(cx)?;
        let Some(action) = page.header_actions.find(path_param_segment(cx, "action")) else {
            return Err(not_found().into());
        };
        if !(action.can_run)(cx) {
            return Err(forbidden().into());
        }
        input_options(cx, action.input).await
    })
}

#[cfg(test)]
mod tests;
