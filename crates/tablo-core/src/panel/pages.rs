//! The handler behind every registered [`Page`].

use topcoat::{
    context::Cx,
    router::Body,
    view::{BoxView, HoistView, internal::ThenView},
};

use super::gate::enforce_auth;
use crate::Page;

/// `GET` for a registered page: the auth re-check every panel handler runs,
/// then the page's own body.
pub(super) fn page_handler<P: Page>(cx: &Cx, _body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::new(async move {
        enforce_auth(cx)?;
        P::render(cx).await
    })))
}

#[cfg(test)]
mod tests;
