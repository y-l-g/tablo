//! The handlers behind every registered [`Page`]: its `GET`, and its header actions' `POST`.

use std::sync::Arc;

use topcoat::{
    context::Cx,
    router::{
        Body, RouteFuture,
        error::{forbidden, not_found},
        path_param_segment,
    },
    view::BoxView,
};

use super::actions::{input_options, run_header};
use crate::{
    Page,
    page::{MountedPage, mounted_page},
    topcoat_compat::async_page,
};

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
