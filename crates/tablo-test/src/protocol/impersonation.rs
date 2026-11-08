//! Reads the banner a panel page shows while its signed-in user is acted as another:
//! `[data-impersonating]`, naming the target's id, and the form its Stop button posts; and the
//! buttons `auth::impersonate_button` renders, `form[data-impersonate]`.

use super::dom::{attr_value, elements, strip_text};

/// The banner over a page a user reads while acting as another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpersonationBanner {
    /// The `user_id` of the user being acted as.
    pub user_id: String,
    /// What the banner says, the Stop button's label included.
    pub text: String,
    /// Where its Stop button posts.
    pub stop: String,
}

/// The banner `html` shows, or `None` while the signed-in user acts as themselves.
pub fn impersonation_banner(html: &str) -> Option<ImpersonationBanner> {
    let (_, attrs, inner) = elements(html, "div")
        .into_iter()
        .find(|(_, attrs, _)| attr_value(attrs, "data-impersonating").is_some())?;
    let stop = elements(&inner, "form")
        .into_iter()
        .find_map(|(_, attrs, _)| attr_value(&attrs, "action"))?;
    Some(ImpersonationBanner {
        user_id: attr_value(&attrs, "data-impersonating")?,
        text: strip_text(&inner),
        stop,
    })
}

/// The `user_id`s the `auth::impersonate_button` forms of `html` act as, in document order.
pub fn impersonate_buttons(html: &str) -> Vec<String> {
    elements(html, "form")
        .into_iter()
        .filter_map(|(_, attrs, _)| attr_value(&attrs, "data-impersonate"))
        .collect()
}
