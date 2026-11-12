//! [`PanelId`]: which mounted panel a check is about.

use std::{fmt, sync::Arc};

/// A mounted panel's identity: its prefix, normalized once when the panel is declared.
///
/// [`PanelUser::can_access_panel`](crate::PanelUser::can_access_panel) receives one, and a
/// mounted panel's [`PanelHandle::id`](crate::PanelHandle::id) and [`Panel::id`](crate::Panel::id)
/// give the same one. It compares equal to a prefix in any spelling
/// [`Panel::new`](crate::Panel::new) accepts, so `"admin"`, `"/admin"` and `"/admin/"` all name
/// `Panel::new("admin")`, and to nothing else: it exposes no string to match a substring or a
/// leading segment against.
///
/// ```rust
/// # use tablo_core::PanelId;
/// let admin = PanelId::new("admin");
/// assert!(admin == "/admin");
/// assert!(admin == "admin/");
/// assert!(admin != "/admin-portal");
/// assert_eq!(admin, PanelId::new("/admin/"));
/// assert_eq!(admin.to_string(), "/admin");
/// ```
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct PanelId(Arc<str>);

impl PanelId {
    /// The panel `Panel::new(prefix)` declares, its prefix spelled as for
    /// [`Panel::new`](crate::Panel::new): an empty prefix names `/admin`.
    pub fn new(prefix: &str) -> Self {
        Self(crate::naming::normalize_prefix(prefix).into())
    }
}

/// Whether `raw`, spelled as for `Panel::new`, normalizes to `normalized`, without allocating.
fn names(normalized: &str, raw: &str) -> bool {
    let trimmed = raw.trim().trim_matches('/');
    let trimmed = if trimmed.is_empty() { "admin" } else { trimmed };
    normalized.strip_prefix('/') == Some(trimmed)
}

impl PartialEq<str> for PanelId {
    fn eq(&self, other: &str) -> bool {
        names(&self.0, other)
    }
}

impl PartialEq<&str> for PanelId {
    fn eq(&self, other: &&str) -> bool {
        names(&self.0, other)
    }
}

impl PartialEq<String> for PanelId {
    fn eq(&self, other: &String) -> bool {
        names(&self.0, other)
    }
}

impl PartialEq<PanelId> for str {
    fn eq(&self, other: &PanelId) -> bool {
        other == self
    }
}

impl PartialEq<PanelId> for &str {
    fn eq(&self, other: &PanelId) -> bool {
        other == *self
    }
}

impl PartialEq<PanelId> for String {
    fn eq(&self, other: &PanelId) -> bool {
        other == self
    }
}

impl fmt::Display for PanelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for PanelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PanelId").field(&&*self.0).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::PanelId;

    #[test]
    fn a_prefix_in_any_spelling_names_the_panel_and_no_other() {
        let admin = PanelId::new("admin");
        for spelling in ["admin", "/admin", "/admin/", " admin "] {
            assert!(admin == spelling, "{spelling:?} names /admin");
        }
        for other in ["/admin-portal", "/adm", "/admin/reports", "portal"] {
            assert!(admin != other, "{other:?} names another panel");
        }
        assert!(PanelId::new("") == "admin", "an empty prefix is /admin");
        assert!(PanelId::new("a/b") == "/a/b/");
    }
}
