//! The toast a write flashes for the next request: one [`Notification`] in a JSON cookie with
//! hardened attributes, which Topcoat flushes on error responses too (topcoat#408).

use topcoat::{
    context::{Cx, try_request_context},
    cookie::{CookieJar, CookieJarCell, Cookies, cookie_store, cookies},
};

use crate::outcome::Notification;

pub(crate) const COOKIE_NAME: &str = "__Host-tablo_notification";

/// Applies the hardened cookie attributes the flash cookie relies on to writes
/// and removals alike.
fn hardened(jar: &CookieJar) -> impl Cookies + '_ {
    jar.default_path("/")
        .default_http_only(true)
        .default_secure(true)
        .default_same_site(topcoat::cookie::SameSite::Lax)
}

/// Store a notification for the next request (flash).
pub fn set_notification(cx: &Cx, notification: Notification) {
    if try_request_context::<CookieJarCell>(cx).is_none() {
        return;
    }
    if let Err(error) = cookie_store::<Notification, _>(hardened(cookies(cx)), COOKIE_NAME)
        .set(notification)
        .commit()
    {
        tracing::error!(error = %error, "flash notification commit failed");
    }
}

/// Flashes the failure of a write that passed validation but did not land; the
/// title names the operation, never the driver's text, and the message appears
/// on the next panel page.
pub(crate) fn notify_write_failure(cx: &Cx, action: &str) {
    set_notification(
        cx,
        Notification::error(format!("Couldn't {action}"))
            .description("Nothing was changed — try again."),
    );
}

/// Take the notification from the request (if present) and clear it.
pub fn take_notification(cx: &Cx) -> Option<Notification> {
    try_request_context::<CookieJarCell>(cx)?;
    let unparsed = cookie_store::<Notification, _>(hardened(cookies(cx)), COOKIE_NAME);
    match unparsed.parse() {
        Ok(Some(store)) => {
            let notification = store.get();
            store.remove();
            Some(notification)
        }
        // Unreadable value: expire it with no toast.
        Err(_) => {
            cookie_store::<Notification, _>(hardened(cookies(cx)), COOKIE_NAME).remove();
            None
        }
        Ok(None) => None,
    }
}

#[cfg(test)]
mod tests;
