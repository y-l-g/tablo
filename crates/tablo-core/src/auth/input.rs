//! What the auth pages share in reading a submission: the posted body, an app's [`FormInput`]
//! read from it, and the refusal of a form field those pages cannot serve.

use topcoat::{context::Cx, router::Body};

use crate::{
    form::{FieldErrors, FormInput, FormValues},
    schema::Schema,
    upload::{Capture, FormParts, parse_body},
};

/// Parses an auth form's body. Auth stores no file, so a chosen file's bytes are drained, never
/// staged.
pub(crate) async fn read_form(cx: &Cx, body: Body) -> topcoat::Result<FormParts> {
    parse_body(cx, body, Capture::None).await
}

/// Parses `values` into `I` and checks its controls' own rules against `stored`, the values the
/// form started from: the input, or every error to render. `form` names the page in the
/// declaration error an error no control renders raises.
pub(super) async fn parse<I: FormInput>(
    cx: &Cx,
    values: &FormValues,
    stored: &FormValues,
    form: &str,
) -> topcoat::Result<Result<I, FieldErrors>> {
    let schema = I::schema();
    let mut errors = FieldErrors::new();
    let parsed = match I::parse(cx, values) {
        Ok(parsed) => Some(parsed),
        Err(refused) => {
            for error in refused {
                errors.push(error);
            }
            None
        }
    };
    schema.check_controls(cx, values, stored, &mut errors).await;
    placed(&schema, &errors, &format!("the {form} input"), form)?;
    Ok(parsed.filter(|_| errors.is_empty()).ok_or(errors))
}

/// Refuses `errors` when one names a key no control of `schema` renders, which the page would show
/// no message for: a declaration error saying `who` refused it on the `form` page.
pub(super) fn placed(
    schema: &Schema,
    errors: &FieldErrors,
    who: &str,
    form: &str,
) -> topcoat::Result<()> {
    match schema.unplaced_error(errors) {
        Some(key) => Err(crate::error::declaration(format!(
            "{who} refused key `{key}`, which no {form} control renders"
        ))),
        None => Ok(()),
    }
}

/// Why an auth page cannot serve one of its form's fields.
pub(super) enum FieldFault {
    /// The field is named like a key the page's POST carries itself.
    Reserved,
    /// A file field, whose upload the page's POST does not read.
    File,
    /// A relationship choice, whose options no route serves the page.
    Relationship,
}

/// Each field of `schema` the page cannot serve, by name, given the `reserved` keys its POST
/// carries itself.
pub(super) fn field_faults<'a>(
    schema: &'a Schema,
    reserved: &'a [&str],
) -> impl Iterator<Item = (FieldFault, String)> + 'a {
    schema.fields().filter_map(|field| {
        let name = field.name().to_string();
        let fault = if reserved.contains(&name.as_str()) {
            FieldFault::Reserved
        } else if field.is_file() {
            FieldFault::File
        } else if field
            .as_choice()
            .is_some_and(|choice| choice.is_relationship())
        {
            FieldFault::Relationship
        } else {
            return None;
        };
        Some((fault, name))
    })
}
