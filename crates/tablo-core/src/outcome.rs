//! What a write's response says and where it lands: the [`Notification`] it flashes, and the
//! [`Outcome`] an [`Action`](crate::Action) or [`HeaderAction`](crate::HeaderAction) returns.

use serde::{Deserialize, Serialize};

/// The kind of notification (status).
///
/// The serde tokens are lowercase so the JSON cookie reads naturally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
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

/// A message for a panel user: flashed as a toast after a write, or stored for the bell with
/// [`Notifications::send`](crate::Notifications::send).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    pub status: NotificationStatus,
    pub title: String,
    /// Supporting line under the title; `None` keeps the cookie wire format so
    /// older cookies still decode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The links under the text, in the order they were added.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<NotificationAction>,
}

/// A link a [`Notification`] offers under its text, such as "Download" or "View order".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationAction {
    pub label: String,
    /// A same-origin path such as `/admin/orders/42`, or an `http` or `https` URL. Anything else,
    /// such as `javascript:…`, renders no link.
    pub url: String,
}

impl NotificationAction {
    /// The URL when a browser may follow it: a same-origin path, or an `http` or `https` URL.
    pub(crate) fn safe_url(&self) -> Option<&str> {
        let url = self.url.trim();
        if same_origin_path(url).is_some() {
            return Some(url);
        }
        let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
        let web = lower.starts_with("https://") || lower.starts_with("http://");
        (web && !url.chars().any(|c| c.is_control() || c == '\\')).then_some(url)
    }
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
            actions: Vec::new(),
        }
    }

    /// Attach the supporting line under the title.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Adds a link under the text: `url` is a same-origin path such as `/admin/orders/42`, or an
    /// `http` or `https` URL. A URL of any other scheme renders no link.
    pub fn action(mut self, label: impl Into<String>, url: impl Into<String>) -> Self {
        self.actions.push(NotificationAction {
            label: label.into(),
            url: url.into(),
        });
        self
    }
}

/// What an [`Action`](crate::Action) or a [`HeaderAction`](crate::HeaderAction) asks of the
/// response once its write commits: which notification it flashes and where it lands.
///
/// [`Outcome::done`] keeps the defaults: the action's [`success`](crate::Action::success)
/// notification, landing back where the action ran from. A run builds on it:
///
/// ```rust
/// # use tablo_core::{Notification, Outcome};
/// # let (sent, bounced, id) = (3, 1, uuid::Uuid::nil());
/// // Report what happened, and open the copy the action made.
/// let outcome = Outcome::done()
///     .notify(
///         Notification::warning(format!("{sent} sent")).description(format!("{bounced} bounced")),
///     )
///     .redirect(format!("/admin/invoices/{id}"));
/// # let _ = outcome;
/// ```
///
/// A file to download is a redirect to an app route that serves it.
#[derive(Debug, Clone, Default)]
#[must_use]
pub struct Outcome {
    notification: Option<Notification>,
    redirect: Option<String>,
}

impl Outcome {
    /// The defaults: the action's success notification, landing where it ran from.
    pub fn done() -> Self {
        Self::default()
    }

    /// Flashes `notification` instead of the action's success notification. A bulk run that
    /// skipped records appends their count to its title.
    pub fn notify(mut self, notification: Notification) -> Self {
        self.notification = Some(notification);
        self
    }

    /// Lands on `path` instead of where the action ran from. `path` is a same-origin path such
    /// as `/admin/invoices/42`; anything else, such as `https://…` or `//host`, lands where the
    /// action ran from and logs an error, since the write has already committed.
    pub fn redirect(mut self, path: impl Into<String>) -> Self {
        self.redirect = Some(path.into());
        self
    }

    /// The outcome with `skipped`, a bulk run's count of skipped records, appended to its own
    /// notification's title.
    pub(crate) fn skipping(mut self, skipped: &str) -> Self {
        if let Some(notification) = &mut self.notification {
            notification.title.push_str(skipped);
        }
        self
    }

    /// The notification to flash, if not the default.
    pub(crate) fn notification(&self) -> Option<&Notification> {
        self.notification.as_ref()
    }

    /// The landing path asked for, if any, as given.
    pub(crate) fn redirect_path(&self) -> Option<&str> {
        self.redirect.as_deref()
    }
}

/// `path` when it is a same-origin relative path: it starts with one `/` and holds no
/// backslash or control character, so a browser cannot read it as another host.
pub(crate) fn same_origin_path(path: &str) -> Option<&str> {
    let path = path.trim();
    if !path.starts_with('/') || path.starts_with("//") {
        return None;
    }
    if path.contains('\\') || path.chars().any(|c| c.is_control()) {
        return None;
    }
    Some(path)
}
