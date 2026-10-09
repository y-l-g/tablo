//! A many-to-many relation's join model, written one row per link.
//!
//! Toasty reads a `#[has_many(via = taggings.tag)]` field but refuses to insert or remove through
//! it ("insert is not supported on multi-step relation traversals"), and a generic write cannot
//! name the join model's type, which only the app schema knows. So a link is one join row built
//! from that schema: the two foreign keys set, `#[auto]` fields left to the database, and every
//! other field `NULL`. [`JoinTable::of`] refuses a join model with a column that cannot hold one.

use toasty::{
    schema::app::{self, FieldId, FieldTy, ModelId},
    stmt::{IntoExpr, Path, Value},
};
use toasty_core::stmt as core;

use super::{
    model::AppSchema,
    pk::{self, parse_key},
};
use crate::declaration::ManyToManyFault;

/// The join model behind one `#[has_many(via = ..)]` field: its rows link the owner to the
/// target's records.
#[derive(Debug, Clone)]
pub(crate) struct JoinTable {
    model: ModelId,
    /// What each field of a new row holds before its two keys are set.
    blank: Vec<core::Expr>,
    /// The join model's field holding the owner's key.
    owner: FieldId,
    /// The join model's field holding the target's key.
    target: FieldId,
    /// The target's primary-key type, which a submitted key parses as.
    key: core::Type,
    /// The target model.
    target_model: ModelId,
}

impl JoinTable {
    /// The join model of `M`'s field `name`, a `#[has_many(via = joins.target)]` whose first step
    /// is a `has_many` to the join model and whose second is the join model's `belongs_to` to the
    /// target.
    ///
    /// # Errors
    ///
    /// `name` is no such field, a key spans more than one column or references no primary key, or
    /// the join model has a column a new row cannot fill.
    pub(crate) fn of<M: toasty::schema::Model>(
        schema: &AppSchema,
        name: &str,
    ) -> Result<Self, ManyToManyFault> {
        let schema = schema.inner();
        let owner = schema
            .app
            .get_model(M::id())
            .and_then(app::Model::as_root)
            .ok_or(ManyToManyFault::NotJoinModel)?;
        let field = owner
            .fields
            .iter()
            .find(|field| field.name.app.as_deref() == Some(name))
            .ok_or(ManyToManyFault::NotJoinModel)?;
        let FieldTy::Via(via) = &field.ty else {
            return Err(ManyToManyFault::NotJoinModel);
        };
        let [has, belongs] = via.path.projection.as_slice() else {
            return Err(ManyToManyFault::NotJoinModel);
        };
        if !via.is_many() || via.is_scalar() {
            return Err(ManyToManyFault::NotJoinModel);
        }
        let has = owner
            .fields
            .get(*has)
            .and_then(|field| field.ty.as_has_many())
            .ok_or(ManyToManyFault::NotJoinModel)?;
        let join = schema
            .app
            .get_model(has.target)
            .and_then(app::Model::as_root)
            .ok_or(ManyToManyFault::NotJoinModel)?;
        let to_owner = join
            .fields
            .get(has.pair_id.index)
            .and_then(|field| field.ty.as_belongs_to())
            .ok_or(ManyToManyFault::NotJoinModel)?;
        let to_target = join
            .fields
            .get(*belongs)
            .and_then(|field| field.ty.as_belongs_to())
            .ok_or(ManyToManyFault::NotJoinModel)?;
        let target = schema
            .app
            .get_model(to_target.target)
            .and_then(app::Model::as_root)
            .ok_or(ManyToManyFault::NotJoinModel)?;
        let owner_key = key_column(to_owner, owner)?;
        let target_key = key_column(to_target, target)?;
        let key = match target
            .fields
            .get(target_key_of(to_target)?.index)
            .map(|f| &f.ty)
        {
            Some(FieldTy::Primitive(primitive)) => primitive.ty.clone(),
            _ => return Err(ManyToManyFault::CompositeKey),
        };
        let mut blank = Vec::with_capacity(join.fields.len());
        for field in &join.fields {
            let id = field.id;
            blank.push(if field.auto().is_some() {
                core::Expr::Default
            } else {
                if !(field.is_relation() || field.nullable() || id == owner_key || id == target_key)
                {
                    return Err(ManyToManyFault::UnwritableColumn {
                        model: join.name.upper_camel_case(),
                        column: field.name.app_unwrap().to_string(),
                    });
                }
                core::Expr::Value(core::Value::Null)
            });
        }
        Ok(Self {
            model: join.id,
            blank,
            owner: owner_key,
            target: target_key,
            key,
            target_model: target.id,
        })
    }

    /// The model whose records the owner links to.
    pub(crate) fn target_model(&self) -> ModelId {
        self.target_model
    }

