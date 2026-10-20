//! Messages for a panel user: the toast a write flashes, kept in a JSON flash cookie with
//! hardened attributes that Topcoat flushes on error responses too (topcoat#408), and the
//! [`StoredNotification`]s [`Notifications`] sends to the bell of a panel with
//! [`Panel::notifications`](crate::Panel::notifications).

mod bell;
mod bus;
mod stored;

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

pub use self::{
    bell::{
        clear_notifications, mark_all_notifications_read, mark_notification, notification_bell,
        remove_notification,
    },
    stored::{Notifications, StoredNotification},
};
pub(crate) use self::{
    bell::{prefetch as prefetch_bell, topbar_bell},
    bus::NotificationBus,
    stored::bus,
};
pub use crate::outcome::{Notification, NotificationAction, NotificationStatus};

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

/// The icon a notification of `status` shows, and its classes.
pub(crate) fn status_icon(status: &NotificationStatus) -> (topcoat::icon::IconData, &'static str) {
    match status {
        NotificationStatus::Success => (icons::CIRCLE_CHECK, "size-4"),
        NotificationStatus::Error => (icons::OCTAGON_X, "size-4 text-destructive"),
        NotificationStatus::Info => (icons::INFO, "size-4"),
        NotificationStatus::Warning => (icons::TRIANGLE_ALERT, "size-4"),
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
        actions,
    } = notification;
    let (icon_data, icon_class) = status_icon(&status);
    let links: Vec<(String, String)> = actions
        .iter()
        .filter_map(|action| Some((action.label.clone(), action.safe_url()?.to_owned())))
        .collect();
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
                if !links.is_empty() {
                    <div class="flex flex-wrap gap-3 pt-1">
                        for (label, url) in links {
                            <a
                                href=(url)
                                class="font-medium text-primary underline-offset-4 hover:underline"
                            >
                                (label)
                            </a>
                        }
                    </div>
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
