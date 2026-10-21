//! [`FormMask`]: the form keys one request may not see, and the ones it may see but not change.

use std::collections::HashSet;

use crate::values::FormValues;

/// The form keys a request's [`field_access`](crate::ResourceDef::field_access) masks: a hidden
/// key's control renders nothing and its value never reaches the page; a locked key's control
/// renders disabled. A submission posting either is refused.
#[derive(Debug, Clone, Default)]
pub(crate) struct FormMask {
    hidden: HashSet<String>,
    locked: HashSet<String>,
}

impl FormMask {
    /// A mask hiding `hidden` and locking `locked`.
    pub(crate) fn new(
        hidden: impl IntoIterator<Item = String>,
        locked: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            hidden: hidden.into_iter().collect(),
            locked: locked.into_iter().collect(),
        }
    }

    /// Whether the control posting `key` renders nothing.
    pub(crate) fn hides(&self, key: &str) -> bool {
        self.hidden.contains(key)
    }

    /// Whether the control posting `key` renders disabled.
    pub(crate) fn locks(&self, key: &str) -> bool {
        self.locked.contains(key)
    }

    /// Every masked key, hidden or locked.
    pub(crate) fn keys(&self) -> impl Iterator<Item = &str> {
        self.hidden.iter().chain(&self.locked).map(String::as_str)
    }

    /// The masked key a posted `name` writes: the key itself, a repeater row's control under
    /// `{key}.`, or a file field's `clear_{key}` and `keep_{key}` flags.
    pub(crate) fn writes(&self, name: &str) -> Option<&str> {
        let flagged = name
            .strip_prefix("clear_")
            .or_else(|| name.strip_prefix("keep_"));
        self.keys().find(|key| {
            name == *key
                || name
                    .strip_prefix(key)
                    .is_some_and(|rest| rest.starts_with('.'))
                || flagged == Some(*key)
        })
    }

    /// `values` without the hidden keys, which the page must not carry.
    pub(crate) fn shown(&self, values: &FormValues) -> FormValues {
        let mut shown = values.clone();
        for key in &self.hidden {
            shown.remove(key);
        }
        shown
    }
}
