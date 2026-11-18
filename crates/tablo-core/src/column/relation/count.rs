//! [`CountColumn`]: how many related records each row on a page holds, read by one count query per
//! row rather than by loading the records.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
};

use derive_where::derive_where;
use toasty::{
    Deferred,
    stmt::{IntoExpr, List, Path, Query},
};
use topcoat::{Result, context::Cx};

use super::{ColumnWidth, Declared, RelationLens, restrict::Restrict};
use crate::{
    DeclarationErrorKind,
    schema::{OptionSource, ensure_option_access},
    toasty_compat::{join::linked_to, pk},
};

/// The query a count narrows to one row's related records, or `None` when the request may count
/// none.
type Scope<T> = fn(&Cx) -> Result<Option<Query<List<T>>>>;

/// A column counting the records of a `Deferred` `has_many` field that a source `S` shows, a
/// many-to-many `#[has_many(via = ..)]` one included.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     #[has_many]
/// #     comments: toasty::Deferred<Vec<Comment>>,
/// # }
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
/// # struct Comment {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     #[index]
/// #     post_id: uuid::Uuid,
/// #     #[belongs_to(key = post_id, references = id)]
/// #     post: toasty::Deferred<Post>,
/// # }
/// # struct CommentResource;
/// # impl tablo_core::extend::OptionSource for CommentResource {
/// #     type Model = Comment;
/// #     fn scoped_query(_cx: &topcoat::context::Cx)
/// #         -> topcoat::Result<toasty::stmt::Query<toasty::stmt::List<Comment>>>
/// #     {
/// #         Ok(toasty::stmt::Query::all())
/// #     }
/// #     fn label(_cx: &topcoat::context::Cx, comment: &Comment) -> String {
/// #         comment.id.to_string()
/// #     }
/// # }
/// use tablo_core::{CountColumn, relation};
///
/// CountColumn::of::<CommentResource>(relation!(Post.comments));
/// ```
///
/// The header and the column's name are the field's name, capitalized, until
/// [`label`](Self::label) replaces both.
///
/// The column loads no related record. Once a page's rows load, it counts each row's records in
/// the database, one count query per row: what `S`'s
/// [`scoped_query`](OptionSource::scoped_query) returns of them when it
/// [`lists`](OptionSource::lists) for the user, as a relationship choice over `S` offers them. A
/// request `S` does not list for, and a row loaded around the table's or the
/// [`Detail`](crate::Detail)'s loader, which runs the counts, renders empty.
/// [`unscoped`](Self::unscoped) counts every related record.
///
/// A panel refuses a count over a related model whose primary key spans several columns.
#[derive_where(Clone, Debug)]
pub struct CountColumn<M> {
    pub(super) relation: Declared<M>,
    /// Names the column's counts among the request's.
    key: Arc<str>,
    #[derive_where(skip(Debug))]
    row_key: fn(&M) -> String,
}

impl<M> CountColumn<M>
where
    M: toasty::schema::Model + IntoExpr<M> + Send + Sync + 'static,
{
    /// Declare a column counting the records of `relation` that `S` shows.
    pub fn of<S>(relation: RelationLens<M, Deferred<Vec<S::Model>>>) -> Self
    where
        S: OptionSource,
    {
        let scope: Scope<S::Model> = |cx| {
            if ensure_option_access::<S>(cx).is_err() {
                return Ok(None);
            }
            S::scoped_query(cx).map(Some)
        };
        let key = counts_key::<M, _>(&relation, std::any::type_name::<S>());
        let restrict = counter(key.clone(), via(&relation), scope);
        Self::declare(
            Declared::of(&relation, ColumnWidth::Auto).scoped::<S>(restrict),
            key,
            composite::<S::Model>(),
        )
    }

    /// Declare a column counting every record of `relation`, under no resource's query or
    /// policy: for a related model no resource administers. A count shows no field of the
    /// records.
    pub fn unscoped<T>(relation: RelationLens<M, Deferred<Vec<T>>>) -> Self
    where
        T: toasty::schema::Model + Send + Sync + 'static,
    {
        let scope: Scope<T> = |_| Ok(Some(Query::all()));
        let key = counts_key::<M, _>(&relation, "");
        let restrict = counter(key.clone(), via(&relation), scope);
        Self::declare(
            Declared::of(&relation, ColumnWidth::Auto).restricted(restrict),
            key,
            composite::<T>(),
        )
    }

    fn declare(relation: Declared<M>, key: Arc<str>, composite: bool) -> Self {
        let mut relation = relation.without_include().fieldless();
        if composite && relation.misdeclared.is_none() {
            relation.misdeclared = Some(DeclarationErrorKind::CompositeKeyCount {
                column: relation.name.clone(),
            });
        }
        Self {
            relation,
            key,
            row_key: pk::pk_text::<M>,
        }
    }
}

