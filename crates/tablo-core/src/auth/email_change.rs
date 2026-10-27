//! Email-change confirmation: [`request_email_change`], which a profile editor calls to keep the
//! old address until the new one proves itself, and the `{prefix}/email-change` routes its two
//! links open.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use topcoat::{
    context::{Cx, try_context},
    router::{
        Body, RouteFuture,
        error::see_other,
        response::{IntoResponse, Response},
    },
    session::Token,
    view::{ViewExt, attributes, view},
};

use super::{
    PanelUser, SignedIn, infrastructure_failure,
    login::login_url,
    mail::{TOKEN_FIELD, after_response, private, token_from_query},
    page::{AuthPage, render},
    panel_root,
    session::{credential_panels, end_credentials, resolve, start_once},
    token::{self, AuthToken, CREDENTIALS, REVOCATION, TokenPurpose, run_serializable},
    verification::VERIFICATION_LIFETIME,
};
use crate::{
    auth::realm::{Realm, current_realm, hosted, panel_prefix},
    flash::set_notification,
    outcome::Notification,
};

/// How long the old address may undo a change: long enough to notice a mail while away.
const CANCEL_LIFETIME: Duration = Duration::from_hours(24 * 7);

/// What [`request_email_change`] did with a new address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum EmailChange {
    /// The panel mails the new address a link that stores it, and the old one a link that
    /// cancels: keep the old address until the first is opened.
    Pending,
    /// The panel verifies no address: store the new one now.
    Immediate,
}

