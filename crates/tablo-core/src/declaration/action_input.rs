//! The declaration mistakes of an action's input, and their wording.

use std::fmt;

/// Why an action's input cannot be served:
/// [`DeclarationErrorKind::ActionInput`](super::DeclarationErrorKind::ActionInput).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ActionInputFault {
    /// A field named like a key the action's POST carries itself: `csrf_token`, `confirm` or
    /// `ids`.
    ReservedField(String),
    /// A file field, whose upload an action's POST does not read.
    FileField(String),
    /// No field, but a parse that refuses an empty submission, so the action could never run.
    RefusesEmpty,
    /// A dependent choice, whose options no route serves an action's input.
    DependentChoice(String),
}

/// [`DeclarationErrorKind::ActionInput`](super::DeclarationErrorKind::ActionInput)'s sentence.
pub(super) fn fmt_action_input(
    f: &mut fmt::Formatter<'_>,
    action: &str,
    fault: &ActionInputFault,
) -> fmt::Result {
    match fault {
        ActionInputFault::ReservedField(field) => write!(
            f,
            "action '{action}' declares an input field named '{field}', which its POST \
             carries itself: rename the field"
        ),
        ActionInputFault::FileField(field) => write!(
            f,
            "action '{action}' declares a file field '{field}' in its input, which an \
             action's POST does not upload: take the file in a record form"
        ),
        ActionInputFault::RefusesEmpty => write!(
            f,
            "action '{action}' declares no input field, but its input refuses an empty \
             submission: declare the fields it parses, or name `()`"
        ),
        ActionInputFault::DependentChoice(field) => write!(
            f,
            "action '{action}' declares a dependent choice '{field}' in its input, whose \
             options only a resource's form refreshes: drop `depends_on`"
        ),
    }
}
