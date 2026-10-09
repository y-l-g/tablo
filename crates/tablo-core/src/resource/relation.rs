//! Relation tables: a record page's related rows through the related resource's own list table.

use std::{any::TypeId, collections::HashMap, future::Future, pin::Pin, sync::Arc};

use toasty::stmt::{Expr, IntoExpr, List, Path};
use topcoat::{Result, context::Cx};

use super::{ErasedInput, InputResult, InputSpec, Resource, can, scoped_query};
use crate::{
    form::{FieldError, FormScalar},
    naming::{sentence_case, type_short_name},
    policy::Ability,
    schema::{Field, Schema},
    toasty_compat::{
        join::{JoinTable, linked_to},
        model::{self, AppSchema, ModelPath},
        pk,
    },
};

/// One relation of a parent resource's records.
///
/// Declared by [`ResourceDef::relation`](super::ResourceDef::relation); the related resource
/// must be registered on the same panel, which keys the relation by its slug:
///
/// ```rust
/// # use tablo_core::{NoForm, Relation, Resource, ResourceDef};
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     #[has_many]
/// #     comments: toasty::Deferred<Vec<Comment>>,
/// # }
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Comment {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     #[index]
/// #     post_id: uuid::Uuid,
/// #     #[belongs_to(key = post_id, references = id)]
/// #     post: toasty::Deferred<Post>,
/// # }
/// # struct PostResource;
/// # impl Resource for PostResource { type Model = Post; type Form = NoForm<Post>; }
/// # struct CommentResource;
/// # impl Resource for CommentResource { type Model = Comment; type Form = NoForm<Comment>; }
/// let def: ResourceDef<PostResource> =
///     ResourceDef::new().relation(Relation::has_many::<CommentResource>(Comment::fields().post_id()));
/// ```
pub struct Relation<P> {
    pub(crate) child: TypeId,
    pub(crate) child_name: &'static str,
    pub(crate) label: Option<String>,
    pub(crate) kind: RelationKind<P>,
    bind: BindFn<P>,
    /// Why the relation's field binds nothing, when it does not.
    pub(crate) misdeclared: Option<crate::DeclarationErrorKind>,
}

/// How a relation's child rows belong to the owner.
pub(crate) enum RelationKind<P> {
    /// Each child row holds the owner's key in this column, which a create from the table seeds.
    HasMany { foreign_key: String },
    /// The owner links child records through a join model, which the table attaches and detaches.
    ManyToMany(Links<P>),
}

/// An owner's rows filter on the child.
type BindFn<P> = Arc<dyn Fn(&P) -> (Expr<bool>, String) + Send + Sync>;

/// What linking or unlinking records returns.
type LinkFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// A many-to-many relation's writes: link the child record a key names to an owner, or unlink
/// the ones keys name.
pub(crate) struct Links<P> {
    /// The owner's `#[has_many(via = ..)]` field.
    pub(crate) field: String,
    /// The attach dialog's input: a choice of one child record.
    pub(crate) input: InputSpec,
    /// Links the record the key names, which the input's re-check proved a child record in
    /// scope that the user may view; linking a linked record changes nothing.
    pub(crate) attach: LinkFn<P, String>,
    /// Unlinks the records the keys name: 404 unless each is linked and in scope, 403 unless the
    /// user may view each.
    pub(crate) detach: LinkFn<P, Vec<String>>,
}

/// A write of an owner's links, given what names the related records.
type LinkFn<P, K> = Arc<
    dyn for<'a> Fn(&'a Cx, &'a P, K, &'a mut dyn toasty::Executor) -> LinkFuture<'a> + Send + Sync,
>;

/// The input key the attach dialog's choice posts.
const ATTACH_KEY: &str = "record";

/// A foreign-key column type referencing a primary key of type `K`: `K` itself, or `Option<K>`
/// for an optional reference. Sealed: these are the only two.
pub trait ForeignKey<K>:
    sealed::Sealed<K> + IntoExpr<Self> + FormScalar + Send + Sync + 'static
{
}

impl<K> ForeignKey<K> for K where K: IntoExpr<K> + FormScalar + Send + Sync + 'static {}

impl<K> ForeignKey<K> for Option<K> where
    Option<K>: IntoExpr<Self> + FormScalar + Send + Sync + 'static
{
}

mod sealed {
    pub trait Sealed<K> {}

    impl<K> Sealed<K> for K {}

