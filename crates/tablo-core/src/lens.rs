//! [`Lens`]: a model field's typed path paired with the reader of its value, built by [`lens!`].

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
pub struct Lens<M, T> {
    path: Path<M, T>,
    read: fn(&M) -> &T,
}

impl<M, T> Lens<M, T> {
    /// Pair `path` with the function reading the same field; [`lens!`](crate::lens!) writes both
    /// from one field name.
    #[doc(hidden)]
    pub fn new(path: Path<M, T>, read: fn(&M) -> &T) -> Self {
        Self { path, read }
    }

    /// The field's query path.
    pub fn path(&self) -> &Path<M, T> {
        &self.path
    }

    /// Read the field off `record`.
    pub fn read<'a>(&self, record: &'a M) -> &'a T {
        (self.read)(record)
    }
}

impl<M, T> Clone for Lens<M, T> {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            read: self.read,
        }
    }
}

impl<M, T> std::fmt::Debug for Lens<M, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Lens").field(&self.path).finish()
    }
}

impl<M, T> From<Lens<M, T>> for Path<M, T> {
    fn from(lens: Lens<M, T>) -> Self {
        lens.path
    }
}

/// Build a [`Lens`] from a model and a field, or a chain of embedded fields.
///
/// `lens!(User.name)` is the path `User::fields().name()` paired with `|user| &user.name`. A
/// chain through a relation does not compile: a relation's records are not part of the row.
///
/// ```text
/// lens!(User.name)          // Lens<User, String>
/// lens!(Post.seo.title)     // an embedded struct's field
/// lens!(crate::blog::Post.title)
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

#[cfg(test)]
mod tests;
