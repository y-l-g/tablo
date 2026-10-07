//! [`RelationColumn`] and [`CountColumn`]: columns over a relation field, which load the relation
//! they read, and the [`RelationLens`] that names the field, built by
//! [`relation!`](crate::relation!).

use std::sync::Arc;

use toasty::{Deferred, stmt::Path};

use super::{Column, ColumnWidth, Includes};
use crate::{DeclarationErrorKind, toasty_compat::model};

/// A relation field's include, which loads it, paired with the function that reads the loaded
/// relation off a record.
///
/// [`relation!`](crate::relation!) builds one from a single field name, so the column that reads
/// the relation always declares its include:
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Author { #[key] #[auto] id: uuid::Uuid, name: String }
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     author_id: uuid::Uuid,
/// #     #[belongs_to(key = author_id, references = id)]
/// #     author: toasty::Deferred<Author>,
/// # }
/// # use tablo_core::{RelationColumn, relation};
/// RelationColumn::new(relation!(Post.author), |a: &Author| a.name.clone());
/// ```
pub struct RelationLens<M, R> {
    includes: Includes<M>,
    /// The relation field's name, or why the path names no single field.
    field: Result<(String, String), DeclarationErrorKind>,
    read: fn(&M) -> &R,
}

impl<M, R> RelationLens<M, R>
where
    M: toasty::schema::Model,
{
    /// Pair the relation `path` with the function reading the same field;
    /// [`relation!`](crate::relation!) writes both from one field name.
    #[doc(hidden)]
    pub fn new<T>(path: impl Into<Path<M, T>>, read: fn(&M) -> &R) -> Self {
        let path = path.into();
        let field = model::field::<M>(&model::ModelPath::of(&path)).map(|f| (f.name, f.label));
        Self {
            includes: Includes::new().with(path),
            field,
            read,
        }
    }
}

impl<M, R> RelationLens<M, R> {
    /// The include that loads the relation.
    pub fn includes(&self) -> &Includes<M> {
        &self.includes
    }

    /// Read the relation field off `record`.
    pub fn read<'a>(&self, record: &'a M) -> &'a R {
        (self.read)(record)
    }

    /// The field's name and label, or why the path names no single field.
    pub(crate) fn field(&self) -> Result<(&str, &str), DeclarationErrorKind> {
        self.field
            .as_ref()
            .map(|(name, label)| (name.as_str(), label.as_str()))
            .map_err(Clone::clone)
    }
}

impl<M, R> Clone for RelationLens<M, R> {
    fn clone(&self) -> Self {
        Self {
            includes: self.includes.clone(),
            field: self.field.clone(),
            read: self.read,
        }
    }
}

impl<M, R> std::fmt::Debug for RelationLens<M, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RelationLens").field(&self.includes).finish()
    }
}

/// Build a [`RelationLens`] from a model and one of its relation fields.
///
/// `relation!(Post.author)` is the include `Post::fields().author()` paired with
/// `|post| &post.author`.
///
/// ```text
/// relation!(Post.author)          // RelationLens<Post, Deferred<Author>>
/// relation!(Post.comments)        // RelationLens<Post, Deferred<Vec<Comment>>>
/// relation!(crate::blog::Post.author)
/// ```
#[macro_export]
macro_rules! relation {
    ($($model:ident)::+ . $field:ident) => {
        $crate::RelationLens::new(
            <$($model)::+>::fields().$field(),
            |record: &$($model)::+| &record.$field,
        )
    };
}

/// The text a relation column renders for a row whose relation was not loaded.
const UNLOADED: &str = "(unloaded)";

/// What a relation column reads off a row, or `None` when the row's relation was not loaded.
type Loaded<M, T> = Arc<dyn Fn(&M) -> Option<T> + Send + Sync>;

/// A to-one relation field a [`RelationColumn`] reads: `Deferred<T>`, or `Deferred<Option<T>>`
/// for a nullable relation.
///
/// `Shape` tells the two impls apart; it is inferred.
pub trait ToOneRelation<Shape>: sealed::Sealed<Shape> {
    /// The related model.
    type Target;

    /// The related record, `Some(None)` for a nullable relation holding none, or `None` when the
    /// relation was not loaded.
    #[doc(hidden)]
    fn loaded(&self) -> Option<Option<&Self::Target>>;
}

/// The [`ToOneRelation`] shapes.
#[doc(hidden)]
pub mod shape {
    /// `Deferred<T>`.
    pub enum Required {}
    /// `Deferred<Option<T>>`.
    pub enum Nullable {}
}

mod sealed {
    pub trait Sealed<Shape> {}

    impl<T: toasty::schema::Model> Sealed<super::shape::Required> for toasty::Deferred<T> {}
    impl<T: toasty::schema::Model> Sealed<super::shape::Nullable> for toasty::Deferred<Option<T>> {}
}

impl<T: toasty::schema::Model> ToOneRelation<shape::Required> for Deferred<T> {
    type Target = T;

    fn loaded(&self) -> Option<Option<&T>> {
        (!self.is_unloaded()).then(|| Some(self.get()))
    }
}

impl<T: toasty::schema::Model> ToOneRelation<shape::Nullable> for Deferred<Option<T>> {
    type Target = T;

    fn loaded(&self) -> Option<Option<&T>> {
        (!self.is_unloaded()).then(|| self.get().as_ref())
    }
}

