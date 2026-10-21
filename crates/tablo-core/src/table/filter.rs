//! Table filters: the [`Filter`] trait, the [`FilterValue`] it reads, the four built-in filters,
//! and the [`IntoFilters`] seam.

use std::{collections::BTreeMap, sync::Arc};

use toasty::stmt::Expr;
use topcoat::{context::Cx, view::*};

use crate::{
    BoxFuture,
    table::state::{MAX_FILTER_LEN, MAX_FILTER_VALUES},
};

/// One table filter declares a control and its predicate.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, age: i64 }
/// # use tablo_core::extend::{Filter, FilterInput, FilterValue};
/// # use toasty::stmt::Expr;
/// # use topcoat::{context::Cx, view::BoxView};
/// struct Adults;
///
/// impl Filter<User> for Adults {
///     fn name(&self) -> &str {
///         "adults"
///     }
///     fn label(&self) -> &str {
///         "Adults"
///     }
///     fn to_expr(&self, _cx: &Cx, value: &FilterValue) -> Option<Expr<bool>> {
///         (value.single() == Some("yes")).then(|| User::fields().age().ge(18))
///     }
///     fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
///         input.select(cx, vec![("yes".into(), "Adults only".into())])
///     }
/// }
/// ```
pub trait Filter<M>: Send + Sync {
    /// The filter's identifier, distinct within its table.
    fn name(&self) -> &str;

    /// The label the control renders beside it.
    fn label(&self) -> &str;

    /// The predicate `value` selects for the request `cx`, or `None` when it selects none. The
    /// table warns about a value that selects none unless [`is_noop_value`](Self::is_noop_value)
    /// accepts it.
    fn to_expr(&self, cx: &Cx, value: &FilterValue) -> Option<Expr<bool>>;

    /// Whether `value` selects no predicate without being invalid.
    fn is_noop_value(&self, _value: &FilterValue) -> bool {
        false
    }

    /// The control in the filter bar.
    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a>;

    /// The control in the filter bar once what it offers has loaded, such as rows a query
    /// returns. The filter bar renders this one; it defaults to [`control`](Self::control).
    fn load_control<'s, 'a: 's>(&'s self, cx: &'a Cx, input: FilterInput) -> ControlFuture<'s, 'a> {
        Box::pin(async move { self.control(cx, input) })
    }

    /// Whether the request may filter by `value`, asked before the table builds its predicate. A
    /// filter whose value names another resource's records admits only the ones the user may
    /// see; a refused value reads as invalid, as one that names no record does. An error is the
    /// database failing the check.
    #[doc(hidden)]
    fn admits<'s>(
        &'s self,
        _cx: &'s Cx,
        _value: &'s FilterValue,
    ) -> BoxFuture<'s, topcoat::Result<bool>> {
        Box::pin(async { Ok(true) })
    }

    /// The fields of the record [`to_expr`](Self::to_expr) filters by, or `None` when the filter
    /// does not say.
    ///
    /// A panel drops a filter reading a field [`field_access`](crate::ResourceDef::field_access)
    /// hides from the user, and refuses to mount a resource restricting a field whose filters
    /// answer `None`. The built-in filters answer their own field.
    fn reads(&self) -> Option<crate::lens::Reads<M>> {
        None
    }

    /// What is wrong with this filter's declaration.
    #[doc(hidden)]
    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        None
    }

    /// The option source's type when the request's panel cannot load from it, which a mount
    /// refuses. A filter offering no options answers `None`.
    #[doc(hidden)]
    fn unavailable_source(&self, _cx: &Cx) -> Option<&'static str> {
        None
    }

    /// The model's name when the filter's options show records under a label a panel cannot mask,
    /// over a model a mounted resource restricts a field of, which a mount refuses. The panel
    /// answers `is_resource` for a source's name and `restricted` for a model.
    #[doc(hidden)]
    fn unmasked_model(
        &self,
        _is_resource: &dyn Fn(&str) -> bool,
        _restricted: &dyn Fn(crate::toasty_compat::model::ModelId) -> bool,
    ) -> Option<&'static str> {
        None
    }
}

/// What a request carries for one filter: the values its `f.<name>` parameter repeats, in the
/// order the request carries them, and the parts its `f.<name>.<part>` parameters name.
///
/// A blank value is no value, and a value or part the filter holds already is not added again.
/// Nor is one past what the URL codec carries — 64 values and parts, each part and value at most
/// 256 bytes — which would spell a link the next request drops and flags.
///
/// ```
/// use tablo_core::{FilterValue, TableState};
///
/// let state = TableState::from_query("f.status=draft&f.status=review&f.created.from=2024-01-01");
/// assert_eq!(state.filters["status"].values(), ["draft", "review"]);
/// assert_eq!(state.filters["status"].single(), None);
/// assert_eq!(state.filters["created"].part("from"), Some("2024-01-01"));
/// assert_eq!(FilterValue::from("draft").single(), Some("draft"));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilterValue {
    values: Vec<String>,
    parts: BTreeMap<String, String>,
}

impl FilterValue {
    /// No value.
    pub fn new() -> Self {
        Self::default()
    }

    /// This value with `value`, trimmed, after the values it holds; one past the caps adds
    /// nothing.
    pub fn with(mut self, value: impl Into<String>) -> Self {
        self.push(value.into().trim());
        self
    }

    /// This value with `part` holding `value`, trimmed, unless it holds `part` already; one past
    /// the caps adds nothing.
    pub fn with_part(mut self, part: impl Into<String>, value: impl Into<String>) -> Self {
        self.insert_part(&part.into(), value.into().trim());
        self
    }