impl<M> CountColumn<M> {
    /// The row's count, or `None` when no count ran for it.
    pub(super) fn loaded_text(&self, cx: &Cx, row: &M) -> Option<String> {
        counts(cx, crate::tenancy::tenant_id(cx))
            .get(&self.key, &(self.row_key)(row))
            .map(|count| count.to_string())
    }
}

/// The path from `M` to the records `relation` holds, or `None` when it names no single field.
fn via<M, T>(relation: &RelationLens<M, Deferred<Vec<T>>>) -> Option<Path<M, List<T>>>
where
    M: toasty::schema::Model,
{
    relation.index.map(M::path_field::<List<T>>)
}

/// Whether `T`'s primary key spans several columns, which [`linked_to`] matches none of.
fn composite<T: toasty::schema::Model>() -> bool {
    pk::pk_is_composite::<T>()
}

/// What names a column's counts among the request's: the model, the relation and the source,
/// since two columns over one relation counted under different sources count apart.
fn counts_key<M, R>(relation: &RelationLens<M, R>, source: &str) -> Arc<str> {
    let field = relation.field().unwrap_or_default();
    Arc::from(format!("{}.{field}:{source}", std::any::type_name::<M>()))
}

/// Counts each row's records `via` holds that `scope` returns, and records the counts for the
/// request under `key`.
fn counter<M, T>(key: Arc<str>, via: Option<Path<M, List<T>>>, scope: Scope<T>) -> Restrict<M>
where
    M: toasty::schema::Model + IntoExpr<M> + Send + Sync + 'static,
    T: toasty::schema::Model + Send + Sync + 'static,
{
    Arc::new(move |cx, rows, ex| {
        let (key, via) = (key.clone(), via.clone());
        Box::pin(async move {
            let Some(via) = via else {
                return Ok(());
            };
            let store = counts(cx, crate::tenancy::tenant_id(cx));
            for row in rows {
                let id = pk::pk_text(row);
                if store.get(&key, &id).is_some() {
                    continue;
                }
                let Some(query) = scope(cx)? else {
                    return Ok(());
                };
                let count = crate::toasty_compat::count(
                    query.filter(linked_to(row, via.clone())),
                    &mut *ex,
                )
                .await
                .map_err(crate::error::unavailable)?;
                store.insert(&key, id, count);
            }
            Ok(())
        })
    })
}

/// The counts the request's count columns ran, by column and row.
#[derive(Default)]
struct Counts(Mutex<HashMap<(Arc<str>, String), u64>>);

impl Counts {
    fn get(&self, key: &Arc<str>, row: &str) -> Option<u64> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&(key.clone(), row.to_string()))
            .copied()
    }

    fn insert(&self, key: &Arc<str>, row: String, count: u64) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert((key.clone(), row), count);
    }
}

/// The request's [`Counts`] in `tenant`.
#[topcoat::context::memoize]
fn counts(cx: &Cx, _tenant: Option<crate::TenantId>) -> Arc<Counts> {
    let _ = cx;
    Arc::default()
}
