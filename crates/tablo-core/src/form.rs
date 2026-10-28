//! Record forms: the typed value a form submission parses into, and the write
//! that stores it.
//!
//! Declares one `RecordForm` struct with one field per written model column.
//! The struct is the form's one source of truth: it parses every key, and a
//! field's blank answer decides whether its control is required. Completes
//! unposted keys from the stored record and writes only named fields plus model
//! defaults.
//!
//! ```no_run
//! #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
//! struct User {
//!     #[key]
//!     #[auto]
//!     id: uuid::Uuid,
//!     name: String,
//!     age: i64,
//! }
//!
//! #[derive(tablo_core::RecordForm)]
//! #[form(model = User)]
//! struct UserForm {
//!     name: String,
//!     #[form(blank = 0)]
//!     age: i64,
//! }
//!
//! // A column type toasty stores but the form edge cannot spell.
//! #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
//! struct Tagged {
//!     #[key]
//!     #[auto]
//!     id: uuid::Uuid,
//!     tags: Vec<String>,
//! }
//! ```
//!
//! # Compile-time refusals
//!
//! The derive refuses, at compile time:
//!
//! - a field the model does not have;
//! - a type the model's field does not have, such as `i32` for an `i64` column;
//! - a bare choice, which would offer nothing: a foreign key names its source with
//!   `#[form(relationship = ..)]`;
//! - `relationship` and `file` on one field, which each pick the control;
//! - a scalar type the form edge cannot spell, such as `Vec<String>`;
//! - a many-to-many field whose keys are not its source's primary key;
//! - `optional` on a type with no empty value;
//! - `embed` on a type that is not an [`EmbeddedForm`](crate::EmbeddedForm).

use std::{collections::HashSet, fmt::Debug, hash::Hash};

use toasty::{Executor, schema::Model};
use topcoat::context::Cx;

#[doc(hidden)]
pub use crate::schema::input::required_input;
use crate::{
    column::Includes,
    schema::{OptionSource, Schema},
};
// A record form's values, scalars and errors sit below `schema`, which reads and renders them,
// and a form input is a schema's parse; both are re-exported here, where a record form names
// them.
pub use crate::{
    schema::input::FormInput,
    values::{
        FieldError, FieldErrors, FormField, FormScalar, FormValue, FormValues, NullableScalar,
        assert_form_scalar, parse_list, parse_password, parse_scalar,
    },
};

/// The hidden control the edit form carries the record's [`ModelColumns::version`] in.
///
/// [`ModelColumns::version`]: crate::ModelColumns::version
pub(crate) const VERSION_FIELD: &str = "record_version";

/// The name of the create form's second submit button, which saves and lands on a fresh create
/// form rather than the list.
pub(crate) const ANOTHER_FIELD: &str = "create_another";

/// The form key of the many-to-many field `path`, a `#[has_many(via = ..)]` list of `R`'s
/// records, holding their keys of type `K`: the derive calls it, so a field whose type is not
/// `Vec<K>` of the related model's key, or whose source is not that model's, fails to compile.
#[doc(hidden)]
pub fn links_key<M, R, K>(
    path: impl Into<toasty::stmt::Path<M, toasty::stmt::List<R::Model>>>,
) -> String
where
    M: Model,
    R: OptionSource,
    R::Model: Model<PrimaryKey = K>,
    K: FormScalar,
{
    crate::schema::form_key(path.into())
}

/// `includes` and the include loading the many-to-many field `path`, which
/// [`RecordForm::hydrate`] reads.
#[doc(hidden)]
pub fn links_include<M, R>(
    includes: Includes<M>,
    path: impl Into<toasty::stmt::Path<M, toasty::stmt::List<R::Model>>>,
) -> Includes<M>
where
    M: Model,
    R: OptionSource,
{
    includes.with(path.into())
}

/// The keys of the records a many-to-many field holds, as its form key carries them, or `None`
/// when the record was loaded without them.
#[doc(hidden)]
pub fn write_links<T>(records: &toasty::Deferred<Vec<T>>) -> Option<FormValue>
where
    T: Model + toasty::stmt::IntoExpr<T>,
{
    (!records.is_unloaded()).then(|| {
        FormValue::List(
            records
                .get()
                .iter()
                .map(crate::toasty_compat::pk::pk_text)
                .collect(),
        )
    })
}

