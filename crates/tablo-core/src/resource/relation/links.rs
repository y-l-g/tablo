//! A many-to-many relation's writes: the attach dialog's input, and linking and unlinking an
//! owner's records through its join model.

use toasty::stmt::{IntoExpr, List, Path};
use topcoat::{Result, context::Cx};

use super::{
    ATTACH_KEY, ErasedInput, InputResult, Resource, Trashed, can, scoped_query, scoped_query_in,
};
use crate::{
    form::{FieldError, FormValues},
    policy::Ability,
    schema::{Field, Schema},
    toasty_compat::{
        join::{JoinTable, linked_to},
        model::AppSchema,
        pk,
    },
};

/// The attach dialog's input: one searchable choice of `C`'s records.
pub(super) fn attach_schema<C: Resource>() -> Schema {
    Schema::new(
        Field::choice_input(ATTACH_KEY)
            .label(crate::Text::model::<C::Model>())
            .relationship::<C>()
            .searchable(),
    )
}

/// The key the attach dialog's choice posts: the schema's check proved it one of the choice's
/// records.
pub(super) fn parse_attach(_cx: &Cx, values: &FormValues) -> InputResult {
    match values.text(ATTACH_KEY).map(str::trim) {
        Some(key) if !key.is_empty() => Ok(Box::new(key.to_string()) as ErasedInput),
        _ => Err(vec![FieldError::required(ATTACH_KEY)]),
    }
}

/// The join model behind `owner`'s `field`.
fn join_table<P: toasty::schema::Model>(cx: &Cx, field: &str) -> Result<JoinTable> {
    let schema = AppSchema::of(cx).ok_or_else(|| {
        crate::error::declaration(crate::DeclarationErrorKind::MissingDb.to_string())
    })?;
    JoinTable::of::<P>(&schema, field).map_err(|fault| {
        crate::error::declaration(
            crate::DeclarationErrorKind::ManyToMany {
                field: field.to_string(),
                fault,
            }
            .to_string(),
        )
    })
}

/// Links `owner` to the record `key` names, unless they are linked already.
pub(super) async fn attach<P, C>(
    cx: &Cx,
    owner: &P,
    via: Path<P, List<C::Model>>,
    field: &str,
    key: String,
    ex: &mut dyn toasty::Executor,
) -> Result<String>
where
    P: toasty::schema::Model + IntoExpr<P>,
    C: Resource,
{
    let join = join_table::<P>(cx, field)?;
    let (Some(key), Some(record)) = (
        pk::canonical_key::<C::Model>(&key),
        pk::pk_eq_expr::<C::Model>(&key),
    ) else {
        return Err(topcoat::router::error::not_found().into());
    };
    let linked = scoped_query::<C>(cx)?
        .filter(record)
        .filter(linked_to(owner, via))
        .first()
        .exec(&mut *ex)
        .await
        .map_err(crate::error::unavailable)?;
    if linked.is_none() {
        join.link(owner, std::slice::from_ref(&key), ex)
            .await
            .map_err(|error| -> topcoat::Error { error.into() })?;
    }
    Ok(key)
}

/// Unlinks `owner` from the records `keys` names.
pub(super) async fn detach<P, C>(
    cx: &Cx,
    owner: &P,
    via: Path<P, List<C::Model>>,
    field: &str,
    keys: Vec<String>,
    ex: &mut dyn toasty::Executor,
) -> Result<Vec<String>>
where
    P: toasty::schema::Model + IntoExpr<P>,
    C: Resource,
{
    let join = join_table::<P>(cx, field)?;
    let ids: Vec<&str> = keys.iter().map(String::as_str).collect();
    let Some(records) = pk::pk_in_expr::<C::Model>(&ids) else {
        return Err(topcoat::router::error::not_found().into());
    };
    // A trashed record stays linked, and the relation table offers to unlink it.
    let linked = scoped_query_in::<C>(cx, Trashed::With)?
        .filter(records)
        .filter(linked_to(owner, via))
        .exec(&mut *ex)
        .await
        .map_err(crate::error::unavailable)?;
    if linked.len() != keys.len() {
        return Err(topcoat::router::error::not_found().into());
    }
    // A record the user cannot view cannot be unlinked by guessing its key.
    if linked
        .iter()
        .any(|record| !can::<C>(cx, Ability::View(record)))
    {
        return Err(topcoat::router::error::forbidden().into());
    }
    join.unlink(owner, &keys, ex)
        .await
        .map_err(|error| -> topcoat::Error { error.into() })?;
    Ok(linked.iter().map(pk::pk_text).collect())
}