/// Asks the account `user` to confirm `email` as its new address before it changes, when the
/// request's panel installs [`Auth::email_verification`](super::Auth::email_verification); call
/// it from [`ProfileEditor::save`](super::ProfileEditor::save) through the save's transaction
/// `ex`, in place of writing the address.
///
/// It answers [`EmailChange::Pending`] once it has recorded the change: the panel mails the new
/// address a link that stores it, verified, and the old address a link that keeps it, both after
/// the save commits. Confirming the first is a credential change. The second works for a week,
/// before or after the change goes through: it restores the old address and ends every session
/// the account holds. It answers [`EmailChange::Immediate`] on a panel that verifies no address,
/// and writes nothing: store the address yourself.
///
/// The link proves only that whoever asked reads the new mailbox, and a stolen session can name
/// one its thief reads: ask for the current password before calling it, as
/// [`PasswordAuth`](super::PasswordAuth)'s profile does.
///
/// ```rust
/// # use tablo_core::{PanelUser, auth::{self, EmailChange}};
/// # use topcoat::context::Cx;
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Staff { #[key] #[auto] id: uuid::Uuid, email: String, name: String }
/// # impl PanelUser for Staff {
/// #     fn user_id(&self) -> String { self.id.to_string() }
/// #     fn display_name(&self) -> &str { &self.name }
/// #     fn can_access_panel(&self, _panel: &str) -> bool { true }
/// #     fn email(&self) -> Option<&str> { Some(&self.email) }
/// # }
/// async fn save_email(
///     cx: &Cx,
///     staff: &Staff,
///     email: String,
///     ex: &mut dyn toasty::Executor,
/// ) -> topcoat::Result<()> {
///     if auth::request_email_change(cx, ex, staff, &email).await? == EmailChange::Immediate {
///         Staff::filter_by_id(staff.id)
///             .update()
///             .email(email)
///             .exec(ex)
///             .await?;
///     }
///     Ok(())
/// }
/// ```
///
/// # Errors
///
/// The opaque outage when the database cannot answer.
pub async fn request_email_change(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    user: &dyn PanelUser,
    email: &str,
) -> topcoat::Result<EmailChange> {
    if !current_realm(cx).is_some_and(|panel| panel.auth.verification().is_some()) {
        return Ok(EmailChange::Immediate);
    }
    let user_id = user.user_id();
    let confirm = token::issue(
        cx,
        &mut *ex,
        TokenPurpose::EmailChange,
        &user_id,
        Some(email.to_string()),
        VERIFICATION_LIFETIME,
    )
    .await?;
    let old = user.email().map(str::to_string);
    // The cancel link carries the address it restores; an account without one has none to keep.
    let cancel = match &old {
        Some(old) => Some(
            token::issue(
                cx,
                ex,
                TokenPurpose::EmailChangeCancel,
                &user_id,
                Some(old.clone()),
                CANCEL_LIFETIME,
            )
            .await?,
        ),
        None => None,
    };
    let request = Request {
        old,
        new: email.to_string(),
        confirm,
        cancel,
    };
    match try_context::<Requested>(cx) {
        // Mailed once the save commits: a rolled-back save mails nothing.
        Some(requested) => requested.push(request),
        None => mail(cx, request),
    }
    Ok(EmailChange::Pending)
}

/// One recorded change: the addresses, and the tokens of its two links.
struct Request {
    old: Option<String>,
    new: String,
    confirm: Token,
    cancel: Option<Token>,
}

/// The changes a request recorded, which its handler mails once it commits.
#[derive(Clone, Default)]
pub(super) struct Requested(Arc<Mutex<Vec<Request>>>);

impl Requested {
    fn push(&self, request: Request) {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(request);
    }

    /// Mails every recorded change, and answers whether there was one.
    pub(super) fn mail(&self, cx: &Cx) -> bool {
        let requests =
            std::mem::take(&mut *self.0.lock().unwrap_or_else(|poison| poison.into_inner()));
        let any = !requests.is_empty();
        for request in requests {
            mail(cx, request);
        }
        any
    }
}

/// Mails `request`'s two links after the response.
fn mail(cx: &Cx, request: Request) {
    after_response(cx, "email change links", move |cx, panel| async move {
        let mail = panel
            .auth
            .auth_mail()
            .ok_or_else(topcoat::router::error::not_found)?;
        let brand = hosted(&cx)?.brand_name(&cx);
        let confirm = mail.link(&confirm_url(&cx), &request.confirm);
        let new = &request.new;
        let message = mail.compose(
            new,
            crate::t!(
                &cx,
                "tablo-email-change-mail-subject",
                brand = brand.clone()
            ),
            crate::t!(
                &cx,
                "tablo-email-change-mail-body",
                new = new,
                brand = brand.clone(),
                link = confirm
            ),
        )?;
        topcoat::mail::send(&cx, message).await?;
        if let (Some(old), Some(cancel)) = (&request.old, &request.cancel) {
            let cancel = mail.link(&cancel_url(&cx), cancel);
            let message = mail.compose(
                old,
                crate::t!(
                    &cx,
                    "tablo-email-change-notice-subject",
                    brand = brand.clone()
                ),
                crate::t!(
                    &cx,
                    "tablo-email-change-notice-body",
                    brand = brand.clone(),
                    new = new,
                    old = old,
                    link = cancel
                ),
            )?;
            topcoat::mail::send(&cx, message).await?;
        }
        Ok(())
    });
}

/// Where the link to the new address opens: `{prefix}/email-change/confirm`.
pub(crate) fn confirm_url(cx: &Cx) -> String {
    format!("{}/email-change/confirm", panel_prefix(cx))
}

/// Where the link to the old address opens: `{prefix}/email-change/cancel`.
pub(crate) fn cancel_url(cx: &Cx) -> String {
    format!("{}/email-change/cancel", panel_prefix(cx))
}

/// A link the account may still use: its live token, the account, and the signed-in user of this
/// browser when it is that account.
struct Opened {
    row: AuthToken,
    account: Arc<dyn PanelUser>,
    signed: Option<SignedIn>,
}

/// The live token for `purpose` that `encoded` names, on an account that may still enter the
/// panel, or on any account for a cancel link, or `None`.
async fn opened(
    cx: &Cx,
    panel: &Arc<Realm>,
    purpose: TokenPurpose,
    encoded: &str,
) -> topcoat::Result<Option<Opened>> {
    let Some(authenticator) = panel.auth.authenticator() else {
        return Ok(None);
    };
    let mut db = crate::db::db(cx)?;
    let Some(row) = token::live(cx, &mut db, purpose, encoded).await? else {
        return Ok(None);
    };
    let account = authenticator
        .find_by_id(cx, &row.user_id)
        .await
        .map_err(infrastructure_failure)?
        // A cancel link only restores the old address and ends sessions: a rule that reads the
        // address must not refuse it to the account the change shut out.
        .filter(|account| {
            purpose == TokenPurpose::EmailChangeCancel || account.can_access_panel(&panel.prefix)
        });
    let Some(account) = account else {
        return Ok(None);
    };
    // The routes answer signed out, so the gate resolved no session for them.
    let signed = resolve(cx, panel, authenticator)
        .await?
        .filter(|signed| signed.user.user_id() == row.user_id);
    Ok(Some(Opened {
        row,
        account,
        signed,
    }))
}

/// The page a link opens: what posting it does, and the button that does it. Opening a link
/// changes nothing, so a mail scanner that fetches it changes nothing either.
async fn link_page(
    cx: &Cx,
    title: String,
    description: String,
    button: String,
) -> topcoat::Result<Response> {
    let csrf = crate::csrf::ensure_token(cx);
    let action = topcoat::router::request::uri(cx).path().to_string();
    let token = token_from_query(cx);
    let form = view! {
        cx =>
        <form method="post" action=(action) data-email-change="">
            (crate::csrf::field(cx, &csrf))
            <input type="hidden" name=(TOKEN_FIELD) value=(token)>
            tablo_ui::button(
                variant: tablo_ui::ButtonVariant::Primary,
                attrs: attributes! { type="submit" class="w-full" },
                (button)
            )
        </form>
    }
    .boxed();
    let page = render(
        cx,
        AuthPage {
            title,
            description,
            status: None,
            form,
            switch: None,
            note: None,
        },
    )
    .await?;
    private(page.single().await?.into_response(cx)?)
}

/// The panel's verification and the link `encoded` names for `purpose`, or the page an unusable
/// link lands on.
async fn link(
    cx: &Cx,
    purpose: TokenPurpose,
    encoded: &str,
) -> topcoat::Result<Result<(Arc<Realm>, Opened), Response>> {
    let panel = current_realm(cx).ok_or_else(topcoat::router::error::not_found)?;
    if panel.auth.verification().is_none() {
        return Err(topcoat::router::error::not_found().into());
    }
    match opened(cx, panel, purpose, encoded).await? {
        Some(opened) => Ok(Ok((Arc::clone(panel), opened))),
        None => Ok(Err(super::verification::invalid_link(cx).await?)),
    }
}

/// The token a link's form posts.
async fn posted_token(cx: &Cx, body: Body) -> topcoat::Result<String> {
    let form = crate::auth::input::read_form(cx, body).await?;
    crate::csrf::verify(cx, form.last(crate::csrf::FIELD_NAME))?;
    Ok(form.last(TOKEN_FIELD).unwrap_or_default().to_string())
}

/// `GET {prefix}/email-change/confirm?token=…` — the button that confirms the new address.
pub(super) fn confirm_page(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let (_, opened) = match link(cx, TokenPurpose::EmailChange, &token_from_query(cx)).await? {
            Ok(link) => link,
            Err(invalid) => return Ok(invalid),
        };
        let email = opened.row.email.unwrap_or_default();
        link_page(
            cx,
            crate::t!(cx, "tablo-email-change-confirm-title"),
            crate::t!(cx, "tablo-email-change-confirm-description", email = email),
            crate::t!(cx, "tablo-confirm"),
        )
        .await
    })
}

