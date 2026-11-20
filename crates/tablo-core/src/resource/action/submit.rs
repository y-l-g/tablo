//! [`Submits`]: the buttons of an action's input dialog beside the action's own.

use std::marker::PhantomData;

use crate::{Text, schema::input::FormInput};

/// The buttons of an action's input dialog and input page beside the action's own, each running
/// the same action with one checkbox of the input checked.
///
/// [`Action::submits`](crate::Action::submits) and
/// [`HeaderAction::submits`](crate::HeaderAction::submits) add them, each over a `bool` field of
/// the input picked off its [`Keys`](crate::FormFields::Keys):
///
/// ```rust
/// # use tablo_core::{FormInput, Submits};
/// #[derive(FormInput)]
/// struct Shipping {
///     carrier: String,
///     notify: bool,
/// }
///
/// fn submits(submits: Submits<Shipping>) -> Submits<Shipping> {
///     submits.add("Ship and notify", |f| f.notify)
/// }
/// ```
///
/// The dialog renders no control for a checkbox a submit picks: the action's own button posts it
/// unchecked, `false`, and the submit posts it checked, `true`, so [`run`](crate::Action::run)
/// reads which button ran it off the input. The buttons follow the action's own in the order they
/// are added, so Enter in a field still runs the action's own. The POST goes through the action's
/// route, its policy, `can_run` and input checks, whichever button sent it.
///
/// Mounting refuses a submit picking a field that does not render a checkbox.
pub struct Submits<I> {
    specs: Vec<SubmitSpec>,
    input: PhantomData<fn() -> I>,
}

impl<I: FormInput> Submits<I> {
    /// No button beside the action's own.
    pub fn new() -> Self {
        Self {
            specs: Vec::new(),
            input: PhantomData,
        }
    }

    /// Adds a button labelled `label` that checks the field `pick` picks off the input's keys.
    #[must_use]
    pub fn add(mut self, label: impl Into<Text>, pick: impl FnOnce(I::Keys) -> I::Field) -> Self {
        self.specs.push(SubmitSpec {
            label: label.into(),
            key: I::key(pick(I::keys())),
        });
        self
    }

    /// The buttons past the input's type.
    pub(crate) fn into_specs(self) -> Vec<SubmitSpec> {
        self.specs
    }
}

impl<I: FormInput> Default for Submits<I> {
    fn default() -> Self {
        Self::new()
    }
}

/// One of [`Submits`]' buttons past its input's type: its label and the key it posts `true`
/// under.
#[derive(Clone)]
pub(crate) struct SubmitSpec {
    pub(crate) label: Text,
    pub(crate) key: &'static str,
}