    /// Whether it holds no value and no part.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.parts.is_empty()
    }

    /// The values, in the order the request carries them.
    pub fn values(&self) -> &[String] {
        &self.values
    }

    /// The one value a single-choice filter reads: `None` unless it holds exactly one value and no
    /// part.
    pub fn single(&self) -> Option<&str> {
        match self.values.as_slice() {
            [value] if self.parts.is_empty() => Some(value),
            _ => None,
        }
    }

    /// The value `part` holds.
    pub fn part(&self, part: &str) -> Option<&str> {
        self.parts.get(part).map(String::as_str)
    }

    /// The parts, by name.
    pub fn parts(&self) -> impl Iterator<Item = (&str, &str)> {
        self.parts
            .iter()
            .map(|(part, value)| (part.as_str(), value.as_str()))
    }

    /// How many values and parts it holds.
    pub(crate) fn len(&self) -> usize {
        self.values.len() + self.parts.len()
    }

    /// Whether adding `value`, under `part` when given, would change nothing.
    pub(crate) fn holds(&self, part: Option<&str>, value: &str) -> bool {
        match part {
            Some(part) => self.parts.contains_key(part),
            None => self.values.iter().any(|held| held == value),
        }
    }

    /// Whether `value` fits what the URL codec carries: it holds text, at most
    /// [`MAX_FILTER_LEN`] bytes, beside fewer than [`MAX_FILTER_VALUES`] values and parts. Past
    /// that the parse drops the occurrence, so what this rejects would only encode a link the
    /// next request flags as dropped.
    fn fits(&self, value: &str) -> bool {
        !value.is_empty() && value.len() <= MAX_FILTER_LEN && self.len() < MAX_FILTER_VALUES
    }

    /// Add `value`, already trimmed, after the values.
    pub(crate) fn push(&mut self, value: &str) {
        if self.fits(value) && !self.holds(None, value) {
            self.values.push(value.to_string());
        }
    }

    /// Set `part` to `value`, already trimmed, unless it holds `part` already.
    pub(crate) fn insert_part(&mut self, part: &str, value: &str) {
        if !part.is_empty() && part.len() <= MAX_FILTER_LEN && self.fits(value) {
            self.parts
                .entry(part.to_string())
                .or_insert_with(|| value.to_string());
        }
    }
}

impl From<&str> for FilterValue {
    fn from(value: &str) -> Self {
        Self::new().with(value)
    }
}

impl From<String> for FilterValue {
    fn from(value: String) -> Self {
        Self::new().with(value)
    }
}

/// What [`Filter::load_control`] returns: the control, once what it offers has loaded.
pub type ControlFuture<'s, 'a> = BoxFuture<'s, BoxView<'a>>;

/// Generate a built-in filter's `Clone` and its metadata-only `Debug`.
macro_rules! filter_impls {
    (
        $ty:ident, $this:ident {
            $( $field:ident: $value:expr ),* $(,)?
        }
        clone { $( $clone:ident ),* $(,)? }
    ) => {
        impl<M> Clone for $ty<M> {
            fn clone(&self) -> Self {
                Self {
                    $( $clone: self.$clone.clone(), )*
                }
            }
        }

        impl<M> std::fmt::Debug for $ty<M> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let $this = self;
                f.debug_struct(stringify!($ty))
                    $( .field(stringify!($field), $value) )*
                    .finish()
            }
        }
    };
}

mod date;
mod input;
mod query;
mod select;
mod ternary;

pub use date::{DateFilter, FilterDate};
pub use input::FilterInput;
pub use query::QueryFilter;
pub use select::{FilterKey, SelectFilter};
pub use ternary::TernaryFilter;

/// A table's filters, as the table stores them.
pub(crate) type BoxFilter<M> = Arc<dyn Filter<M>>;

/// Convert a single built-in filter, or a tuple of any [`Filter`]s, into a table's filter list.
pub trait IntoFilters<M> {
    #[doc(hidden)]
    fn into_filters(self) -> Vec<BoxFilter<M>>;
}

/// The single-filter impls of [`IntoFilters`], one per built-in.
macro_rules! into_filters_single {
    ($($ty:ident),+) => {
        $(
            impl<M> IntoFilters<M> for $ty<M>
            where
                M: toasty::schema::Model + Send + Sync + 'static,
            {
                fn into_filters(self) -> Vec<BoxFilter<M>> {
                    vec![Arc::new(self)]
                }
            }
        )+
    };
}

into_filters_single!(SelectFilter, TernaryFilter, DateFilter, QueryFilter);

/// Generate the tuple impls of [`IntoFilters`] from one list per arity.
macro_rules! into_filters_tuples {
    ($($T:ident => $v:ident),+ $(,)?) => {
        impl<M, $($T),+> IntoFilters<M> for ($($T,)+)
        where
            $($T: Filter<M> + 'static,)+
        {
            fn into_filters(self) -> Vec<BoxFilter<M>> {
                let ($($v,)+) = self;
                vec![$(Arc::new($v) as BoxFilter<M>,)+]
            }
        }
    };
}

into_filters_tuples!(A => a);
into_filters_tuples!(A => a, B => b);
into_filters_tuples!(A => a, B => b, C => c);
into_filters_tuples!(A => a, B => b, C => c, D => d);
into_filters_tuples!(A => a, B => b, C => c, D => d, E => e);
into_filters_tuples!(A => a, B => b, C => c, D => d, E => e, F => f);
into_filters_tuples!(A => a, B => b, C => c, D => d, E => e, F => f, G => g);
into_filters_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h
);
into_filters_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i
);
into_filters_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i, J => j
);
into_filters_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i, J => j, K => k
);
into_filters_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i, J => j, K => k, L => l
);

#[cfg(test)]
mod tests;
