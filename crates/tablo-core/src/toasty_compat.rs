//! What Tablo builds on Toasty's internals because no public API offers it yet.
//!
//! This is the only module that names `toasty_core` or reads a model's `app`, `mapping` or `db`
//! schema; `tests/layers.rs` refuses either anywhere else. An item that fills an upstream gap
//! names it, and retires when Toasty closes that gap.
//!
//! - [`cursor`]: a pagination cursor's value as a URL token (tablo#398).
//! - [`join`]: a many-to-many relation's links, written as its join model's rows (Toasty writes no
//!   multi-step `via`).
//! - [`model`]: a model's fields, columns, indices and relations, and an embedded path's column
//!   (tablo#183).
//! - [`predicate`]: the root fields a predicate reads, which Toasty's typed `Expr` does not tell.
//! - [`pk`]: the primary key read off an instance and spelled as a URL id that parses back into a
//!   predicate through `toasty_core`'s untyped expressions.
//! - [`value_text`]: a value's text, for URL ids and enum discriminants.
//! - [`case_insensitive_search`]: a search predicate's `LIKE` matches run as `ILIKE` where the
//!   driver has it.
//! - [`count`]: a count that reads 0 where Toasty answers no value.
//! - [`RowUpdate`]: an update setting fields by typed path, for code generic over its model, which
//!   has no generated update builder.
//! - [`UntypedInclude`], [`Assignments`] and [`IsolationLevel`]: the `toasty_core` types a public
//!   Toasty API takes without re-exporting them.
//! - [`VariantId`]: an embedded enum variant's id, re-exported for the derives.

pub(crate) mod cursor;
pub(crate) mod join;
pub(crate) mod model;
pub(crate) mod pk;
pub(crate) mod predicate;

use toasty::stmt::{Expr, IntoExpr, Value};
use toasty_core::stmt::{ExprLike, VisitMut, visit_mut};

/// An include with its model and target types erased, which `Query::include` takes.
pub(crate) type UntypedInclude = toasty_core::stmt::Include;

/// An embedded enum variant's id, which the derives name.
pub use toasty::schema::app::VariantId;
/// The isolation level `toasty::TransactionBuilder::isolation` takes.
pub(crate) use toasty_core::driver::operation::IsolationLevel;
/// The assignments `toasty::schema::Field::new_update` takes.
pub(crate) use toasty_core::stmt::Assignments;

/// How many rows `query` holds.
///
/// Toasty answers a count whose filter it folds to `false`, such as two filters asking one column
/// for two values, with no value rather than 0, which `Query::count` fails to read as a `u64`. The
/// count reads it as an `Option` instead.
pub(crate) async fn count<M: toasty::schema::Model>(
    query: toasty::stmt::Query<toasty::stmt::List<M>>,
    ex: &mut dyn toasty::Executor,
) -> toasty::Result<u64> {
    let untyped = toasty::stmt::IntoStatement::into_statement(query.count()).into_untyped();
    let rows = toasty::stmt::Statement::<Option<u64>>::from_untyped_stmt(untyped)
        .exec(ex)
        .await?;
    Ok(rows.unwrap_or(0))
}

/// The text a scalar value is spelled as in a URL or a form: the inverse of the parse
/// [`pk::pk_eq_expr`] applies. `None` for a value with no text form.
pub(crate) fn value_text(value: &Value) -> Option<String> {
    Some(match value {
        Value::Bool(v) => v.to_string(),
        Value::I8(v) => v.to_string(),
        Value::I16(v) => v.to_string(),
        Value::I32(v) => v.to_string(),
        Value::I64(v) => v.to_string(),
        Value::U8(v) => v.to_string(),
        Value::U16(v) => v.to_string(),
        Value::U32(v) => v.to_string(),
        Value::U64(v) => v.to_string(),
        Value::F32(v) => v.to_string(),
        Value::F64(v) => v.to_string(),
        Value::String(v) => v.clone(),
        Value::Uuid(v) => v.to_string(),
        Value::Timestamp(v) => v.to_string(),
        Value::Date(v) => v.to_string(),
        Value::Time(v) => v.to_string(),
        Value::DateTime(v) => v.to_string(),
        Value::Zoned(v) => v.to_string(),
        Value::Bytes(v) => String::from_utf8(v.clone()).ok()?,
        _ => return None,
    })
}

/// An update of the `M` rows a filter selects, setting the root fields typed paths name: what a
/// model's generated update builder writes, for code generic over the model, which cannot name
/// the builder's setters.
///
/// It starts from that builder, so the fields the model stamps on every update, such as an
/// `#[update(Timestamp::now())]` column, are set too.
pub(crate) struct RowUpdate {
    update: toasty_core::stmt::Update,
}

impl RowUpdate {
    /// An update of the `M` rows `filter` selects, setting nothing yet but the model's own
    /// update stamps, and returning nothing.
    pub(crate) fn new<M>(filter: Expr<bool>) -> Self
    where
        M: toasty::schema::Model<
                UpdateQuery: From<toasty::stmt::Query<toasty::stmt::List<M>>>
                                 + toasty::stmt::IntoStatement<Returning = ()>,
            >,
    {
        let query = toasty::stmt::Query::<toasty::stmt::List<M>>::all().filter(filter);
        let builder = M::UpdateQuery::from(query);
        let toasty_core::stmt::Statement::Update(mut update) =
            toasty::stmt::IntoStatement::into_statement(builder).into_untyped()
        else {
            unreachable!("a model's update builder builds an update statement");
        };
        update.returning = None;
        Self { update }
    }

    /// Sets the root field `path` names to `value`.
    pub(crate) fn set<M, T: IntoExpr<T>>(&mut self, path: &toasty::stmt::Path<M, T>, value: T) {
        let path: toasty_core::stmt::Path = path.clone().into();
        let value: toasty_core::stmt::Expr = value.into_expr().into();
        self.update.assignments.set(path.projection, value);
    }

    /// Runs the update through `ex`.
    pub(crate) async fn exec(self, ex: &mut dyn toasty::Executor) -> toasty::Result<()> {
        toasty::stmt::Statement::<()>::from_untyped_stmt(self.update.into())
            .exec(ex)
            .await
    }
}

/// A filter no record matches.
pub(crate) fn never() -> Expr<bool> {
    Expr::from_untyped(toasty_core::stmt::Expr::from(false))
}

/// `predicate` with its `LIKE` matches run as `ILIKE` when `db`'s driver has the operator
/// (PostgreSQL), so a search is case-insensitive there as SQLite's `LIKE` and MySQL's default
/// collations already are.
///
/// Toasty's `ilike` builds a case-insensitive match, but a table and a record title declare their
/// search before any `Db` exists, and Toasty refuses `ILIKE` on a driver without it, so the
/// search switches when a request runs it.
pub(crate) fn case_insensitive_search(db: &toasty::Db, predicate: Expr<bool>) -> Expr<bool> {
    if db.capability().native_ilike {
        ilike(predicate)
    } else {
        predicate
    }
}

/// `predicate` with every `LIKE` in it, nested ones included, made case-insensitive.
fn ilike(predicate: Expr<bool>) -> Expr<bool> {
    struct Ilike;
    impl VisitMut for Ilike {
        fn visit_expr_like_mut(&mut self, like: &mut ExprLike) {
            like.case_insensitive = true;
            visit_mut::visit_expr_like_mut(self, like);
        }
    }
    let mut untyped = toasty_core::stmt::Expr::from(predicate);
    Ilike.visit_expr_mut(&mut untyped);
    Expr::from_untyped(untyped)
}

#[cfg(test)]
mod tests;