/// The typed value a resource's form submission parses into.
///
/// Derive it from a model with [`tablo_core::ModelForm`](crate::ModelForm) or from a struct with
/// [`tablo_core::RecordForm`](crate::RecordForm); a hand-written impl is what the derives expand
/// to.
pub trait RecordForm: Sized + Send + 'static {
    /// The model the form writes.
    type Model: Model + toasty::stmt::IntoExpr<Self::Model> + Send + Sync + 'static;

    /// One variant per form field: the key [`Posted`] uses. Its bound form keys
    /// are [`Self::fields`]'.
    type Field: Copy + Eq + Hash + Debug + Send + Sync + 'static;

    /// Every field, in declaration order, with the keys it binds.
    fn fields() -> Vec<FormField<Self::Field>>;

    /// The control the form renders `field` with when its [`form`](crate::ResourceDef::form)
    /// does not place it: what `controls()` hands over for the field, as a one-control schema.
    ///
    /// A resource's form renders every field it does not place after the ones it does, in
    /// declaration order, so a resource that declares no form renders one control per field.
    fn control(field: Self::Field) -> Schema<Self>;

    /// The relations [`hydrate`](Self::hydrate) reads off the record: each many-to-many field's,
    /// which the edit page and the edit's write load the record with.
    fn includes() -> Includes<Self::Model> {
        Includes::new()
    }

    /// The stored record as the form spells it.
    fn hydrate(cx: &Cx, record: &Self::Model) -> FormValues;

    /// The records each many-to-many field holds, as their keys' form spelling: what
    /// [`write_create`](crate::resource::write_create) links a new record to, and what
    /// [`write_update`](crate::resource::write_update) leaves a posted field's record linked to.
    fn links(&self) -> Vec<(Self::Field, Vec<String>)> {
        Vec::new()
    }

    /// Parse a completed submission: the form's only presence and type check.
    ///
    /// # Errors
    ///
    /// Every key that failed, each once, under one of [`Self::fields`]' keys.
    fn parse(cx: &Cx, values: &FormValues) -> std::result::Result<Self, Vec<FieldError>>;

    /// The create builder with every field set.
    fn into_create(self) -> <Self::Model as Model>::Create;

    /// The instance update builder with one assignment per named field, or
    /// `None` when no field is named.
    ///
    /// `None` is what keeps a submission that names no field from building an
    /// empty update, which toasty refuses with an assertion.
    fn into_update<'a>(
        self,
        record: &'a mut Self::Model,
        named: &HashSet<Self::Field>,
    ) -> Option<<Self::Model as Model>::Update<'a>>;

    /// Execute a builder [`Self::into_update`] returned.
    ///
    /// Toasty's instance update builder implements no trait that carries
    /// `exec`, so the generic write reaches it through the form.
    fn exec_update<'a>(
        update: <Self::Model as Model>::Update<'a>,
        ex: &'a mut dyn Executor,
    ) -> impl Future<Output = toasty::Result<()>> + Send + 'a;

    /// Whether the resource naming this form has create and edit pages.
    ///
    /// [`Panel::resource`](crate::Panel::resource) registers the create, edit,
    /// and options routes only when this holds. [`NoForm`] sets it to `false`.
    const HAS_FORM: bool = true;
}

/// The record form of a resource with no create or edit page.
///
/// A list-only resource names it as [`Resource::Form`](crate::Resource::Form):
/// `type Form = NoForm<Self::Model>;`. [`fields`](RecordForm::fields) and
/// [`hydrate`](RecordForm::hydrate) are empty and
/// [`into_update`](RecordForm::into_update) answers `None`. No route parses or
/// writes it, so [`parse`](RecordForm::parse) and
/// [`into_create`](RecordForm::into_create) panic naming the model, and the
/// future [`exec_update`](RecordForm::exec_update) returns panics when polled.
pub struct NoForm<M>(std::marker::PhantomData<fn() -> M>);

impl<M: Model + toasty::stmt::IntoExpr<M> + Send + Sync + 'static> NoForm<M> {
    fn unreachable() -> ! {
        panic!("`NoForm<{}>` has no form", std::any::type_name::<M>())
    }
}

impl<M: Model + toasty::stmt::IntoExpr<M> + Send + Sync + 'static> RecordForm for NoForm<M> {
    type Model = M;
    type Field = std::convert::Infallible;

    const HAS_FORM: bool = false;

    fn fields() -> Vec<FormField<Self::Field>> {
        Vec::new()
    }

    fn control(field: Self::Field) -> Schema<Self> {
        match field {}
    }

    fn hydrate(_cx: &Cx, _record: &M) -> FormValues {
        FormValues::new()
    }

    fn parse(_cx: &Cx, _values: &FormValues) -> std::result::Result<Self, Vec<FieldError>> {
        Self::unreachable()
    }

    fn into_create(self) -> M::Create {
        Self::unreachable()
    }

    fn into_update<'a>(
        self,
        _record: &'a mut M,
        _named: &HashSet<Self::Field>,
    ) -> Option<M::Update<'a>> {
        None
    }

    fn exec_update<'a>(
        update: M::Update<'a>,
        ex: &'a mut dyn Executor,
    ) -> impl Future<Output = toasty::Result<()>> + Send + 'a {
        // `into_update` answers `None`, so no builder reaches here. The
        // builder is not known to be `Send`, so the future must not hold it.
        drop((update, ex));
        async { Self::unreachable() }
    }
}

/// An edit submission: the parsed form, and the fields the submission named.
///
/// Derefs to the form, so a record fn reads `posted.author_id` — the posted
/// value, or the stored one when the submission did not name the field.
pub struct Posted<F: RecordForm> {
    form: F,
    named: HashSet<F::Field>,
}

impl<F: RecordForm> Posted<F> {
    /// A posted form that names `named`.
    pub fn new(form: F, named: impl IntoIterator<Item = F::Field>) -> Self {
        Self {
            form,
            named: named.into_iter().collect(),
        }
    }

    /// Whether the submission named `field`.
    pub fn named(&self, field: F::Field) -> bool {
        self.named.contains(&field)
    }

    /// The update builder assigning every named field, or `None` when the
    /// submission named none. See [`RecordForm::into_update`].
    pub fn into_update(self, record: &mut F::Model) -> Option<<F::Model as Model>::Update<'_>> {
        self.form.into_update(record, &self.named)
    }
}

impl<F: RecordForm> std::ops::Deref for Posted<F> {
    type Target = F;

    fn deref(&self) -> &F {
        &self.form
    }
}
