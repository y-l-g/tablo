//! [`Banner`]: a strip the app shows above every page of a panel, such as a failed payment.

use std::future::Future;

use topcoat::{
    Result,
    context::Cx,
    view::{BoxView, View, ViewExt, view},
};

use crate::topcoat_compat::async_page;

/// A strip above every page of a panel, under the bar that names whom a user acts as, installed
/// with [`Panel::banner`](crate::Panel::banner): what the signed-in user must see wherever they
/// are, such as a failed payment or an expiring trial.
///
/// ```rust
/// use tablo_core::{Banner, Panel, auth};
/// use topcoat::{
///     Result,
///     context::Cx,
///     view::{View, view},
/// };
///
/// struct PaymentFailed;
///
/// impl Banner for PaymentFailed {
///     async fn render(cx: &Cx) -> Result<Option<impl View>> {
///         // A real banner reads the signed-in user's billing state here.
///         let failed = auth::signed_in(cx) && false;
///         Ok(failed.then(|| view! { <div role="alert">"Your last payment failed."</div> }))
///     }
/// }
///
/// Panel::new("app").banner::<PaymentFailed>();
/// ```
pub trait Banner: Send + Sync + 'static {
    /// The strip for this request, or `None` to show nothing. It is asked on every page the
    /// panel's shell frames, signed in or not, and an error fails the page.
    fn render(cx: &Cx) -> impl Future<Output = Result<Option<impl View>>> + Send;
}

/// A [`Banner`] with its type erased: what the shell renders.
pub(crate) type BannerFn = for<'a> fn(&'a Cx) -> BoxView<'a>;

/// `B`'s strip, or nothing.
pub(crate) fn erased<B: Banner>(cx: &Cx) -> BoxView<'_> {
    async_page(async move {
        Ok(match B::render(cx).await? {
            Some(strip) => strip.boxed(),
            None => view! {}.boxed(),
        })
    })
}
