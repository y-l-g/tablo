//! The handler behind every registered [`Page`].

use topcoat::{context::Cx, router::Body, view::BoxView};

use crate::{Page, topcoat_compat::async_page};

/// `GET` for a registered page: the auth re-check every panel handler runs,
/// then the page's own body.
pub(super) fn page_handler<P: Page>(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        crate::auth::guard(cx)?;
        P::render(cx).await
    })
}

#[cfg(test)]
mod tests;