/// A column showing a `belongs_to` or `has_one` relation's record as text, which declares the
/// relation's include so every page that renders it loads the relation.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Author { #[key] #[auto] id: uuid::Uuid, name: String }
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     author_id: uuid::Uuid,
/// #     #[belongs_to(key = author_id, references = id)]
/// #     author: toasty::Deferred<Author>,
/// # }
/// use tablo_core::{RelationColumn, relation};
///
/// RelationColumn::new(relation!(Post.author), |a: &Author| a.name.clone());
/// ```
///
/// The header is the field's name, capitalized. A nullable relation holding no record renders
/// empty. The column neither searches nor sorts.
pub struct RelationColumn<M> {
    relation: Relation<M>,
    project: Loaded<M, String>,
}

impl<M> RelationColumn<M>
where
    M: toasty::schema::Model,
{
    /// Declare a column rendering `project` of the record `relation` loads.
    pub fn new<R, Shape>(
        relation: RelationLens<M, R>,
        project: impl Fn(&R::Target) -> String + Send + Sync + 'static,
    ) -> Self
    where
        R: ToOneRelation<Shape> + 'static,
        M: 'static,
    {
        let read = relation.clone();
        Self {
            relation: Relation::of(&relation, ColumnWidth::Wide),
            project: Arc::new(move |row| {
                read.read(row)
                    .loaded()
                    .map(|target| target.map(&project).unwrap_or_default())
            }),
        }
    }

    /// Replace the label the field's name gives it.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.relation.label = Some(label.into());
        self
    }

    /// Declare this column's width.
    pub fn width(mut self, width: ColumnWidth) -> Self {
        self.relation.width = width;
        self
    }
}

impl<M> RelationColumn<M> {
    /// The row's text, or `None` when its relation was not loaded.
    fn loaded_text(&self, row: &M) -> Option<String> {
        (self.project)(row)
    }
}

/// A column counting a `has_many` relation's records, which declares the relation's include so
/// every page that renders it loads the relation.
///
/// ```rust
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
/// use tablo_core::{CountColumn, relation};
///
/// CountColumn::new(relation!(Post.comments));
/// ```
///
/// The header is the field's name, capitalized. The count is the length of the loaded list, so a
/// page loads every related record of every row it counts.
pub struct CountColumn<M> {
    relation: Relation<M>,
    count: Loaded<M, usize>,
}

impl<M> CountColumn<M>
where
    M: toasty::schema::Model,
{
    /// Declare a column counting the records `relation` loads.
    pub fn new<T>(relation: RelationLens<M, Deferred<Vec<T>>>) -> Self
    where
        T: toasty::schema::Model + 'static,
        M: 'static,
    {
        let read = relation.clone();
        Self {
            relation: Relation::of(&relation, ColumnWidth::Narrow),
            count: Arc::new(move |row| {
                let records = read.read(row);
                (!records.is_unloaded()).then(|| records.get().len())
            }),
        }
    }

    /// Replace the label the field's name gives it.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.relation.label = Some(label.into());
        self
    }

    /// Declare this column's width.
    pub fn width(mut self, width: ColumnWidth) -> Self {
        self.relation.width = width;
        self
    }
}

impl<M> CountColumn<M> {
    /// The row's count, or `None` when its relation was not loaded.
    fn loaded_text(&self, row: &M) -> Option<String> {
        (self.count)(row).map(|count| count.to_string())
    }
}

/// What both relation columns declare: the field's name and label, its include and the width.
struct Relation<M> {
    name: String,
    /// The declared label, over the field's.
    label: Option<String>,
    field_label: String,
    misdeclared: Option<DeclarationErrorKind>,
    includes: Includes<M>,
    width: ColumnWidth,
}

impl<M> Relation<M> {
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
            width,
        }
    }

    fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.field_label)
    }

    /// The text of a row's loaded relation, or the unloaded marker when a loader skipped the
    /// include this column declares.
    fn text(&self, loaded: Option<String>) -> String {
        debug_assert!(
            loaded.is_some(),
            "the `{}` column declares its include, but its row was loaded without it",
            self.name
        );
        loaded.unwrap_or_else(|| UNLOADED.to_string())
    }
}

impl<M> Clone for Relation<M> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            label: self.label.clone(),
            field_label: self.field_label.clone(),
            misdeclared: self.misdeclared.clone(),
            includes: self.includes.clone(),
            width: self.width,
        }
    }
}

/// The [`Column`], `Clone` and `Debug` impls both relation columns share.
macro_rules! relation_column {
    ($ty:ident, $read:ident) => {
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

            fn text(&self, row: &M) -> String {
                self.relation.text(self.loaded_text(row))
            }

            fn column_width(&self) -> ColumnWidth {
                self.relation.width
            }

            fn includes(&self) -> Includes<M> {
                self.relation.includes.clone()
            }

            fn misdeclared(&self) -> Option<DeclarationErrorKind> {
                self.relation.misdeclared.clone()
            }
        }

        impl<M> Clone for $ty<M> {
            fn clone(&self) -> Self {
                Self {
                    relation: self.relation.clone(),
                    $read: Arc::clone(&self.$read),
                }
            }
        }

        impl<M> std::fmt::Debug for $ty<M> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($ty))
                    .field("name", &self.relation.name)
                    .field("label", &self.relation.label())
                    .field("width", &self.relation.width)
                    .finish_non_exhaustive()
            }
        }
    };
}

relation_column!(RelationColumn, project);
relation_column!(CountColumn, count);

#[cfg(test)]
mod tests;
