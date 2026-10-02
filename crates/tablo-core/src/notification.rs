//! Transient user-visible message produced by an `Action`'s result and rendered
//! in the `Panel` shell; stored as a JSON flash cookie with hardened attributes
//! that Topcoat flushes on error responses too (topcoat#408).

use serde::{Deserialize, Serialize};
use tablo_ui::{
    icons, toast, toast_close, toast_content, toast_description, toast_icon, toast_title,
};
use topcoat::{
    context::{Cx, try_request_context},
    cookie::{CookieJar, CookieJarCell, Cookies, cookie_store, cookies},
    icon::icon,
    runtime::{Signal, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, attributes, view},
};

/// The kind of notification (status).
///
/// The serde tokens are lowercase so the JSON cookie reads naturally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationStatus {
    Success,
    Error,
    Info,
    Warning,
}

impl NotificationStatus {
    /// The lowercase token, shared with the flash cookie and `data-type`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Error => "error",
            Self::Info => "info",
            Self::Warning => "warning",
        }
    }
}

/// A transient message shown after a mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub status: NotificationStatus,
    pub title: String,
    /// Supporting line under the title; `None` keeps the cookie wire format so
    /// older cookies still decode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Notification {
    pub fn success(title: impl Into<String>) -> Self {
        Self::new(NotificationStatus::Success, title)
    }

    pub fn error(title: impl Into<String>) -> Self {
        Self::new(NotificationStatus::Error, title)
    }

    pub fn info(title: impl Into<String>) -> Self {
        Self::new(NotificationStatus::Info, title)
    }

    pub fn warning(title: impl Into<String>) -> Self {
        Self::new(NotificationStatus::Warning, title)
    }

    fn new(status: NotificationStatus, title: impl Into<String>) -> Self {
        Self {
            status,
            title: title.into(),
            description: None,
        }
    }

    /// Attach the supporting line under the title.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

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

/// Renders one notification as the toast, with `attrs` merged onto its surface.
pub async fn render_notification<'a>(
    cx: &'a Cx,
    notification: Notification,
    attrs: Attributes,
) -> topcoat::Result<BoxView<'a>> {
    let Notification {
        status,
        title,
        description,
    } = notification;
    let icon_data = match status {
        NotificationStatus::Success => icons::CIRCLE_CHECK,
        NotificationStatus::Error => icons::OCTAGON_X,
        NotificationStatus::Info => icons::INFO,
        NotificationStatus::Warning => icons::TRIANGLE_ALERT,
    };
    let icon_class = if status == NotificationStatus::Error {
        "size-4 text-destructive"
    } else {
        "size-4"
    };
    let status = status.as_str();
    Ok(view! {
        cx =>
        toast(
            attrs: attributes! { data-type=(status) (attrs) },
            toast_icon(icon(data: icon_data, attrs: attributes! { class=(icon_class) }))
            toast_content(
                toast_title((title))
                if let Some(description) = description {
                    toast_description((description))
                }
            )
            toast_close()
        )
    }
    .boxed())
}

/// Owns the signals that mount a [`Notification`] in place without navigation.
#[derive(Clone)]
pub struct LiveToast {
    /// Sonner status token; empty means no toast mounted.
    pub status: Signal<String>,
    /// The toast title.
    pub title: Signal<String>,
    /// The optional supporting line; empty renders no description.
    pub description: Signal<String>,
    /// Bumped on every mount so an identical repeat still re-renders.
    pub serial: Signal<u64>,
}

/// The live toast signals for this request (call once per page).
pub fn live_toast(cx: &Cx) -> LiveToast {
    LiveToast {
        status: signal(cx, String::new),
        title: signal(cx, String::new),
        description: signal(cx, String::new),
        serial: signal(cx, || 0u64),
    }
}

/// Names the live-toaster endpoint so it stays stable across builds
/// (topcoat#441); staying under `/_topcoat/runtime` keeps the panel's auth gate
/// covering it.
#[cfg(test)]
pub(crate) const LIVE_TOASTER_PATH: &str = "/_topcoat/runtime/shards/tablo-live-toaster";

/// Reads the page's [`LiveToast`] signals and mounts the toast in place when one
/// is set.
#[shard("/_topcoat/runtime/shards/tablo-live-toaster")]
pub async fn live_toaster(
    cx: &Cx,
    status: Signal<String>,
    title: Signal<String>,
    description: Signal<String>,
    serial: Signal<u64>,
) -> topcoat::Result<impl View> {
    render_live_toaster(cx, &status, &title, &description, &serial).await
}

async fn render_live_toaster<'a>(
    cx: &'a Cx,
    status: &Signal<String>,
    title: &Signal<String>,
    description: &Signal<String>,
    serial: &Signal<u64>,
) -> topcoat::Result<BoxView<'a>> {
    // The shard restates the panel gate; an empty slot renders without auth,
    // but a direct POST must not mount toasts unauthenticated.
    crate::auth::guard(cx)?;
    let status = status.get();
    let mount = serial.get();
    if status.is_empty() {
        return Ok(().boxed());
    }
    let title = title.get();
    let description = description.get();
    let notification = match status.as_str() {
        "success" => Notification::success(title),
        "warning" => Notification::warning(title),
        "error" => Notification::error(title),
        _ => Notification::info(title),
    };
    let notification = if description.is_empty() {
        notification
    } else {
        notification.description(description)
    };
    let mount_id = format!("live-toast-{mount}");
    render_notification(cx, notification, attributes! { cx => id=(mount_id) }).await
}

#[cfg(test)]
mod tests;
