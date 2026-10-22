//! The mail auth sends: the [`AuthMail`] configuration every mailed link shares, the links it
//! builds, and the headers a page carrying a token answers with.

use std::{future::Future, sync::Arc};

use topcoat::{
    context::Cx,
    mail::{Mail, Mailbox},
    router::{request::uri, response::Response},
};

use crate::auth::realm::{Realm, current_realm};

/// Where a panel's auth mail comes from and where its links point, installed with
/// [`Auth::mail`](super::Auth::mail): the sender, and the absolute URL every link starts with.
///
/// ```rust
/// use tablo_core::{Auth, AuthMail, PasswordAuth, PasswordReset};
/// use topcoat::mail::Mailbox;
///
/// # fn main() -> Result<(), topcoat::mail::AddressError> {
/// let auth = Auth::password()
///     .mail(AuthMail::new(
///         Mailbox::named("Example", "no-reply@example.com")?,
///         "https://admin.example.com",
///     ))
///     .password_reset(PasswordReset::new(PasswordAuth));
/// # let _ = auth;
/// # Ok(())
/// # }
/// ```
///
/// The base URL is configuration, not the request's `Host` header, which the client chooses: a
/// link built from it could point a victim's email at another host. Mounting refuses a base URL
/// that is not an absolute `http` or `https` URL, or that carries a query or fragment, and a
/// router without a [`MailConfig`](topcoat::mail::MailConfig), which the app registers with
/// `.mail(..)` before mounting the panel.
pub struct AuthMail {
    from: Mailbox,
    base_url: String,
}

impl AuthMail {
    /// Mails from `from`, with links that start with `base_url`, such as
    /// `https://admin.example.com`: the scheme and host the app answers on, before the panel's
    /// prefix.
    #[must_use]
    pub fn new(from: Mailbox, base_url: impl Into<String>) -> Self {
        Self {
            from,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    /// Whether the base URL is an absolute `http` or `https` URL without a query or fragment.
    pub(crate) fn base_url_is_absolute(&self) -> bool {
        self.base_url.parse::<http::Uri>().is_ok_and(|uri| {
            matches!(uri.scheme_str(), Some("http" | "https"))
                && uri.authority().is_some()
                && uri.query().is_none()
        }) && !self.base_url.contains('#')
    }

    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The absolute link to `path` on this app, with `token` in its query.
    pub(super) fn link(&self, path: &str, token: &topcoat::session::Token) -> String {
        let query = form_urlencoded::Serializer::new(String::new())
            .append_pair(TOKEN_FIELD, &token.encode())
            .finish();
        format!("{}{path}?{query}", self.base_url)
    }

    /// A mail from this sender to `address`, titled `subject`, reading `text`.
    pub(super) fn compose(
        &self,
        address: &str,
        subject: String,
        text: String,
    ) -> topcoat::Result<Mail> {
        Ok(Mail::builder()
            .from(self.from.clone())
            .to([Mailbox::new(address)?])
            .subject(subject)
            .text(text)
            .build())
    }
}

impl std::fmt::Debug for AuthMail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthMail")
            .field("from", &self.from.address())
            .field("base_url", &self.base_url)
            .finish()
    }
}

/// The key a mailed link carries its token under, in its query and in the form it opens.
pub(super) const TOKEN_FIELD: &str = "token";

/// The token the request's query carries, or empty.
pub(super) fn token_from_query(cx: &Cx) -> String {
    uri(cx)
        .query()
        .and_then(|query| {
            form_urlencoded::parse(query.as_bytes())
                .find(|(key, _)| key == TOKEN_FIELD)
                .map(|(_, value)| value.into_owned())
        })
        .unwrap_or_default()
}

/// Runs `send` after the response, with the request's panel and its [`AuthMail`], logging a
/// failure as `what`: waiting on the database or the mail server, or answering differently,
/// would tell the visitor whether an account exists.
pub(super) fn after_response<F, Fut>(cx: &Cx, what: &'static str, send: F)
where
    F: FnOnce(Cx, Arc<Realm>) -> Fut + Send + 'static,
    Fut: Future<Output = topcoat::Result<()>> + Send + 'static,
{
    let Some(panel) = current_realm(cx).map(Arc::clone) else {
        return;
    };
    let cx = cx.clone();
    tokio::spawn(async move {
        let prefix = panel.prefix.clone();
        if let Err(error) = send(cx, panel).await {
            tracing::error!(error = %error, panel = %prefix, "{what} not sent");
        }
    });
}

/// `response` with `Referrer-Policy: no-referrer` and `Cache-Control: no-store`: a page whose
/// URL or form carries a token.
pub(super) fn private(mut response: Response) -> topcoat::Result<Response> {
    let headers = response.headers_mut();
    headers.insert(
        http::header::REFERRER_POLICY,
        http::HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        http::header::CACHE_CONTROL,
        http::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

/// The mistakes in what a panel's auth mails: a feature that mails links without
/// [`Auth::mail`](super::Auth::mail), a base URL that is not absolute, and a router that cannot
/// send mail. `has_mail` says whether the router can.
pub(super) fn declaration_errors(
    auth: &super::Auth,
    has_mail: bool,
) -> Vec<crate::DeclarationErrorKind> {
    use crate::{AuthMailFault, DeclarationErrorKind};

    let mailing = auth.mailing();
    if auth.is_disabled() || mailing.is_empty() {
        return Vec::new();
    }
    let Some(mail) = auth.mail.as_ref() else {
        return mailing
            .into_iter()
            .map(|feature| DeclarationErrorKind::AuthMail(AuthMailFault::Unset { feature }))
            .collect();
    };
    let mut faults = Vec::new();
    if !mail.base_url_is_absolute() {
        faults.push(AuthMailFault::BaseUrl(mail.base_url().to_string()));
    }
    if !has_mail {
        faults.push(AuthMailFault::MissingMail);
    }
    faults
        .into_iter()
        .map(DeclarationErrorKind::AuthMail)
        .collect()
}
