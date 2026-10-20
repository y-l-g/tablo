//! Reads the notification bell out of a rendered panel page: `[data-unread]` on the trigger's
//! count, and `li[data-notification]` for each listed notification, with its `data-read` and
//! `data-type`.

use super::dom::{attr_value, strip_text, tags_where};

/// The bell a panel page renders in its topbar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bell {
    /// The count over the trigger; `0` when it shows none.
    pub unread: u64,
    /// The notifications the sheet lists, newest first.
    pub notifications: Vec<BellNotification>,
}

/// One notification the bell's sheet lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BellNotification {
    /// The stored notification's id, which the bell's procedures take.
    pub id: String,
    pub title: String,
    pub read: bool,
    /// The status token: `success`, `error`, `info` or `warning`.
    pub status: String,
    /// The `(label, href)` of each link it offers.
    pub actions: Vec<(String, String)>,
}

/// The bell `html` renders, or `None` when the page shows none.
pub fn bell(html: &str) -> Option<Bell> {
    let trigger = tags_where(html, |tag, attrs| {
        tag == "button"
            && attr_value(attrs, "aria-controls").as_deref() == Some("tablo-notifications")
    });
    if trigger.is_empty() {
        return None;
    }
    let unread = tags_where(html, |tag, attrs| {
        tag == "span" && attr_value(attrs, "data-unread").is_some()
    })
    .first()
    .and_then(|(_, attrs, _)| attr_value(attrs, "data-unread")?.parse().ok())
    .unwrap_or(0);
    let notifications = tags_where(html, |tag, attrs| {
        tag == "li" && attr_value(attrs, "data-notification").is_some()
    })
    .into_iter()
    .map(|(_, attrs, inner)| BellNotification {
        id: attr_value(&attrs, "data-notification").unwrap_or_default(),
        title: tags_where(&inner, |tag, _| tag == "p")
            .first()
            .map(|(_, _, text)| strip_text(text))
            .unwrap_or_default(),
        read: attr_value(&attrs, "data-read").as_deref() == Some("true"),
        status: attr_value(&attrs, "data-type").unwrap_or_default(),
        actions: tags_where(&inner, |tag, _| tag == "a")
            .into_iter()
            .map(|(_, attrs, text)| {
                (
                    strip_text(&text),
                    attr_value(&attrs, "href").unwrap_or_default(),
                )
            })
            .collect(),
    })
    .collect();
    Some(Bell {
        unread,
        notifications,
    })
}
