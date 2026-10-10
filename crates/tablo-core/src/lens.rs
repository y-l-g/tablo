//! [`Lens`]: a model field's typed path paired with the reader of its value, built by [`lens!`].

use derive_where::derive_where;
use toasty::stmt::Path;

/// A model field's typed path, which queries filter and sort on, paired with the function that
/// reads the field off a loaded record.
///
/// [`lens!`](crate::lens!) builds one from a single field name, so the two halves cannot name
/// different fields:
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String }
/// # use tablo_core::{TextColumn, lens};
/// TextColumn::new(lens!(User.name)).searchable();
/// ```
///
/// The filters, the [`Field`](crate::Field) constructors and [`Tenancy`](crate::Tenancy) take a
/// `Lens` or a plain path; a column needs a `Lens`, since it renders the value it sorts on.
///
/// `P` is the type the path names, the field's own type except for a list: Toasty names a
/// `Vec<T>` field's path `List<T>`.
#[derive_where(Clone, Debug)]
pub struct Lens<M, T, P = T> {
    path: Path<M, P>,
    #[derive_where(skip(Debug))]
    read: fn(&M) -> &T,
}

impl<M, T, P> Lens<M, T, P> {
    /// Pair `path` with the function reading the same field; [`lens!`](crate::lens!) writes both
    /// from one field name.
    #[doc(hidden)]
    pub fn new(path: impl Into<Path<M, P>>, read: fn(&M) -> &T) -> Self {
        Self {
            path: path.into(),
            read,
        }
    }

    /// The field's query path.
    pub fn path(&self) -> &Path<M, P> {
        &self.path
    }

    /// Read the field off `record`.
    pub fn read<'a>(&self, record: &'a M) -> &'a T {
        (self.read)(record)
    }
}

impl<M, T, P> From<Lens<M, T, P>> for Path<M, P> {
    fn from(lens: Lens<M, T, P>) -> Self {
        lens.path
    }
}

/// Build a [`Lens`] from a model and a field, or a chain of embedded fields.
///
/// `lens!(User.name)` is the path `User::fields().name()` paired with `|user| &user.name`. A
/// chain through a relation does not compile: a relation's records are not part of the row.
///
/// ```rust
/// # use tablo_core::{Lens, lens};
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String }
/// # #[derive(Debug, Clone, toasty::Embed)]
/// # struct Seo { title: String }
/// # mod blog {
/// #     #[derive(Debug, Clone, toasty::Model)]
/// #     pub struct Post { #[key] #[auto] pub id: uuid::Uuid, pub title: String, pub seo: super::Seo }
/// # }
/// # use blog::Post;
/// # fn main() {
/// let name: Lens<User, String> = lens!(User.name);
/// let seo: Lens<Post, Seo> = lens!(Post.seo); // an embedded value, whole
/// let title: Lens<Post, String> = lens!(Post.seo.title); // an embedded struct's field
/// let qualified = lens!(crate::blog::Post.title);
/// # let _ = (name, seo, title, qualified);
/// # }
/// ```
#[macro_export]
macro_rules! lens {
    ($($model:ident)::+ . $($field:ident).+) => {
        $crate::Lens::new(
            <$($model)::+>::fields()$(.$field())+,
            |record: &$($model)::+| &record$(.$field)+,
        )
    };
}