    impl<K> Sealed<K> for Option<K> {}
}

impl<P> Relation<P>
where
    P: toasty::schema::Model + IntoExpr<P> + 'static,
{
    /// Declare a `has_many` child resource whose `foreign_key` holds the owner's primary key, a
    /// single column.
    ///
    /// ```rust
    /// # use tablo_core::{NoForm, Relation, Resource, ResourceDef};
    /// # #[derive(Debug, Clone, toasty::Model)]
    /// # struct Post {
    /// #     #[key] #[auto] id: uuid::Uuid,
    /// #     #[has_many]
    /// #     comments: toasty::Deferred<Vec<Comment>>,
    /// # }
    /// # #[derive(Debug, Clone, toasty::Model)]
    /// # struct Comment {
    /// #     #[key] #[auto] id: uuid::Uuid,
    /// #     #[index]
    /// #     post_id: uuid::Uuid,
    /// #     #[belongs_to(key = post_id, references = id)]
    /// #     post: toasty::Deferred<Post>,
    /// # }
    /// # struct PostResource;
    /// # impl Resource for PostResource { type Model = Post; type Form = NoForm<Post>; }
    /// # struct CommentResource;
    /// # impl Resource for CommentResource { type Model = Comment; type Form = NoForm<Comment>; }
    /// let comments: Relation<Post> = Relation::has_many::<CommentResource>(Comment::fields().post_id());
    /// ```
    pub fn has_many<C>(foreign_key: Path<C::Model, impl ForeignKey<P::PrimaryKey>>) -> Self
    where
        C: Resource,
    {
        Self::bound_by::<C, _>(foreign_key)
    }

    fn bound_by<C, T>(foreign_key: Path<C::Model, T>) -> Self
    where
        C: Resource,
        T: ForeignKey<P::PrimaryKey>,
    {
        // A foreign key is one column of the child: it binds against the child's model alone.
        let (column, misdeclared) = match model::field::<C::Model>(&ModelPath::of(&foreign_key)) {
            Ok(field) => (field.name, None),
            Err(error) => (String::new(), Some(error)),
        };
        Self {
            child: TypeId::of::<C>(),
            child_name: std::any::type_name::<C>(),
            label: None,
            kind: RelationKind::HasMany {
                foreign_key: column,
            },
            misdeclared,
            bind: Arc::new(move |owner| {
                let scope = foreign_key.clone().eq(pk::pk_expr::<P, T>(owner));
                (scope, pk::pk_text(owner))
            }),
        }
    }

    /// Declare a many-to-many child resource whose records the owner links through `via`, its
    /// `#[has_many(via = joins.target)]` field: the owner's linked records, which the table
    /// attaches and detaches as join rows.
    ///
    /// ```rust
    /// # use tablo_core::{NoForm, Relation, Resource, ResourceDef};
    /// # #[derive(Debug, Clone, toasty::Model)]
    /// # struct Post {
    /// #     #[key] #[auto] id: uuid::Uuid,
    /// #     #[has_many]
    /// #     taggings: toasty::Deferred<Vec<Tagging>>,
    /// #     #[has_many(via = taggings.tag)]
    /// #     tags: toasty::Deferred<Vec<Tag>>,
    /// # }
    /// # #[derive(Debug, Clone, toasty::Model)]
    /// # struct Tag { #[key] #[auto] id: uuid::Uuid, name: String }
    /// # #[derive(Debug, Clone, toasty::Model)]
    /// # struct Tagging {
    /// #     #[key] #[auto] id: uuid::Uuid,
    /// #     #[index] post_id: uuid::Uuid,
    /// #     #[belongs_to(key = post_id, references = id)]
    /// #     post: toasty::Deferred<Post>,
    /// #     #[index] tag_id: uuid::Uuid,
    /// #     #[belongs_to(key = tag_id, references = id)]
    /// #     tag: toasty::Deferred<Tag>,
    /// # }
    /// # struct TagResource;
    /// # impl Resource for TagResource { type Model = Tag; type Form = NoForm<Tag>; }
    /// let tags: Relation<Post> = Relation::belongs_to_many::<TagResource>(Post::fields().tags());
    /// ```
    ///
    /// Attaching and detaching ask the owner's policy for `Update`, as editing it does, and the
    /// related resource's for `View` on each record; mounting refuses a join model a link cannot
    /// write.
    pub fn belongs_to_many<C>(via: impl Into<Path<P, List<C::Model>>>) -> Self
    where
        C: Resource,
        P: Send + Sync,
    {
        let via: Path<P, List<C::Model>> = via.into();
        let (field, misdeclared) = match model::field::<P>(&ModelPath::of(&via)) {
            Ok(field) => (field.name, None),
            Err(error) => (String::new(), Some(error)),
        };
        let attach_via = via.clone();
        let attach_field = field.clone();
        let detach_via = via.clone();
        let detach_field = field.clone();
        Self {
            child: TypeId::of::<C>(),
            child_name: std::any::type_name::<C>(),
            label: None,
            kind: RelationKind::ManyToMany(Links {
                field,
                input: InputSpec {
                    schema: attach_schema::<C>,
                    takes_input: true,
                    parse: parse_attach,
                },
                attach: Arc::new(move |cx, owner, key, ex| {
                    let (via, field) = (attach_via.clone(), attach_field.clone());
                    Box::pin(async move { attach::<P, C>(cx, owner, via, &field, key, ex).await })
                }),
                detach: Arc::new(move |cx, owner, keys, ex| {
                    let (via, field) = (detach_via.clone(), detach_field.clone());
                    Box::pin(async move { detach::<P, C>(cx, owner, via, &field, keys, ex).await })
                }),
            }),
            misdeclared,
            bind: Arc::new(move |owner| (linked_to(owner, via.clone()), pk::pk_text(owner))),
        }
    }
}

