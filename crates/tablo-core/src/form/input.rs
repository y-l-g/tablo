//! A typed value a form posts under keys no column binds: the [`FormInput`] trait.

use topcoat::context::Cx;

use super::{FieldError, FormValues};
use crate::schema::{Field, Schema};

/// A typed value a form posts under its own keys, which no column binds: what an
/// [`Action`](crate::Action) asks for before it runs, what a sign-up reads, one item of a
/// repeater.
///
/// Derive it with [`FormInput`](derive@crate::FormInput) on a struct of form scalars, or name
/// `()` for a value that asks for nothing.
///
/// An action with input asks for it before it runs: its button opens the input form in a dialog
/// over the page, and its submit runs the action with the parsed value. A refused value renders
/// the form as a page with the field's error, and writes nothing.
///
/// A repeater stores its items in a `#[document]` list, so an item is also a `toasty::Embed`
/// struct. Each row posts the item's keys under its own prefix, and [`write`](Self::write) fills
/// a stored item's row on the edit page.
pub trait FormInput: Sized + Send + Sync + 'static {
    /// The value's controls, each posting its own key. An empty schema asks for nothing: an
    /// action runs on its button.
    fn schema() -> Schema;

    /// Parse a submission of [`schema`](Self::schema)'s keys.
    ///
    /// # Errors
    ///
    /// Every key that failed, each once.
    fn parse(cx: &Cx, values: &FormValues) -> Result<Self, Vec<FieldError>>;

    /// Write the value's keys as its form posts them: a repeater fills a stored item's row from
    /// them, and its detail-page column reads them. A password writes nothing, as it never renders
    /// back into a form. Nothing writes an action's input or a sign-up, so a type used only there
    /// writes nothing too.
    fn write(&self, out: &mut FormValues);
}

/// No input: an action runs on its button.
impl FormInput for () {
    fn schema() -> Schema {
        Schema::empty()
    }

    fn parse(_cx: &Cx, _values: &FormValues) -> Result<Self, Vec<FieldError>> {
        Ok(())
    }

    fn write(&self, _out: &mut FormValues) {}
}

/// Renders `field`'s control required: the derive marks each field with no blank answer.
#[doc(hidden)]
pub fn required_input(field: impl Into<Field>) -> Field {
    let mut field = field.into();
    field.set_required(true);
    field
}
