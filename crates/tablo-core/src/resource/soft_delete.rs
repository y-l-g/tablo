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

    fn label(&self) -> &str {
        "Trashed"
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
            "Without trashed",
            vec![
                (WITH.to_string(), "With trashed".to_string()),
                (ONLY.to_string(), "Only trashed".to_string()),
            ],
        )
    }
}
