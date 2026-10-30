//! Soft deletes: the column [`ResourceDef::soft_delete`](super::ResourceDef::soft_delete) names,
//! the [`Trashed`] scope every loader reads through, and the list's trash control.

use toasty::stmt::{Expr, Path};
use topcoat::{context::Cx, view::BoxView};

use crate::{
    DeclarationErrorKind, Lens,
    table::filter::{Filter, FilterInput, FilterValue},
    toasty_compat::model::{self, ModelPath},
};

/// Which records a query over a soft-deleting resource admits: the live ones, the trashed ones,
/// or both.
///
/// On a resource that declares no [`soft_delete`](super::ResourceDef::soft_delete) column, every
/// record is live, so `Only` admits none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Trashed {
    /// The live records only: what every list, form and record loader reads by default.
    #[default]
    Without,
    /// Live and trashed records alike.
    With,
    /// The trashed records only.
    Only,
}

impl Trashed {
    /// Whether this scope admits a record that is, or is not, `trashed`.
    pub fn admits(self, trashed: bool) -> bool {
        match self {
            Self::Without => !trashed,
            Self::With => true,
            Self::Only => trashed,
        }
    }

    /// The scope the list's trash control selects with `value`, or `None` for a value it does not
    /// offer.
    pub(crate) fn from_filter(value: Option<&FilterValue>) -> Option<Self> {
        match value.map(FilterValue::single) {
            None => Some(Self::Without),
            Some(Some(WITH)) => Some(Self::With),
            Some(Some(ONLY)) => Some(Self::Only),
            Some(_) => None,
        }
    }
}

impl<R: super::Resource> super::ResourceDef<R> {
    /// Deletes by stamping `lens`'s column with the time instead of removing the row, as in
    /// `.soft_delete(lens!(Post.deleted_at))`, and keeps the stamped records out of every loader
    /// unless it asks for them.
    ///
    /// The list hides the trashed records behind a trash control, and offers Restore, which
    /// empties the column, and Force delete, which removes the row, on each one: the policy decides
    /// who may through [`Ability::Restore`](crate::Ability::Restore) and
    /// [`Ability::ForceDelete`](crate::Ability::ForceDelete). The detail page opens a
    /// trashed record; its edit page and every other action refuse one until it is restored.
    /// [`scoped_query`](crate::scoped_query) admits the live records only, and
    /// [`scoped_query_in`](crate::scoped_query_in) the ones a [`Trashed`](crate::Trashed) scope
    /// names, each under the tenant filter.
    ///
    /// The unique pre-check probes the trashed records too, since a unique index covers them.
    /// Mounting the panel refuses a lens that is not one field of the model, and a record form
    /// that writes the column.
    #[must_use]
    pub fn soft_delete(mut self, lens: Lens<R::Model, Option<jiff::Timestamp>>) -> Self {
        self.soft_delete = Some(SoftDelete::new(lens));
        self
    }
}

/// The column a resource soft-deletes through, as its def declares it.
pub(crate) struct SoftDelete<M> {
    path: Path<M, Option<jiff::Timestamp>>,
    /// Whether a loaded record's column is set.
    read: fn(&M) -> &Option<jiff::Timestamp>,
    /// The column, or why the lens names none.
    field: Result<(usize, String), DeclarationErrorKind>,
}

impl<M: toasty::schema::Model + Send + Sync + 'static> SoftDelete<M> {
    pub(crate) fn new(lens: Lens<M, Option<jiff::Timestamp>>) -> Self {
        let field =
            model::field::<M>(&ModelPath::of(lens.path())).map(|field| (field.index, field.name));
        Self {
            path: lens.path().clone(),
            read: lens.reader(),
            field,
        }
    }
}

impl<M> SoftDelete<M> {
    /// The predicate `trashed` adds to a query, or `None` when it admits every record.
    pub(crate) fn filter(&self, trashed: Trashed) -> Option<Expr<bool>> {
        match trashed {
            Trashed::Without => Some(self.path.clone().is_none()),
            Trashed::With => None,
            Trashed::Only => Some(self.path.clone().is_some()),
        }
    }

    /// Whether `record` is trashed.
    pub(crate) fn is_trashed(&self, record: &M) -> bool {
        (self.read)(record).is_some()
    }

    /// The column's index in the model, or why the lens names no column.
    pub(crate) fn field(&self) -> Result<&(usize, String), &DeclarationErrorKind> {
        self.field.as_ref()
    }
}

/// The trash control's filter name and URL values.
pub(crate) const TRASH_FILTER: &str = "trashed";
const WITH: &str = "with";
const ONLY: &str = "only";

/// The list's trash control: a select between the live records, every record and the trashed
/// ones. It adds no predicate itself: the list's base query reads its value through
/// [`Trashed::from_filter`], since the default it replaces is a predicate no filter could drop.
pub(crate) struct TrashFilter;

impl<M> Filter<M> for TrashFilter {
    fn name(&self) -> &str {
        TRASH_FILTER
    }

    fn label(&self) -> crate::Text {
        crate::Text::message("tablo-trash")
    }

    /// Nothing a row shows: which records are trashed is what Restore and Delete permanently
    /// offer anyway.
    fn reads(&self) -> Option<crate::lens::Reads<M>> {
        Some(crate::lens::Reads::nothing())
    }

    fn to_expr(&self, _cx: &Cx, _value: &FilterValue) -> Option<Expr<bool>> {
        None
    }

    fn is_noop_value(&self, value: &FilterValue) -> bool {
        Trashed::from_filter(Some(value)).is_some()
    }

    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        input.select_led_by(
            cx,
            &crate::t!(cx, "tablo-trash-without"),
            vec![
                (WITH.to_string(), crate::t!(cx, "tablo-trash-with")),
                (ONLY.to_string(), crate::t!(cx, "tablo-trash-only")),
            ],
        )
    }
}