/// `POST {prefix}/email-change/confirm` — store the new address, verified, and end the account's
/// sessions and sign-in links on each panel signing in its user type, keeping this browser's when
/// it is the account's.
pub(super) fn confirm_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let encoded = posted_token(cx, body).await?;
        let (panel, Opened { row, signed, .. }) =
            match link(cx, TokenPurpose::EmailChange, &encoded).await? {
                Ok(link) => link,
                Err(invalid) => return Ok(invalid),
            };
        let verifier = panel
            .auth
            .verification()
            .ok_or_else(topcoat::router::error::not_found)?
            .verifier();
        let email = row.email.clone().unwrap_or_default();
        let user_type = verifier.user_type();
        let mut started = None;
        let landing = run_serializable!(cx, |tx| async {
            // Read again inside the transaction: of two requests racing on one link, one commits.
            if token::live(cx, tx, TokenPurpose::EmailChange, &encoded)
                .await?
                .is_none()
            {
                return Ok(None);
            }
            token::spend(tx, &row).await?;
            verifier
                .store_verified_email(cx, &row.user_id, &email, tx)
                .await
                .map_err(infrastructure_failure)?;
            Ok(Some(match &signed {
                Some(signed) => {
                    let session = start_once(cx, &mut started).await?;
                    super::session::replace_sessions(cx, tx, signed, user_type, &session).await?;
                    panel_root(cx)
                }
                None => {
                    let panels = credential_panels(cx, user_type, &panel);
                    end_credentials(tx, &row.user_id, &panels, CREDENTIALS).await?;
                    login_url(cx)
                }
            }))
        })?;
        let Some(landing) = landing else {
            return super::verification::invalid_link(cx).await;
        };
        set_notification(
            cx,
            Notification::success(crate::t!(cx, "tablo-email-change-done")).description(crate::t!(
                cx,
                "tablo-email-change-done-description",
                email = email
            )),
        );
        private(see_other(landing).into_response(cx)?)
    })
}

