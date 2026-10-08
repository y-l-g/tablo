//! What an [`Action`](super::Action) asks for before it runs: the [`ActionInput`] trait.

use std::collections::HashMap;

use topcoat::context::Cx;

pub(crate) use crate::table::SUBMITTED_KEY;
use crate::{
    form::FieldError,
    schema::{Field, Schema},
};

/// The typed value an [`Action`](super::Action) asks for before it runs: a rejection's reason, a
/// publication date, the author to reassign to.
///
/// Derive it with [`ActionInput`](derive@crate::ActionInput) on a struct of form scalars, or
/// name `()` for an action that asks for nothing. Its fields post keys no column binds, so an
/// input describes values the action reads rather than columns it writes.
///
/// An action with input asks for it before it runs: its button opens the input form in a dialog
/// over the page, and its submit runs the action with the parsed value. A refused value renders
/// the form as a page with the field's error, and writes nothing; a browser without scripts
/// reaches that page from the button too.
pub trait ActionInput: Sized + Send + 'static {
    /// The input form's controls. An empty schema asks for nothing: the action runs on its
    /// button.
    fn schema() -> Schema;

    /// Parse a submission of [`schema`](Self::schema)'s keys.
    ///
    /// # Errors
    ///
    /// Every key that failed, each once.
    fn parse(cx: &Cx, values: &HashMap<String, String>) -> Result<Self, Vec<FieldError>>;
}

/// No input: the action runs on its button.
impl ActionInput for () {
    fn schema() -> Schema {
        Schema::empty()
    }

    fn parse(_cx: &Cx, _values: &HashMap<String, String>) -> Result<Self, Vec<FieldError>> {
        Ok(())
    }
}

/// Renders `field`'s control required: the derive marks each field with no blank answer.
#[doc(hidden)]
pub fn required_input(field: impl Into<Field>) -> Field {
    let mut field = field.into();
    field.set_required(true);
    field
}

/// The keys an action's POST carries besides its input, which no input field may post.
pub(crate) const RESERVED_KEYS: [&str; 4] =
    [crate::csrf::FIELD_NAME, "confirm", "ids", SUBMITTED_KEY];
