//! Reading a completed submission into the record form, and keying what refuses it by the form
//! keys its errors render under.

use std::collections::HashSet;

use topcoat::{Result, context::Cx};

use crate::{
    form::{FieldError, FieldErrors, FormValues, RecordForm},
    resource::{Mounted, Resource},
    schema::Schema,
};

/// Fills unnamed keys from the stored projection, dropping keys it does not hold.
pub(in crate::panel::forms) fn complete(
    schema: &Schema,
    values: &mut FormValues,
    named: &HashSet<String>,
    stored: &FormValues,
) {
    for field in schema.fields() {
        let name = field.name();
        if named.contains(name) {
            continue;
        }
        match stored.get(name) {
            Some(value) => values.insert(name, value.clone()),
            None => values.remove(name),
        };
    }
}

/// Parses completed values and runs `validate_record` on the parsed form, adding every refusal
/// to `errors` under the form key it renders under.
///
/// # Errors
///
/// A declaration error when a hand-written record form refuses a key its `fields` do not bind:
/// no control would carry the message, and writing anyway would drop it.
pub(in crate::panel::forms) fn parse_form<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    values: &FormValues,
    errors: &mut FieldErrors,
) -> Result<Option<R::Form>, topcoat::Error> {
    match <R::Form as RecordForm>::parse(cx, values) {
        Ok(form) => {
            errors.extend(form_keyed(
                resource,
                "validate_record",
                R::validate_record(cx, &form),
            )?);
            Ok(Some(form))
        }
        Err(failures) => {
            for failure in failures {
                if !resource.fields.iter().any(|field| {
                    field
                        .keys
                        .iter()
                        .any(|key| renders_under(&resource.form, key, &failure.key))
                }) {
                    return Err(unbound::<R>("the parse", &failure.key));
                }
                // The controls' own rules answered first for the key.
                if !errors.contains_key(&failure.key) {
                    errors.push(failure);
                }
            }
            Ok(None)
        }
    }
}

/// `errors`, which `source` keyed by the record form's field enum, keyed by the form key each
/// renders under: a field's first.
///
/// # Errors
///
/// A declaration error when a field binds no form key: no control would carry the message.
fn form_keyed<R: Resource>(
    resource: &Mounted<R>,
    source: &str,
    errors: FieldErrors<<R::Form as RecordForm>::Field>,
) -> Result<FieldErrors, topcoat::Error> {
    let mut keyed = FieldErrors::new();
    for FieldError { key, kind } in errors {
        let Some(first) = resource
            .fields
            .iter()
            .find(|field| field.field == key)
            .and_then(|field| field.keys.first())
        else {
            return Err(unbound::<R>(source, &key));
        };
        keyed.push(FieldError {
            key: first.clone(),
            kind,
        });
    }
    Ok(keyed)
}

/// The refusal a record fn answered with, keyed as [`form_keyed`] keys it.
///
/// # Errors
///
/// A declaration error for a refusal that names no field, which would render the form unchanged,
/// or one [`form_keyed`] refuses.
pub(in crate::panel::forms) fn refusal<R: Resource>(
    resource: &Mounted<R>,
    source: &str,
    errors: FieldErrors<<R::Form as RecordForm>::Field>,
) -> Result<FieldErrors, topcoat::Error> {
    if errors.is_empty() {
        return Err(crate::error::declaration(format!(
            "{source} of `{}` refused the submission with no field error",
            std::any::type_name::<R>()
        )));
    }
    form_keyed(resource, source, errors)
}

/// The declaration error of `source` refusing `key`, which `R`'s record form binds to no form key.
fn unbound<R: Resource>(source: &str, key: &dyn std::fmt::Debug) -> topcoat::Error {
    crate::error::declaration(format!(
        "{source} refused {key:?}, which `{}` binds to no form key",
        std::any::type_name::<R::Form>()
    ))
}

/// Whether an error on `failed` renders under the field posting `key`: its own, or, for a
/// repeater, a row's control, which posts under `{key}.`.
fn renders_under(form: &Schema, key: &str, failed: &str) -> bool {
    match failed.strip_prefix(key) {
        Some("") => true,
        Some(rest) => {
            rest.starts_with('.')
                && form
                    .fields()
                    .any(|field| field.name() == key && field.as_repeater().is_some())
        }
        None => false,
    }
}

/// Re-checks relationship keys inside the write transaction before writing, but the links the
/// record in `stored` holds already.
pub(in crate::panel::forms) async fn recheck_relationships(
    cx: &Cx,
    schema: &Schema,
    values: &FormValues,
    stored: &FormValues,
    errors: &mut FieldErrors,
    ex: &mut dyn toasty::Executor,
) {
    if errors.is_empty() {
        errors.extend(schema.recheck_relationships(cx, values, stored, ex).await);
    }
}

pub(in crate::panel::forms) fn named_fields<R: Resource>(
    resource: &Mounted<R>,
    named: &HashSet<String>,
) -> Vec<<R::Form as RecordForm>::Field> {
    resource
        .fields
        .iter()
        .filter(|field| field.keys.iter().any(|key| named.contains(key)))
        .map(|field| field.field)
        .collect()
}

#[cfg(test)]
mod tests;
