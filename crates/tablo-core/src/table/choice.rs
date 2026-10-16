//! [`ColumnChoice`]: which toggleable columns one reader departs from the defaults of.

use std::collections::BTreeSet;

use super::{BoxColumn, ColumnToggle};
use crate::topcoat_compat::href;

/// The parameter naming a column the reader hides though it shows by default.
const HIDE: &str = "hide";

/// The parameter naming a column the reader shows though it hides by default.
const SHOW: &str = "show";

/// Caps the names one stored choice keeps, so a row stays small whatever it was written with.
const MAX_NAMES: usize = 64;

/// Caps one stored column name, in bytes.
const MAX_NAME_LEN: usize = 256;

/// The toggleable columns one reader shows or hides against their defaults.
///
/// Only departures are kept: a column added later, or one whose default changes, takes its
/// default until the reader picks otherwise, and a name no longer declared is ignored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ColumnChoice {
    /// Shown by default, hidden by the reader.
    hidden: BTreeSet<String>,
    /// Hidden by default, shown by the reader.
    shown: BTreeSet<String>,
}

impl ColumnChoice {
    /// Whether a column named `name`, declared with `toggle`, shows to this reader.
    pub(crate) fn shows(&self, name: &str, toggle: ColumnToggle) -> bool {
        match toggle {
            ColumnToggle::Fixed => true,
            ColumnToggle::ShownByDefault => !self.hidden.contains(name),
            ColumnToggle::HiddenByDefault => self.shown.contains(name),
        }
    }

    /// The choice that shows, among the toggleable `columns`, exactly those named in `checked`.
    pub(crate) fn checked<'a, M: 'a>(
        columns: impl IntoIterator<Item = &'a BoxColumn<M>>,
        checked: &[&str],
    ) -> Self {
        let mut choice = Self::default();
        for column in columns {
            let name = column.name();
            let wanted = checked.contains(&name);
            match column.toggle() {
                ColumnToggle::ShownByDefault if !wanted => {
                    choice.hidden.insert(name.to_string());
                }
                ColumnToggle::HiddenByDefault if wanted => {
                    choice.shown.insert(name.to_string());
                }
                _ => {}
            }
        }
        choice
    }

    /// Whether the choice departs from no default.
    pub(crate) fn is_default(&self) -> bool {
        self.hidden.is_empty() && self.shown.is_empty()
    }

    /// The stored spelling: `hide=<name>` and `show=<name>` pairs, query-encoded.
    pub(crate) fn encode(&self) -> String {
        let pairs: Vec<(String, &str)> = self
            .hidden
            .iter()
            .map(|name| (HIDE.to_string(), name.as_str()))
            .chain(
                self.shown
                    .iter()
                    .map(|name| (SHOW.to_string(), name.as_str())),
            )
            .collect();
        href::encode_query(&pairs)
    }

    /// The choice [`encode`](Self::encode) spelled; an unknown parameter, an over-long name and
    /// any name past the cap are dropped.
    pub(crate) fn decode(stored: &str) -> Self {
        let mut choice = Self::default();
        let pairs = form_urlencoded::parse(stored.as_bytes())
            .filter(|(_, name)| !name.is_empty() && name.len() <= MAX_NAME_LEN)
            .take(MAX_NAMES);
        for (param, name) in pairs {
            let set = match param.as_ref() {
                HIDE => &mut choice.hidden,
                SHOW => &mut choice.shown,
                _ => continue,
            };
            set.insert(name.into_owned());
        }
        choice
    }
}

#[cfg(test)]
mod tests;
