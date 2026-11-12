//! [`NumberFilter`]: a number field matched exactly, or within a range whose ends are optional.

use std::{marker::PhantomData, sync::Arc};

use toasty::stmt::{Expr, IntoExpr, Path};
use topcoat::{context::Cx, view::*};

use super::{Filter, FilterInput, FilterValue};
use crate::{schema::Binding, values::TypedValue};

/// A field type a [`NumberFilter`] compares: the integer types up to 64 bits, `f32` and `f64`, and
/// an `Option` of one.
///
/// The decimal types are left out: SQLite, MySQL and Turso store them as text, which compares
/// `10.00` below `9.50`.
pub trait FilterNumber: Sized + Send + Sync + 'static {
    /// The value the field compares against.
    type Value: IntoExpr<Self> + PartialOrd;

    /// A submitted bound, or `None` when it is not a number of the type.
    fn parse(value: &str) -> Option<Self::Value>;
}

/// Binds the number types a [`NumberFilter`] compares, through their form parsing.
macro_rules! filter_number {
    ($($ty:ty),* $(,)?) => {
        $(
            impl FilterNumber for $ty {
                type Value = Self;

                fn parse(value: &str) -> Option<Self> {
                    <$ty as TypedValue>::parse_input(value)
                }
            }
        )*
    };
}

filter_number!(i8, i16, i32, i64, isize, u8, u16, u32, f32, f64);

/// Binds the unsigned types wider than an `i64`, refusing a bound above `i64::MAX`: SQLite wraps
/// one into a negative and PostgreSQL refuses it, so it could match every row or fail the page.
macro_rules! filter_wide_unsigned {
    ($($ty:ty),* $(,)?) => {
        $(
            impl FilterNumber for $ty {
                type Value = Self;

                fn parse(value: &str) -> Option<Self> {
                    <$ty as TypedValue>::parse_input(value)
                        .filter(|value| i64::try_from(*value).is_ok())
                }
            }
        )*
    };
}

filter_wide_unsigned!(u64, usize);

impl<T: FilterNumber<Value = T> + IntoExpr<T> + PartialOrd> FilterNumber for Option<T> {
    type Value = T;

    fn parse(value: &str) -> Option<T> {
        T::parse(value)
    }
}

/// Which side of a [`NumberFilter`]'s range a bound is, or neither for an exact value.
#[derive(Clone, Copy)]
enum Bound {
    Exact,
    /// The value and every greater one: `f.<name>.min`.
    Min,
    /// The value and every smaller one: `f.<name>.max`.
    Max,
}

/// A number filter's predicate for a bound, typed through its field.
type BoundFn = Arc<dyn Fn(&str, Bound) -> Option<Expr<bool>> + Send + Sync>;

/// Whether a range's `min` exceeds its `max`, read through the field's type.
type ReversedFn = Arc<dyn Fn(&str, &str) -> bool + Send + Sync>;

/// Number filter matching a number field exactly, or between two bounds; see [`FilterNumber`]
/// for the types.
///
/// A value names one number, `f.<name>=42`; on a [`range`](Self::range) filter the parts
/// `f.<name>.min` and `f.<name>.max` bound it, both ends included, either one alone leaving it
/// open. A record whose optional field holds nothing matches no bound.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
/// # struct Book { #[key] #[auto] id: uuid::Uuid, pages: i64 }
/// # use tablo_core::{NumberFilter, lens};
/// NumberFilter::new(lens!(Book.pages)).range();
/// ```
pub struct NumberFilter<M> {
    binding: Binding,
    label: Option<crate::Text>,
    matches: BoundFn,
    reversed: ReversedFn,
    range: bool,
    model: PhantomData<fn() -> M>,
}

