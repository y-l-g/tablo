//! What the panel shows of [`Auth::impersonation`](crate::Auth::impersonation): the banner on every
//! page while a user acts as another, and the [`Impersonate`] action that starts it.

use std::convert::Infallible;

use topcoat::{
    context::Cx,
    icon::icon,
    view::{BoxView, ViewExt, attributes, view},
};

use crate::{Action, ActionDef, Outcome, PanelUser, Places, Resource, WriteError};

/// Acts as the record's user: an [`Action`] for a resource whose model is the panel's user type,
/// placed on its rows and record pages.
///
/// Its button shows only on a user the signed-in user may act as under the panel's
/// [`Impersonation`](crate::Impersonation) rule, and never while they act as someone already.
/// Running it lands on the panel root as that user. The resource's policy must also allow
/// [`Ability::Run`](crate::Ability::Run) on the record.
///
/// ```rust
/// # use tablo_core::{Allow, Impersonate, NoForm, Resource, ResourceDef, auth::AdminUser};
/// struct Users;
///
/// impl Resource for Users {
///     type Model = AdminUser;
///     type Form = NoForm<AdminUser>;
///
///     fn declare() -> ResourceDef<Self> {
///         ResourceDef::new().policy(Allow).action::<Impersonate>()
///     }
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Impersonate;

impl<R: Resource> Action<R> for Impersonate
where
    R::Model: PanelUser,
{
    type Input = ();

    const NAME: &'static str = "impersonate";

    fn declare() -> ActionDef<R> {
        ActionDef::new()
            .places(Places::RECORD)
            .icon(tablo_ui::icons::EYE)
            .label(crate::text!("tablo-impersonation-start"))
            .can_run(|cx, record: &R::Model| crate::auth::may_impersonate(cx, record))
            .success(|_| crate::text!("tablo-impersonation-started"))
    }

    async fn run(
        cx: &Cx,
        records: &[R::Model],
        (): (),
        ex: &mut dyn toasty::Executor,
    ) -> Result<Outcome, WriteError<Infallible>> {
        let [target] = records else {
            return Err(WriteError::declined(crate::text!(
                "tablo-impersonation-one"
            )));
        };
        crate::auth::impersonate(cx, ex, target)
            .await
            .map_err(WriteError::Failed)?;
        Ok(Outcome::done().redirect(crate::auth::panel_prefix(cx)))
    }
}

/// The strip above every page while the signed-in user is acted as: whom, and Stop. Nothing
/// otherwise.
pub(super) fn banner(cx: &Cx) -> BoxView<'_> {
    match crate::auth::signed(cx) {
        Some(signed) if signed.impersonator.is_some() => strip(cx, signed),
        _ => view! {}.boxed(),
    }
}

fn strip<'a>(cx: &'a Cx, signed: &'a crate::auth::SignedIn) -> BoxView<'a> {
    let csrf = crate::csrf::ensure_token(cx);
    let stop = crate::auth::stop_impersonating_url(cx);
    let name = signed.user.display_name().to_owned();
    view! {
        <div
            class="flex flex-wrap items-center justify-between gap-2 border-b border-warning/40 \
                       bg-warning/15 px-4 py-2 text-sm text-foreground"
            role="status"
            data-impersonating=(signed.user.user_id())
        >
            <span>(crate::t!(cx, "tablo-impersonation-banner", name = name))</span>
            <form method="post" action=(stop)>
                (crate::csrf::field(cx, &csrf))
                tablo_ui::button(
                    variant: tablo_ui::ButtonVariant::Outline,
                    size: tablo_ui::ButtonSize::Sm,
                    attrs: attributes! { type="submit" },
                    icon(data: tablo_ui::icons::LOG_OUT)
                    (crate::t!(cx, "tablo-impersonation-stop"))
                )
            </form>
        </div>
    }
    .boxed()
}