/// The attach dialog's input: one searchable choice of `C`'s records.
fn attach_schema<C: Resource>() -> Schema {
    Schema::new(
        Field::choice_input(ATTACH_KEY)
            .label(sentence_case(type_short_name::<C::Model>()))
            .relationship::<C>()
            .searchable(),
    )
}

/// The key the attach dialog's choice posts: the schema's check proved it one of the choice's
/// records.
fn parse_attach(_cx: &Cx, values: &HashMap<String, String>) -> InputResult {
    match values.get(ATTACH_KEY).map(|key| key.trim()) {
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
async fn attach<P, C>(
    cx: &Cx,
    owner: &P,
    via: Path<P, List<C::Model>>,
    field: &str,
    key: String,
    ex: &mut dyn toasty::Executor,
) -> Result<()>
where
    P: toasty::schema::Model + IntoExpr<P>,
    C: Resource,
{
    let join = join_table::<P>(cx, field)?;
    let Some(record) = pk::pk_eq_expr::<C::Model>(&key) else {
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
        join.link(owner, &[key], ex)
            .await
            .map_err(|error| -> topcoat::Error { error.into() })?;
    }
    Ok(())
}

/// Unlinks `owner` from the records `keys` names.
async fn detach<P, C>(
    cx: &Cx,
    owner: &P,
    via: Path<P, List<C::Model>>,
    field: &str,
    keys: Vec<String>,
    ex: &mut dyn toasty::Executor,
) -> Result<()>
where
    P: toasty::schema::Model + IntoExpr<P>,
    C: Resource,
{
    let join = join_table::<P>(cx, field)?;
    let ids: Vec<&str> = keys.iter().map(String::as_str).collect();
    let Some(records) = pk::pk_in_expr::<C::Model>(&ids) else {
        return Err(topcoat::router::error::not_found().into());
    };
    let linked = scoped_query::<C>(cx)?
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
        .map_err(|error| -> topcoat::Error { error.into() })
}

impl<P> Relation<P> {
    /// Titles the section `label` instead of the related resource's plural label.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The child's rows that belong to `owner`, and the owner's primary key as text.
    pub(crate) fn bind(&self, owner: &P) -> (Expr<bool>, String) {
        (self.bind)(owner)
    }
}

impl<P> Relation<P> {
    /// The relation's join-row writes, when it is a many-to-many one.
    pub(crate) fn links(&self) -> Option<&Links<P>> {
        match &self.kind {
            RelationKind::ManyToMany(links) => Some(links),
            RelationKind::HasMany { .. } => None,
        }
    }
}

impl<P> std::fmt::Debug for Relation<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("Relation");
        debug
            .field("child", &self.child_name)
            .field("label", &self.label);
        match &self.kind {
            RelationKind::HasMany { foreign_key } => debug.field("foreign_key", foreign_key),
            RelationKind::ManyToMany(links) => debug.field("via", &links.field),
        };
        debug.finish_non_exhaustive()
    }
}
