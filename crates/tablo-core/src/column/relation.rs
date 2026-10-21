//! [`RelationColumn`] and [`CountColumn`]: columns over a relation field, which load the relation
//! they read, and the [`RelationLens`] that names the field, built by
//! [`relation!`](crate::relation!).

use std::sync::Arc;

use derive_where::derive_where;
use toasty::{Deferred, Executor};
use topcoat::{Result, context::Cx};

use super::{Column, ColumnToggle, ColumnWidth, Includes, Related};
use crate::{BoxFuture, DeclarationErrorKind, lens::Reads, schema::OptionSource};

mod lens;
mod restrict;

pub use lens::RelationLens;
use restrict::{Restrict, restrict_many, restrict_one, shows};

/// The text an unscoped relation column renders for a row whose relation was not loaded.
const UNLOADED: &str = "(unloaded)";

/// What a relation column reads off a row in the request's context, or `None` when the row's
/// relation was not loaded.
type Loaded<M, T> = Arc<dyn Fn(&Cx, &M) -> Option<T> + Send + Sync>;

/// A label source's type name, and whether a context's panel can load from it.
type LabelSource = (&'static str, fn(&Cx) -> bool);

/// Whether a request may not view a field of the related records a column shows.
type Masked = Arc<dyn Fn(&Cx) -> bool + Send + Sync>;

/// The text a relation column shows for a row, which a source's label reads in the request's
/// context, or `None` when the row's relation was not loaded.
type LoadedText<M> = Arc<dyn Fn(&Cx, &M) -> Option<String> + Send + Sync>;

/// A to-one relation field to `T` a [`RelationColumn`] reads: `Deferred<T>`, or
/// `Deferred<Option<T>>` for a nullable relation.
///
/// `T` is inferred from the field: `Deferred<X>` matches the first impl with `T = X` alone,
/// since an `Option` is not a model, and the two impls cannot overlap, which would need
/// `T = Option<T>`.
pub trait ToOneRelation<T>: sealed::Sealed<T> {
    /// The related record, `Some(None)` for a nullable relation holding none, or `None` when the
    /// relation was not loaded.
    #[doc(hidden)]
    fn loaded(&self) -> Option<Option<&T>>;
}

mod sealed {
    pub trait Sealed<T> {}

    impl<T: toasty::schema::Model> Sealed<T> for toasty::Deferred<T> {}
    impl<T: toasty::schema::Model> Sealed<T> for toasty::Deferred<Option<T>> {}
}

impl<T: toasty::schema::Model> ToOneRelation<T> for Deferred<T> {
    fn loaded(&self) -> Option<Option<&T>> {
        (!self.is_unloaded()).then(|| Some(self.get()))
    }
}

impl<T: toasty::schema::Model> ToOneRelation<T> for Deferred<Option<T>> {
    fn loaded(&self) -> Option<Option<&T>> {
        (!self.is_unloaded()).then(|| self.get().as_ref())
    }
}

/// A column showing the record of a `Deferred` `belongs_to` or `has_one` field as text, or the
/// records of a `has_many` one ([`list`](Self::list)), which declares the field's include so every
/// page that renders it loads them.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Author { #[key] #[auto] id: uuid::Uuid, name: String }
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     author_id: uuid::Uuid,
/// #     #[belongs_to(key = author_id, references = id)]
/// #     author: toasty::Deferred<Author>,
/// # }
/// # struct AuthorResource;
/// # impl tablo_core::extend::OptionSource for AuthorResource {
/// #     type Model = Author;
/// #     fn scoped_query(_cx: &topcoat::context::Cx)
/// #         -> topcoat::Result<toasty::stmt::Query<toasty::stmt::List<Author>>>
/// #     {
/// #         Ok(toasty::stmt::Query::all())
/// #     }
/// #     fn label(_cx: &topcoat::context::Cx, author: &Author) -> String {
/// #         author.name.clone()
/// #     }
/// # }
/// use tablo_core::{RelationColumn, lens, relation};
///
/// RelationColumn::new::<AuthorResource>(
///     relation!(Post.author),
///     lens!(Author.name),
///     |a: &Author| a.name.clone(),
/// );
/// ```
///
/// The header and the column's name are the field's name, capitalized, until
/// [`label`](Self::label) replaces both. The column neither searches nor sorts.
///
/// A column over a source `S` shows only the related records `S` shows the user: the ones its
/// [`scoped_query`](OptionSource::scoped_query) returns and its policy lets the user view, as a
/// relationship choice over `S` offers them. A related record it hides, soft-deleted or another
/// tenant's, renders empty, and so does every record of a row loaded around the table's or the
/// [`Detail`](crate::Detail)'s loader, which checks them. The row keeps what the include loaded,
/// so a policy reading the relation sees it whole. [`unscoped`](Self::unscoped) shows whatever
/// the include loads.
///
/// A field [`field_access`](crate::ResourceDef::field_access) restricts shows to a user only
/// where the user may view it, on every resource the panel mounts over its model: a request that
/// may not view a field [`new`](Self::new) reads finds no such column. A record's
/// [`label`](OptionSource::label) is its title, which the mount keeps clear of every restricted
/// field, or its key, which no field access may hide.
#[derive_where(Clone, Debug)]
pub struct RelationColumn<M> {
    relation: Declared<M>,
    #[derive_where(skip(Debug))]
    project: LoadedText<M>,
}

impl<M> RelationColumn<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    /// Declare a column rendering `project` of the record `relation` loads, when `S` shows it.
    /// `reads` names the fields of the record `project` reads, a [`lens!`](crate::lens!) or a
    /// [`Reads`]: a request `S` hides one of them from finds no such column.
    pub fn new<S>(
        relation: RelationLens<M, impl ToOneRelation<S::Model> + 'static>,
        reads: impl Into<Reads<S::Model>>,
        project: impl Fn(&S::Model) -> String + Send + Sync + 'static,
    ) -> Self
    where
        S: OptionSource,
    {
        let read = relation.read;
        let reads = reads.into();
        Self {
            relation: Declared::of(&relation, ColumnWidth::Grow)
                .scoped::<S>(restrict_one::<M, S, _>(relation.read))
                .masked(Arc::new(move |cx| reads.reads_any(&S::hidden_fields(cx)))),
            project: Arc::new(move |cx, row| {
                read(row).loaded().map(|target| {
                    target
                        .filter(|target| shows::<S>(cx, target))
                        .map(&project)
                        .unwrap_or_default()
                })
            }),
        }
    }

    /// Declare a column showing the record `relation` loads by `S`'s
    /// [`label`](OptionSource::label), when `S` shows it: for a resource, the
    /// [`record_title`](crate::ResourceDef::record_title) its detail page is titled with.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Author { #[key] #[auto] id: uuid::Uuid, name: String }
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post {
    /// #     #[key] #[auto] id: uuid::Uuid,
    /// #     author_id: uuid::Uuid,
    /// #     #[belongs_to(key = author_id, references = id)]
    /// #     author: toasty::Deferred<Author>,
    /// # }
    /// # struct AuthorResource;
    /// # impl tablo_core::extend::OptionSource for AuthorResource {
    /// #     type Model = Author;
    /// #     fn scoped_query(_cx: &topcoat::context::Cx)
    /// #         -> topcoat::Result<toasty::stmt::Query<toasty::stmt::List<Author>>>
    /// #     {
    /// #         Ok(toasty::stmt::Query::all())
    /// #     }
    /// #     fn label(_cx: &topcoat::context::Cx, author: &Author) -> String {
    /// #         author.name.clone()
    /// #     }
    /// # }
    /// use tablo_core::{RelationColumn, relation};
    ///
    /// RelationColumn::of::<AuthorResource>(relation!(Post.author));
    /// ```
    pub fn of<S>(relation: RelationLens<M, impl ToOneRelation<S::Model> + 'static>) -> Self
    where
        S: OptionSource,
    {
        let read = relation.read;
        Self {
            relation: Declared::of(&relation, ColumnWidth::Grow)
                .scoped::<S>(restrict_one::<M, S, _>(relation.read)),
            project: Arc::new(move |cx, row| {
                read(row).loaded().map(|target| {
                    target
                        .filter(|target| shows::<S>(cx, target))
                        .map(|target| S::label(cx, target))
                        .unwrap_or_default()
                })
            }),
        }
    }

    /// Declare a column listing the records a `has_many` `relation` loads that `S` shows, a
    /// many-to-many `#[has_many(via = ..)]` one included, each by `S`'s
    /// [`label`](OptionSource::label), joined by commas.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post {
    /// #     #[key] #[auto] id: uuid::Uuid,
    /// #     #[has_many]
    /// #     taggings: toasty::Deferred<Vec<Tagging>>,
    /// #     #[has_many(via = taggings.tag)]
    /// #     tags: toasty::Deferred<Vec<Tag>>,
    /// # }
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
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
    /// # impl tablo_core::extend::OptionSource for TagResource {
    /// #     type Model = Tag;
    /// #     fn scoped_query(_cx: &topcoat::context::Cx)
    /// #         -> topcoat::Result<toasty::stmt::Query<toasty::stmt::List<Tag>>>
    /// #     {
    /// #         Ok(toasty::stmt::Query::all())
    /// #     }
    /// #     fn label(_cx: &topcoat::context::Cx, tag: &Tag) -> String {
    /// #         tag.name.clone()
    /// #     }
    /// #     fn allows(_cx: &topcoat::context::Cx, _: tablo_core::Ability<'_, Tag>) -> bool {
    /// #         true
    /// #     }
    /// # }
    /// use tablo_core::{RelationColumn, relation};
    ///
    /// RelationColumn::list::<TagResource>(relation!(Post.tags));
    /// ```
    pub fn list<S>(relation: RelationLens<M, Deferred<Vec<S::Model>>>) -> Self
    where
        S: OptionSource,
    {
        let read = relation.read;
        Self {
            relation: Declared::of(&relation, ColumnWidth::Grow)
                .scoped::<S>(restrict_many::<M, S>(relation.read)),
            project: Arc::new(move |cx, row| {
                let records = read(row);
                (!records.is_unloaded()).then(|| {
                    records
                        .get()
                        .iter()
                        .filter(|record| shows::<S>(cx, record))
                        .map(|record| S::label(cx, record))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
            }),
        }
    }

    /// Declare a column rendering `project` of whatever record `relation` loads, under no
    /// resource's query, policy or field access: for a related model no resource administers. A
    /// panel mounting a resource that restricts a field of the related model refuses it.
    pub fn unscoped<T>(
        relation: RelationLens<M, impl ToOneRelation<T> + 'static>,
        project: impl Fn(&T) -> String + Send + Sync + 'static,
    ) -> Self {
        let read = relation.read;
        Self {
            relation: Declared::of(&relation, ColumnWidth::Grow),
            project: Arc::new(move |_cx, row| {
                read(row)
                    .loaded()
                    .map(|target| target.map(&project).unwrap_or_default())
            }),
        }
    }
}

impl<M> RelationColumn<M> {
    /// The row's text, or `None` when its relation was not loaded.
    fn loaded_text(&self, cx: &Cx, row: &M) -> Option<String> {
        (self.project)(cx, row)
    }
}

/// A column counting the records of a `Deferred` `has_many` field that a source `S` shows, which
/// declares the field's include so every page that renders it loads the records.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     #[has_many]
/// #     comments: toasty::Deferred<Vec<Comment>>,
/// # }
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
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
/// The count is how many of the loaded records `S` shows the user, which [`RelationColumn`]
/// decides the same way: a page loads every related record of every row it counts.
/// [`unscoped`](Self::unscoped) counts every record the include loads.
#[derive_where(Clone, Debug)]
pub struct CountColumn<M> {
    relation: Declared<M>,
    #[derive_where(skip(Debug))]
    count: Loaded<M, usize>,
}

impl<M> CountColumn<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    /// Declare a column counting the records `relation` loads that `S` shows.
    pub fn of<S>(relation: RelationLens<M, Deferred<Vec<S::Model>>>) -> Self
    where
        S: OptionSource,
    {
        Self {
            relation: Declared::of(&relation, ColumnWidth::Auto)
                .scoped::<S>(restrict_many::<M, S>(relation.read))
                .fieldless(),
            count: count(relation.read, shows::<S>),
        }
    }

    /// Declare a column counting every record `relation` loads, under no resource's query or
    /// policy: for a related model no resource administers. A count shows no field of the
    /// records.
    pub fn unscoped<T>(relation: RelationLens<M, Deferred<Vec<T>>>) -> Self
    where
        T: toasty::schema::Model + 'static,
    {
        Self {
            relation: Declared::of(&relation, ColumnWidth::Auto).fieldless(),
            count: count(relation.read, |_, _| true),
        }
    }
}

impl<M> CountColumn<M> {
    /// The row's count, or `None` when its relation was not loaded.
    fn loaded_text(&self, cx: &Cx, row: &M) -> Option<String> {
        (self.count)(cx, row).map(|count| count.to_string())
    }
}

/// How many records of the list `read` reads off a row `shown` accepts, or `None` when it was not
/// loaded.
fn count<M: 'static, T: 'static>(
    read: fn(&M) -> &Deferred<Vec<T>>,
    shown: fn(&Cx, &T) -> bool,
) -> Loaded<M, usize> {
    Arc::new(move |cx, row| {
        let records = read(row);
        (!records.is_unloaded()).then(|| records.get().iter().filter(|r| shown(cx, r)).count())
    })
}

/// What both relation-field columns declare: the name and label, the include and the width.
#[derive_where(Clone, Debug)]
struct Declared<M> {
    name: String,
    /// The declared label, over the field's.
    label: Option<String>,
    field_label: String,
    misdeclared: Option<DeclarationErrorKind>,
    includes: Includes<M>,
    reads: crate::lens::Reads<M>,
    width: ColumnWidth,
    toggle: ColumnToggle,
    /// The source that decides which related records show, and whether a context's panel can
    /// load from it.
    source: Option<LabelSource>,
    /// Drops the related records the source does not show; `None` for an unscoped column.
    #[derive_where(skip(Debug))]
    restrict: Option<Restrict<M>>,
    /// What the column shows of the related records, which a panel checks at mount.
    related: Related,
    /// Whether a request may not view a field the column shows; `None` for one showing only a
    /// record's label or count.
    #[derive_where(skip(Debug))]
    masked: Option<Masked>,
}

impl<M> Declared<M> {
    fn of<R>(relation: &RelationLens<M, R>, width: ColumnWidth) -> Self {
        let (name, field_label, misdeclared) = match relation.field() {
            Ok((name, label)) => (name.to_string(), label.to_string(), None),
            Err(error) => (String::new(), String::new(), Some(error)),
        };
        Self {
            name,
            label: None,
            field_label,
            misdeclared,
            includes: relation.includes().clone(),
            reads: relation.reads(),
            width,
            toggle: ColumnToggle::Fixed,
            source: None,
            restrict: None,
            related: Related::Unmasked,
            masked: None,
        }
    }

    /// Show only the related records `S` shows, which `restrict` keeps, by their label or count
    /// until [`masked`](Self::masked) says what else shows.
    fn scoped<S: OptionSource>(self, restrict: Restrict<M>) -> Self {
        Self {
            source: Some((std::any::type_name::<S>(), S::available)),
            restrict: Some(restrict),
            related: Related::Source(std::any::TypeId::of::<S>()),
            ..self
        }
    }

    /// Show the fields of the related records `masked` hides from a request it refuses.
    fn masked(self, masked: Masked) -> Self {
        Self {
            masked: Some(masked),
            ..self
        }
    }

    /// Show no field of the related records.
    fn fieldless(self) -> Self {
        Self {
            related: Related::None,
            ..self
        }
    }

    fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.field_label)
    }

    fn relabel(&mut self, label: String) {
        self.name = label.to_lowercase();
        self.label = Some(label);
    }

    /// The text of a row's loaded relation. A scoped column renders empty when its row holds no
    /// record, which is how a required related record the source hides reads. An unscoped one
    /// renders the unloaded marker, since a loader skipped the include it declares.
    fn text(&self, loaded: Option<String>) -> String {
        if self.restrict.is_some() {
            return loaded.unwrap_or_default();
        }
        debug_assert!(
            loaded.is_some(),
            "the `{}` column declares its include, but its row was loaded without it",
            self.name
        );
        loaded.unwrap_or_else(|| UNLOADED.to_string())
    }
}

/// The builders and the [`Column`] impl both relation columns share.
macro_rules! relation_column {
    ($ty:ident) => {
        impl<M> $ty<M> {
            /// Replace the label the field's name gives it, and name the column after the label,
            /// as a [`ComputedColumn`](super::ComputedColumn) is: two columns over one relation
            /// need two labels.
            pub fn label(mut self, label: impl Into<String>) -> Self {
                self.relation.relabel(label.into());
                self
            }

            /// Declare this column's width.
            pub fn width(mut self, width: ColumnWidth) -> Self {
                self.relation.width = width;
                self
            }

            toggle_builders!(relation.toggle);
        }

        impl<M> Column<M> for $ty<M>
        where
            M: toasty::schema::Model + Send + Sync + 'static,
        {
            fn name(&self) -> &str {
                &self.relation.name
            }

            fn label(&self) -> &str {
                self.relation.label()
            }

            fn text(&self, cx: &Cx, row: &M) -> String {
                self.relation.text(self.loaded_text(cx, row))
            }

            fn column_width(&self) -> ColumnWidth {
                self.relation.width
            }

            fn toggle(&self) -> ColumnToggle {
                self.relation.toggle
            }

            fn includes(&self) -> Includes<M> {
                self.relation.includes.clone()
            }

            fn reads(&self) -> Option<crate::lens::Reads<M>> {
                Some(self.relation.reads.clone())
            }

            fn restrict<'a>(
                &'a self,
                cx: &'a Cx,
                rows: &'a [M],
                ex: &'a mut dyn Executor,
            ) -> BoxFuture<'a, Result<()>> {
                match &self.relation.restrict {
                    Some(restrict) => restrict(cx, rows, ex),
                    None => Box::pin(async { Ok(()) }),
                }
            }

            fn masked(&self, cx: &Cx) -> bool {
                self.relation
                    .masked
                    .as_ref()
                    .is_some_and(|masked| masked(cx))
            }

            fn related(&self) -> Related {
                self.relation.related
            }

            fn misdeclared(&self) -> Option<DeclarationErrorKind> {
                self.relation.misdeclared.clone()
            }

            fn unavailable_source(&self, cx: &Cx) -> Option<&'static str> {
                self.relation
                    .source
                    .filter(|(_, available)| !available(cx))
                    .map(|(source, _)| source)
            }
        }
    };
}

relation_column!(RelationColumn);
relation_column!(CountColumn);

#[cfg(test)]
mod tests;
