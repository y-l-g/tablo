//! [`FieldStates`]: whether each field's control posts in a submission, and what the server reads
//! for one that does not.
//!
//! The browser posts nothing for a disabled or hidden control, so the server reads a submission
//! the same way whatever a crafted POST carries: it settles each field's state first, then takes
//! every value from that state alone.

use std::collections::HashMap;

use super::{FormMask, Schema};
use crate::values::FormValues;

/// What one field's control posts in a submission.
///
/// When two apply, the later variant wins: a hidden field takes the blank answer even when it is
/// also disabled or masked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldState {
    /// The control posts: its value stands.
    Posted,
    /// The control renders disabled, statically or through a condition, or the request's field
    /// access masks it: it posts nothing, so an edit keeps the stored value and a create takes
    /// the field's `default`.
    Withheld,
    /// A condition hides the control: it posts nothing, so an edit keeps the stored value and a
    /// create takes the blank answer.
    Hidden,
}

/// What a submission's keys stand for when it does not post them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unposted {
    /// What a browser's form leaves out: a key it renders a control for is posted, so one
    /// missing stays missing.
    Missing,
    /// An import row's: the header leaves the key out, so it takes the field's `default`, as the
    /// create page's untouched control posts it.
    Defaulted,
}

/// The form a submission posts from.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Posting<'a> {
    /// Whether the edit form posts, rather than the create form.
    pub(crate) editing: bool,
    /// The keys the request's field access masks.
    pub(crate) mask: &'a FormMask,
    pub(crate) unposted: Unposted,
    /// The record the edit form loads, which a condition reads for a key the submission leaves
    /// out; empty on a create.
    pub(crate) stored: &'a FormValues,
}

/// Each field's [`FieldState`] in one submission.
#[derive(Debug)]
pub(crate) struct FieldStates(HashMap<String, FieldState>);

impl FieldStates {
    /// The state of the field `key`, or `None` when the schema declares no such field.
    pub(crate) fn get(&self, key: &str) -> Option<FieldState> {
        self.0.get(key).copied()
    }

    /// Whether what the submission posts under `key`, a file part or a file field's flags,
    /// stands: the key's control posts, or no field declares it.
    pub(crate) fn posts(&self, key: &str) -> bool {
        self.get(key)
            .is_none_or(|state| state == FieldState::Posted)
    }

    /// The values `posted` leaves under these states: a posted value stands, a withheld field
    /// takes its `default` on a create, and a hidden one takes nothing.
    fn settle<F>(
        &self,
        schema: &Schema<F>,
        posted: &FormValues,
        posting: Posting<'_>,
    ) -> FormValues {
        let mut values = posted.clone();
        for field in schema.fields() {
            let name = field.name();
            let default = field.default_value();
            match self.get(name) {
                Some(FieldState::Posted) | None => {
                    if posting.unposted == Unposted::Defaulted
                        && values.get(name).is_none()
                        && let Some(default) = default
                    {
                        values.insert(name, default.clone());
                    }
                }
                Some(FieldState::Withheld) => {
                    values.remove(name);
                    if let Some(default) = default.filter(|_| !posting.editing) {
                        values.insert(name, default.clone());
                    }
                }
                Some(FieldState::Hidden) => {
                    values.remove(name);
                }
            }
        }
        values
    }
}

impl<F> Schema<F> {
    /// Each field's state in the submission `posted` from `posting`'s form, and the values it
    /// leaves.
    ///
    /// A disabled or masked control's state does not depend on the submission. A condition's
    /// does: it reads the submission as the browser did, with each withheld field's value
    /// settled and the stored value of each key the submission leaves out.
    pub(crate) fn resolve(
        &self,
        posted: &FormValues,
        posting: Posting<'_>,
    ) -> (FieldStates, FormValues) {
        let mask = posting.mask;
        let mut states = FieldStates(
            self.fields()
                .map(|field| {
                    let name = field.name();
                    let withheld =
                        field.is_disabled(posting.editing) || mask.hides(name) || mask.locks(name);
                    let state = if withheld {
                        FieldState::Withheld
                    } else {
                        FieldState::Posted
                    };
                    (name.to_string(), state)
                })
                .collect(),
        );
        let mut read = posting.stored.clone();
        read.extend(states.settle(self, posted, posting));
        // `condition_disabled` leaves out what a condition hides, so neither overwrites the other.
        for key in self.condition_hidden(&read) {
            states.0.insert(key, FieldState::Hidden);
        }
        for key in self.condition_disabled(&read) {
            states.0.insert(key, FieldState::Withheld);
        }
        let values = states.settle(self, posted, posting);
        (states, values)
    }
}

#[cfg(test)]
mod tests;
