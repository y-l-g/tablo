//! [`RelationLens`] and [`relation!`](crate::relation!): a relation field named once, paired with
//! its include and its accessors.

use derive_where::derive_where;
use toasty::stmt::Path;

use crate::{DeclarationErrorKind, column::Includes, toasty_compat::model};

/// A relation field's include, which loads it, paired with the function that reads the loaded
/// relation off a record.
///
/// [`relation!`](crate::relation!) builds one from a single field name, so the column that reads
/// the relation always declares its include:
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
/// # use tablo_core::{RelationColumn, relation};
/// RelationColumn::unscoped(relation!(Post.author), |a: &Author| a.name.clone());
/// ```
#[derive_where(Clone, Debug)]
pub struct RelationLens<M, R> {
    includes: Includes<M>,
    /// The relation field's name, or why the path names no single field.
    field: Result<(String, String), DeclarationErrorKind>,
    /// The relation field, and the foreign keys of a `belongs_to`: what showing it reads.
    reads: Vec<String>,
    #[derive_where(skip(Debug))]
    pub(super) read: fn(&M) -> &R,
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
        let info = model::field::<M>(&model::ModelPath::of(&path));
        let reads = match &info {
            Ok(field) => std::iter::once(field.name.clone())
                .chain(
                    model::belongs_to::<M>(field.index)
                        .map(|relation| relation.foreign_keys)
                        .unwrap_or_default(),
                )
                .collect(),
            Err(_) => Vec::new(),
        };
        Self {
            includes: Includes::new().with(path),
            field: info.map(|f| (f.name, f.label)),
            reads,
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

    /// The fields showing the relation reads.
    pub(crate) fn reads(&self) -> crate::lens::Reads<M> {
        self.reads
            .iter()
            .fold(crate::lens::Reads::nothing(), |reads, field| {
                reads.named(field.clone())
            })
    }

    /// The field's name and label, or why the path names no single field.
    pub(crate) fn field(&self) -> Result<(&str, &str), DeclarationErrorKind> {
        self.field
            .as_ref()
            .map(|(name, label)| (name.as_str(), label.as_str()))
            .map_err(Clone::clone)
    }
}

/// Build a [`RelationLens`] from a model and one of its relation fields.
///
/// `relation!(Post.author)` is the include `Post::fields().author()` paired with
/// `|post| &post.author`.
///
/// ```rust
/// # use tablo_core::{RelationLens, relation};
/// # use toasty::Deferred;
/// # mod blog {
/// #     #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// #     pub struct Author { #[key] #[auto] pub id: uuid::Uuid, pub name: String }
/// #     #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// #     pub struct Post {
/// #         #[key] #[auto] pub id: uuid::Uuid,
/// #         pub author_id: uuid::Uuid,
/// #         #[belongs_to(key = author_id, references = id)]
/// #         pub author: toasty::Deferred<Author>,
/// #         #[has_many]
/// #         pub comments: toasty::Deferred<Vec<Comment>>,
/// #     }
/// #     #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// #     pub struct Comment {
/// #         #[key] #[auto] pub id: uuid::Uuid,
/// #         #[index]
/// #         pub post_id: uuid::Uuid,
/// #         #[belongs_to(key = post_id, references = id)]
/// #         pub post: toasty::Deferred<Post>,
/// #     }
/// # }
/// # use blog::{Author, Comment, Post};
/// # fn main() {
/// let author: RelationLens<Post, Deferred<Author>> = relation!(Post.author);
/// let comments: RelationLens<Post, Deferred<Vec<Comment>>> = relation!(Post.comments);
/// let qualified = relation!(blog::Post.author);
/// # let _ = (author, comments, qualified);
/// # }
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