    /// Links `owner` to the target records `keys` names: one join row each.
    ///
    /// # Errors
    ///
    /// A key the target's primary key does not parse, or the driver's error.
    pub(crate) async fn link<P>(
        &self,
        owner: &P,
        keys: &[String],
        ex: &mut dyn toasty::Executor,
    ) -> toasty::Result<()>
    where
        P: toasty::schema::Model + IntoExpr<P>,
    {
        let owner = owner_key(owner)?;
        for key in keys {
            let mut row = self.blank.clone();
            row[self.owner.index] = core::Expr::Value(owner.clone());
            row[self.target.index] = core::Expr::Value(self.parse(key)?);
            let insert = core::Insert {
                target: core::InsertTarget::Model(self.model),
                source: core::Query::new_single(vec![core::ExprRecord::from_vec(row).into()]),
                upsert: None,
                returning: None,
            };
            toasty::stmt::Statement::<()>::from_untyped_stmt(insert.into())
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }

    /// Removes the join rows linking `owner` to the target records `keys` names.
    ///
    /// # Errors
    ///
    /// A key the target's primary key does not parse, or the driver's error.
    pub(crate) async fn unlink<P>(
        &self,
        owner: &P,
        keys: &[String],
        ex: &mut dyn toasty::Executor,
    ) -> toasty::Result<()>
    where
        P: toasty::schema::Model + IntoExpr<P>,
    {
        if keys.is_empty() {
            return Ok(());
        }
        let owner = owner_key(owner)?;
        let targets = keys
            .iter()
            .map(|key| self.parse(key).map(core::Expr::Value))
            .collect::<toasty::Result<Vec<_>>>()?;
        let filter = core::Expr::and_from_vec(vec![
            core::Expr::eq(core::Expr::ref_self_field(self.owner), owner),
            core::Expr::in_list(
                core::Expr::ref_self_field(self.target),
                core::Expr::list(targets),
            ),
        ]);
        let delete = core::Delete {
            from: core::Source::from(self.model),
            filter: core::Filter::new(filter),
            returning: None,
            condition: core::Condition::default(),
        };
        toasty::stmt::Statement::<()>::from_untyped_stmt(delete.into())
            .exec(ex)
            .await
    }

    /// The target's key `text` spells.
    fn parse(&self, text: &str) -> toasty::Result<Value> {
        parse_key(&self.key, text.trim()).ok_or_else(|| {
            toasty::Error::from_args(format_args!("`{text}` is not a key of the linked model"))
        })
    }
}

/// The field of `from` holding the key of `to`, a single column referencing `to`'s single-column
/// primary key.
fn key_column(from: &app::BelongsTo, to: &app::ModelRoot) -> Result<FieldId, ManyToManyFault> {
    match (
        from.foreign_key.fields.as_slice(),
        to.primary_key.fields.as_slice(),
    ) {
        ([column], [key]) if column.target == *key => Ok(column.source),
        _ => Err(ManyToManyFault::CompositeKey),
    }
}

/// The target's field `from`'s single foreign-key column references.
fn target_key_of(from: &app::BelongsTo) -> Result<FieldId, ManyToManyFault> {
    match from.foreign_key.fields.as_slice() {
        [column] => Ok(column.target),
        _ => Err(ManyToManyFault::CompositeKey),
    }
}

/// `owner`'s single primary-key value.
fn owner_key<P>(owner: &P) -> toasty::Result<Value>
where
    P: toasty::schema::Model + IntoExpr<P>,
{
    match pk::pk_values(owner).as_slice() {
        [value] => Ok(value.clone()),
        _ => Err(toasty::Error::from_args(format_args!(
            "`{}` has no single-column primary key",
            std::any::type_name::<P>()
        ))),
    }
}

/// The predicate selecting the target records `owner` is linked to through `via`, one of `P`'s
/// `#[has_many(via = ..)]` fields: `pk IN (SELECT pk FROM <owner's via>)`.
pub(crate) fn linked_to<P, C>(
    owner: &P,
    via: Path<P, toasty::stmt::List<C>>,
) -> toasty::stmt::Expr<bool>
where
    P: toasty::schema::Model + IntoExpr<P>,
    C: toasty::schema::Model,
{
    let source = toasty::stmt::Query::<toasty::stmt::List<P>>::all().filter(pk::pk_filter(owner));
    let association = toasty::stmt::Association::many(source, via);
    let mut linked = toasty::stmt::IntoStatement::into_statement(association)
        .into_untyped()
        .into_query_unwrap();
    let Some(&[key]) = C::schema()
        .as_root()
        .map(|root| root.primary_key.fields.as_slice())
    else {
        return toasty::stmt::Expr::from_untyped(core::Expr::from(false));
    };
    // Toasty compares a record to a subquery of records only by a relation path; projecting the
    // key on both sides compares what the rows are.
    if let core::ExprSet::Select(select) = &mut linked.body {
        select.returning = core::Returning::Project(core::Expr::ref_self_field(key));
    }
    toasty::stmt::Expr::from_untyped(core::Expr::in_subquery(
        core::Expr::ref_self_field(key),
        linked,
    ))
}

#[cfg(test)]
mod tests;
