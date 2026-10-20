//! [`Lens`]: a model field's typed path paired with the reader of its value, built by [`lens!`];
//! [`Reads`]: the fields a declaration reads off a record.

use derive_where::derive_where;
use toasty::stmt::Path;

/// A model field's typed path, which queries filter and sort on, paired with the function that
/// reads the field off a loaded record.
///
/// [`lens!`](crate::lens!) builds one from a single field name, so the two halves cannot name
/// different fields:
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
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

    /// The function reading the field.
    pub(crate) fn reader(&self) -> fn(&M) -> &T {
        self.read
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
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String }
/// # #[derive(Debug, Clone, toasty::Embed)]
/// # struct Seo { title: String }
/// # mod blog {
/// #     #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// #     pub struct Post { #[key] #[auto] pub id: uuid::Uuid, pub title: String, pub seo: super::Seo }
/// # }
/// # use blog::Post;
/// # fn main() {
/// let name: Lens<User, String> = lens!(User.name);
/// let seo: Lens<Post, Seo> = lens!(Post.seo); // an embedded value, whole
/// let title: Lens<Post, String> = lens!(Post.seo.title); // an embedded struct's field
/// let qualified = lens!(blog::Post.title);
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

/// The fields of `M` a [`Column`](crate::extend::Column) or a [`Filter`](crate::extend::Filter)
/// reads, by the root field each path starts at: what
/// [`field_access`](crate::ResourceDef::field_access) hides it by.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Employee { #[key] #[auto] id: uuid::Uuid, salary: i64, bonus: i64 }
/// use tablo_core::{ComputedColumn, Reads, lens};
///
/// ComputedColumn::new("Pay", |e: &Employee| (e.salary + e.bonus).to_string())
///     .reading(Reads::of(lens!(Employee.salary)).and(lens!(Employee.bonus)));
/// ```
#[derive_where(Clone, Debug, Default)]
pub struct Reads<M> {
    fields: Vec<String>,
    /// Whether some path named no root field: such a read hides under any mask.
    unresolved: bool,
    #[derive_where(skip(Debug))]
    _model: std::marker::PhantomData<fn() -> M>,
}

impl<M> Reads<M> {
    /// No field: a column or a filter that reads nothing off the row.
    pub fn nothing() -> Self {
        Self::default()
    }

    /// The field `path` reads.
    pub fn of<T>(path: impl Into<Path<M, T>>) -> Self
    where
        M: toasty::schema::Model,
    {
        Self::nothing().and(path)
    }

    /// These fields and the one `path` reads.
    pub fn and<T>(self, path: impl Into<Path<M, T>>) -> Self
    where
        M: toasty::schema::Model,
    {
        let path = crate::toasty_compat::model::ModelPath::of(&path.into());
        match crate::toasty_compat::model::root_field::<M>(&path) {
            Some(field) => self.named(field),
            None => Self {
                unresolved: true,
                ..self
            },
        }
    }

    /// These fields and the root field `name`.
    pub(crate) fn named(mut self, name: impl Into<String>) -> Self {
        let name = name.into();
        if !self.fields.contains(&name) {
            self.fields.push(name);
        }
        self
    }

    /// These fields and the ones `other` reads.
    pub(crate) fn union(&self, other: &Self) -> Self {
        let mut union = other
            .fields
            .iter()
            .fold(self.clone(), |reads, field| reads.named(field.clone()));
        union.unresolved |= other.unresolved;
        union
    }

    /// Whether it reads any of `fields`; a read naming no field may read them.
    pub(crate) fn reads_any(&self, fields: &[String]) -> bool {
        (self.unresolved && !fields.is_empty())
            || self.fields.iter().any(|field| fields.contains(field))
    }
}

/// Whether a declaration answering `reads` shows to a request that may not view `hidden`. One that
/// does not say shows: a mount refuses it on a resource restricting a field.
pub(crate) fn shows<M>(reads: Option<Reads<M>>, hidden: &[String]) -> bool {
    reads.is_none_or(|reads| !reads.reads_any(hidden))
}