impl<M> NumberFilter<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    /// Filter the number field `lens` binds to one value, or a range of values.
    pub fn new<T: FilterNumber>(lens: impl Into<Path<M, T>>) -> Self {
        let lens = lens.into();
        let binding = Binding::of(&lens.clone());
        let matches: BoundFn = Arc::new(move |value: &str, bound: Bound| {
            let value = T::parse(value)?;
            Some(match bound {
                Bound::Exact => lens.clone().eq(value),
                Bound::Min => lens.clone().ge(value),
                Bound::Max => lens.clone().le(value),
            })
        });
        let reversed: ReversedFn = Arc::new(
            |min: &str, max: &str| matches!((T::parse(min), T::parse(max)), (Some(min), Some(max)) if max < min),
        );
        Self {
            binding,
            label: None,
            matches,
            reversed,
            range: false,
            model: PhantomData,
        }
    }

    /// The label the control renders, over the field's.
    pub fn label(mut self, label: impl Into<crate::Text>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Choose a range: the control is two inputs, submitting the `min` and `max` parts.
    pub fn range(mut self) -> Self {
        self.range = true;
        self
    }
}

impl<M> Filter<M> for NumberFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn reads(&self) -> Option<crate::lens::Reads<M>> {
        Some(self.binding.reads())
    }

    fn name(&self) -> &str {
        self.binding.name()
    }

    fn label(&self) -> crate::Text {
        self.label.clone().unwrap_or_else(|| self.binding.label())
    }

    /// The exact value, or the range its `min` and `max` parts bound; `None` for a value the type
    /// refuses, another part, a value beside the parts, a `min` above the `max`, or a part where
    /// [`range`](Self::range) is off, whose one input does not render it.
    fn to_expr(&self, _cx: &Cx, value: &FilterValue) -> Option<Expr<bool>> {
        if value.parts.is_empty() {
            return (self.matches)(value.single()?, Bound::Exact);
        }
        if !self.range
            || !value.values.is_empty()
            || value
                .parts()
                .any(|(part, _)| part != "min" && part != "max")
        {
            return None;
        }
        let (min, max) = (value.part("min"), value.part("max"));
        if let (Some(min), Some(max)) = (min, max)
            && (self.reversed)(min, max)
        {
            return None;
        }
        let min = min.map(|min| (self.matches)(min, Bound::Min));
        let max = max.map(|max| (self.matches)(max, Bound::Max));
        match (min, max) {
            (Some(min), Some(max)) => Some(min?.and(max?)),
            (Some(bound), None) | (None, Some(bound)) => bound,
            (None, None) => None,
        }
    }

    /// A number input, or two for a range.
    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        let bound = move |value: String, name: String, aria: String, filter: Option<String>| {
            view! {
                tablo_ui::input(
                    attrs: attributes! {
                        type="number"
                        step="any"
                        name=(name)
                        value=(value)
                        aria-label=(aria)
                        class="min-w-0 flex-1"
                        data-filter-name=(filter)
                    }
                )
            }
            .boxed()
        };
        let name = input.name().to_string();
        let label = input.label().to_string();
        let held = input.value();
        let control = if self.range {
            // A bare value is both ends, so a rerun submits what it selected.
            let (min, max) = match held.single() {
                Some(single) => (single.to_string(), single.to_string()),
                None => (
                    held.part("min").unwrap_or_default().to_string(),
                    held.part("max").unwrap_or_default().to_string(),
                ),
            };
            let min_label = crate::t!(cx, "tablo-filter-min", label = label.clone());
            let max_label = crate::t!(cx, "tablo-filter-max", label = label);
            let (min_param, max_param) = (input.part_param("min"), input.part_param("max"));
            view! {
                <span class="flex items-center gap-2" data-filter-name=(name)>
                    (bound(min, min_param, min_label, None))
                    <span aria-hidden="true">"–"</span>
                    (bound(max, max_param, max_label, None))
                </span>
            }
            .boxed()
        } else {
            let value = held.single().unwrap_or_default().to_string();
            bound(value, input.param().to_string(), label, Some(name))
        };
        input.labelled(cx, control)
    }

    fn internals(&self) -> &dyn crate::table::filter::FilterInternals<M> {
        self
    }
}

impl<M> crate::table::filter::FilterInternals<M> for NumberFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        self.binding.misdeclared()
    }
}

filter_impls! {
    NumberFilter, this {
        name: &this.binding.name(),
        label: &this.label.clone().unwrap_or_else(|| this.binding.label()),
        range: &this.range,
    }
    clone { binding, label, matches, reversed, range, model }
}

#[cfg(test)]
mod tests;
