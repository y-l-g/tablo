//! Email-change confirmation: [`request_email_change`], which a profile editor calls to keep the
//! old address until the new one proves itself, and the `{prefix}/email-change` routes its two
//! links open.

use std::sync::{Arc, Mutex};

use topcoat::{
    context::{Cx, try_request_context},
    router::{Body, RouteFuture, error::see_other, response::IntoResponse},
    session::Token,
};

use super::{
    PanelUser, SignedIn, infrastructure_failure,
    login::login_url,
    mail::{after_response, private, token_from_query},
    panel_root,
    session::{credential_panels, end_credentials, resolve},
    token::{self, AuthToken, TokenPurpose, serializable},
    verification::VERIFICATION_LIFETIME,
};
use crate::{
    notification::{Notification, set_notification},
    panel::{
        Panel, panel_prefix,
        state::{PanelState, current},
    },
};

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
/// address a link that stores it, verified, and the old address a link that cancels it, both
/// after the save commits. Opening the first is a credential change, and the second ends every
/// session the account holds. It answers [`EmailChange::Immediate`] on a panel that verifies no
/// address, and writes nothing: store the address yourself.
///
/// ```rust
/// # use tablo_core::{PanelUser, auth::{self, EmailChange}};
/// # use topcoat::context::Cx;
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Staff { #[key] #[auto] id: uuid::Uuid, email: String, name: String }
/// # impl PanelUser for Staff {
/// #     fn user_id(&self) -> String { self.id.to_string() }
/// #     fn display_name(&self) -> &str { &self.name }
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
    if !current(cx).is_some_and(|panel| panel.auth.verification().is_some()) {
        return Ok(EmailChange::Immediate);
    }
    let user_id = user.user_id();
    let new = Some(email.to_string());
    let confirm = token::issue(
        cx,
        &mut *ex,
        TokenPurpose::EmailChange,
        &user_id,
        new.clone(),
        VERIFICATION_LIFETIME,
    )
    .await?;
    let cancel = token::issue(
        cx,
        ex,
        TokenPurpose::EmailChangeCancel,
        &user_id,
        new,
        VERIFICATION_LIFETIME,
    )
    .await?;
    let request = Request {
        old: user.email().map(str::to_string),
        new: email.to_string(),
        confirm,
        cancel,
    };
    match try_request_context::<Requested>(cx) {
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
    cancel: Token,
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
        let brand = Panel::brand_name(&cx);
        let confirm = mail.link(&confirm_url(&cx), &request.confirm);
        let new = &request.new;
        let message = mail.compose(
            new,
            format!("Confirm your new {brand} email address"),
            format!(
                "Open this link to make {new} the address of your {brand} account. It works \
                 once, for the next 24 hours:\n\n{confirm}\n\n\
                 If you did not ask for the change, ignore this email.\n"
            ),
        )?;
        topcoat::mail::send(&cx, message).await?;
        if let Some(old) = &request.old {
            let cancel = mail.link(&cancel_url(&cx), &request.cancel);
            let message = mail.compose(
                old,
                format!("Your {brand} email address is changing"),
                format!(
                    "Someone asked to change the address of your {brand} account to {new}.\n\n\
                     If it was not you, open this link to cancel the change and sign every \
                     browser out, then reset your password:\n\n{cancel}\n"
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

/// The live token for `purpose` the request's query names, the account it acts on, and the
/// signed-in user of this browser when it is that account.
async fn opened(
    cx: &Cx,
    panel: &Arc<PanelState>,
    purpose: TokenPurpose,
) -> topcoat::Result<Option<(AuthToken, Option<SignedIn>)>> {
    let Some(authenticator) = panel.auth.authenticator() else {
        return Ok(None);
    };
    let mut db = crate::db::db(cx);
    let Some(row) = token::live(cx, &mut db, purpose, &token_from_query(cx)).await? else {
        return Ok(None);
    };
    let account = authenticator
        .find_by_id(cx, &row.user_id)
        .await
        .map_err(infrastructure_failure)?;
    if !account.is_some_and(|account| account.can_access_panel()) {
        return Ok(None);
    }
    // The routes answer signed out, so the gate resolved no session for them.
    let signed = resolve(cx, panel, authenticator)
        .await?
        .filter(|signed| signed.user.user_id() == row.user_id);
    Ok(Some((row, signed)))
}

/// `GET {prefix}/email-change/confirm?token=…` — store the new address, verified, and end the
/// account's sessions and links on each panel signing in its user type, keeping this browser's
/// when it is the account's.
pub(super) fn confirm_page(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let panel = current(cx).ok_or_else(topcoat::router::error::not_found)?;
        let verification = panel
            .auth
            .verification()
            .ok_or_else(topcoat::router::error::not_found)?;
        let Some((row, signed)) = opened(cx, panel, TokenPurpose::EmailChange).await? else {
            return super::verification::invalid_link(cx).await;
        };
        let email = row.email.clone().unwrap_or_default();
        let mut db = crate::db::db(cx);
        let mut tx = serializable(&mut db).await?;
        // Read again inside the transaction: of two requests racing on one link, one commits.
        let encoded = token_from_query(cx);
        if token::live(cx, &mut tx, TokenPurpose::EmailChange, &encoded)
            .await?
            .is_none()
        {
            return super::verification::invalid_link(cx).await;
        }
        verification
            .verifier()
            .store_verified_email(cx, &row.user_id, &email, &mut tx)
            .await
            .map_err(infrastructure_failure)?;
        let user_type = verification.verifier().user_type().0;
        let landing = match &signed {
            Some(signed) => {
                let session = topcoat::session::start(cx).await?;
                super::session::replace_sessions(cx, &mut tx, signed, user_type, &session).await?;
                panel_root(cx)
            }
            None => {
                let panels = credential_panels(cx, user_type, panel);
                end_credentials(&mut tx, &row.user_id, &panels).await?;
                login_url(cx)
            }
        };
        tx.commit().await.map_err(infrastructure_failure)?;
        set_notification(
            cx,
            Notification::success("Email changed")
                .description(format!("You now sign in as {email}.")),
        );
        private(see_other(landing).into_response(cx)?)
    })
}

/// `GET {prefix}/email-change/cancel?token=…` — drop the pending change and end every session
/// and link the account holds on each panel signing in its user type: whoever asked for the
/// change may hold one.
pub(super) fn cancel_page(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let panel = current(cx).ok_or_else(topcoat::router::error::not_found)?;
        let verification = panel
            .auth
            .verification()
            .ok_or_else(topcoat::router::error::not_found)?;
        let Some((row, _)) = opened(cx, panel, TokenPurpose::EmailChangeCancel).await? else {
            return super::verification::invalid_link(cx).await;
        };
        let mut db = crate::db::db(cx);
        let mut tx = serializable(&mut db).await?;
        let panels = credential_panels(cx, verification.verifier().user_type().0, panel);
        end_credentials(&mut tx, &row.user_id, &panels).await?;
        tx.commit().await.map_err(infrastructure_failure)?;
        // The cancelling browser may hold one of the ended sessions; its cookie goes too.
        topcoat::session::stop(cx).await?;
        set_notification(
            cx,
            Notification::success("Email change cancelled").description(
                "Your address stays the same and every browser is signed out. Reset your \
                 password if someone else knows it.",
            ),
        );
        private(see_other(login_url(cx)).into_response(cx)?)
    })
}