/// `GET {prefix}/email-change/cancel?token=…` — the button that keeps the old address.
pub(super) fn cancel_page(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let (_, opened) =
            match link(cx, TokenPurpose::EmailChangeCancel, &token_from_query(cx)).await? {
                Ok(link) => link,
                Err(invalid) => return Ok(invalid),
            };
        let email = opened.row.email.unwrap_or_default();
        link_page(
            cx,
            crate::t!(cx, "tablo-email-change-keep-title"),
            crate::t!(cx, "tablo-email-change-keep-description", email = email),
            crate::t!(cx, "tablo-email-change-keep"),
        )
        .await
    })
}

/// `POST {prefix}/email-change/cancel` — keep the old address, restoring it when the change went
/// through, and end every session, sign-in link and pending change the account holds on each panel
/// signing in its user type: whoever asked for the change may hold one.
pub(super) fn cancel_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let encoded = posted_token(cx, body).await?;
        let (panel, Opened { row, account, .. }) =
            match link(cx, TokenPurpose::EmailChangeCancel, &encoded).await? {
                Ok(link) => link,
                Err(invalid) => return Ok(invalid),
            };
        let verifier = panel
            .auth
            .verification()
            .ok_or_else(topcoat::router::error::not_found)?
            .verifier();
        let old = row.email.clone().unwrap_or_default();
        let cancelled = run_serializable!(cx, |tx| async {
            if token::live(cx, tx, TokenPurpose::EmailChangeCancel, &encoded)
                .await?
                .is_none()
            {
                return Ok(false);
            }
            token::spend(tx, &row).await?;
            if account.email() != Some(old.as_str()) {
                // The change went through: the old address, which this link proved, comes back.
                verifier
                    .store_verified_email(cx, &row.user_id, &old, tx)
                    .await
                    .map_err(infrastructure_failure)?;
            }
            let panels = credential_panels(cx, verifier.user_type(), &panel);
            end_credentials(tx, &row.user_id, &panels, REVOCATION).await?;
            Ok(true)
        })?;
        if !cancelled {
            return super::verification::invalid_link(cx).await;
        }
        // The cancelling browser may hold one of the ended sessions; its cookie goes too.
        topcoat::session::stop(cx).await?;
        set_notification(
            cx,
            Notification::success(crate::t!(cx, "tablo-email-change-kept")).description(crate::t!(
                cx,
                "tablo-email-change-kept-description",
                email = old
            )),
        );
        private(see_other(login_url(cx)).into_response(cx)?)
    })
}
