//! [`DateFilter`]: a date or time field matched by calendar day or by a range of days, over any
//! [`FilterDate`].

use std::{marker::PhantomData, sync::Arc};

use jiff::tz::TimeZone;
use toasty::stmt::{Expr, IntoExpr, Path};
use topcoat::{context::Cx, view::*};

use super::{Filter, FilterInput, FilterValue};
use crate::schema::Binding;

/// A field type a [`DateFilter`] matches by calendar day: `jiff::Timestamp` (the day in the
/// request's [`time_zone`](crate::time_zone)), `jiff::civil::Date`, `jiff::civil::DateTime`, and an
/// `Option` of any of them.
pub trait FilterDate: Sized + Send + Sync + 'static {
    /// The value the field compares against, and orders a range's ends by.
    type Value: IntoExpr<Self> + PartialOrd;

    /// The first instant of `date` in `zone`, or `None` past the type's range. A civil type's day
    /// starts at its own midnight, whatever the zone.
    fn start_of(date: jiff::civil::Date, zone: &TimeZone) -> Option<Self::Value>;

    /// A submitted value naming one exact value rather than a day, a wall-clock time read in
    /// `zone`.
    fn exact(value: &str, zone: &TimeZone) -> Option<Self::Value> {
        let _ = (value, zone);
        None
    }
}

/// Whether the ends `from` and `to` bound no range at all, each resolved the way the predicate
/// resolves it in `zone`: an exact value the type reads, else the day's first instant.
///
/// An exact end bounds the range at itself, both ends included; a day end excludes the day after
/// it. An unresolvable end answers `false`, leaving the refusal to the predicate.
fn reversed_range<T: FilterDate>(from: &str, to: &str, zone: &TimeZone) -> bool {
    let start_of = |date| T::start_of(date, zone);
    let Some(lower) =
        T::exact(from, zone).or_else(|| from.parse::<jiff::civil::Date>().ok().and_then(start_of))
    else {
        return false;
    };
    match T::exact(to, zone) {
        Some(upper) => upper < lower,
        None => to
            .parse::<jiff::civil::Date>()
            .ok()
            .and_then(|date| date.tomorrow().ok())
            .and_then(start_of)
            .is_some_and(|after| after <= lower),
    }
}

impl FilterDate for jiff::Timestamp {
    type Value = Self;

    fn start_of(date: jiff::civil::Date, zone: &TimeZone) -> Option<Self> {
        date.to_zoned(zone.clone()).ok().map(|day| day.timestamp())
    }

    /// An RFC 3339 timestamp, or a date and time without an offset, read in `zone`.
    fn exact(value: &str, zone: &TimeZone) -> Option<Self> {
        // Query decoding turns `+` into space.
        value
            .parse()
            .or_else(|_| value.replace(' ', "+").parse())
            .ok()
            .or_else(|| crate::values::read_datetime_local(value, zone))
    }
}

impl FilterDate for jiff::civil::Date {
    type Value = Self;

    fn start_of(date: jiff::civil::Date, _zone: &TimeZone) -> Option<Self> {
        Some(date)
    }
}

impl FilterDate for jiff::civil::DateTime {
    type Value = Self;

    fn start_of(date: jiff::civil::Date, _zone: &TimeZone) -> Option<Self> {
        Some(date.to_datetime(jiff::civil::Time::midnight()))
    }

    fn exact(value: &str, _zone: &TimeZone) -> Option<Self> {
        value.contains('T').then(|| value.parse().ok()).flatten()
    }
}

impl<T: FilterDate<Value = T> + IntoExpr<T> + PartialOrd> FilterDate for Option<T> {
    type Value = T;

    fn start_of(date: jiff::civil::Date, zone: &TimeZone) -> Option<T> {
        T::start_of(date, zone)
    }

    fn exact(value: &str, zone: &TimeZone) -> Option<T> {
        T::exact(value, zone)
    }
}

/// Which side of a [`DateFilter`]'s range a bound is.
#[derive(Clone, Copy)]
enum Bound {
    /// The day a value names and every later one: `f.<name>.from`.
    From,
    /// The day a value names and every earlier one: `f.<name>.to`.
    To,
}

/// A date filter's predicate for a value, which is one day, or one side of a range, read in a
/// time zone.
type BoundFn = Arc<dyn Fn(&str, Option<Bound>, &TimeZone) -> Option<Expr<bool>> + Send + Sync>;

/// Whether a range's two ends bound nothing, read through the field's own type in a time zone.
type ReversedFn = Arc<dyn Fn(&str, &str, &TimeZone) -> bool + Send + Sync>;

/// Date filter matching a date or time field by calendar day, or by a range of days; see
/// [`FilterDate`] for the types.
///
/// A value names one day, `f.<name>=2024-01-15`, a timestamp's in the request's
/// [`time_zone`](crate::time_zone); on a [`range`](Self::range) filter the parts
/// `f.<name>.from` and `f.<name>.to` name the first and the last day of a range, either one
/// alone leaving it open. A value naming an exact time, such as a `jiff::Timestamp`'s
/// `2024-01-15T09:30:00Z`, or its `2024-01-15T09:30` read in the request's time zone, matches that
/// time, or bounds the range at it.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, created_at: jiff::Timestamp }
/// # use tablo_core::{DateFilter, lens};
/// DateFilter::new(lens!(Post.created_at)).range();
/// ```
pub struct DateFilter<M> {
    binding: Binding,
    label: Option<crate::Text>,
    /// The predicate for a value.
    matches: BoundFn,
    /// Whether a range's `from` and `to` parts bound nothing.
    reversed: ReversedFn,
    range: bool,
    model: PhantomData<fn() -> M>,
}

impl<M> DateFilter<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    /// Filter the field `lens` binds to one calendar day, or a range of days.
    pub fn new<T: FilterDate>(lens: impl Into<Path<M, T>>) -> Self {
        let lens = lens.into();
        let binding = Binding::of(&lens.clone());
        let matches: BoundFn = Arc::new(move |value: &str, bound: Option<Bound>, zone| {
            if let Some(exact) = T::exact(value, zone) {
                return Some(match bound {
                    None => lens.clone().eq(exact),
                    Some(Bound::From) => lens.clone().ge(exact),
                    Some(Bound::To) => lens.clone().le(exact),
                });
            }
            let date = value.parse::<jiff::civil::Date>().ok()?;
            let start = || T::start_of(date, zone).map(|start| lens.clone().ge(start));
            // The day after the last day is past the type's range when there is none.
            let end = || {
                date.tomorrow()
                    .ok()
                    .and_then(|after| T::start_of(after, zone))
                    .map(|end| lens.clone().lt(end))
            };
            match bound {
                None => Some(match end() {
                    Some(end) => start()?.and(end),
                    None => start()?,
                }),
                Some(Bound::From) => start(),
                // The last day the type holds bounds nothing.
                Some(Bound::To) => end().or_else(|| start().map(|_| true.into_expr())),
            }
        });
        let reversed: ReversedFn = Arc::new(reversed_range::<T>);
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

    /// Choose a range of days: the control is two inputs, submitting the `from` and `to` parts.
    pub fn range(mut self) -> Self {
        self.range = true;
        self
    }
}

impl<M> Filter<M> for DateFilter<M>
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

    /// One day for a single value, or the range its `from` and `to` parts bound; `None` for both,
    /// another part, a value beside the parts, a range ending before it starts, or a part where
    /// [`range`](Self::range) is off, whose one input does not render it.
    fn to_expr(&self, cx: &Cx, value: &FilterValue) -> Option<Expr<bool>> {
        let zone = crate::time_zone(cx);
        if value.parts.is_empty() {
            return (self.matches)(value.single()?, None, &zone);
        }
        if !self.range {
            return None;
        }
        if !value.values.is_empty()
            || value
                .parts()
                .any(|(part, _)| part != "from" && part != "to")
        {
            return None;
        }
        let (from, to) = (value.part("from"), value.part("to"));
        if let (Some(from), Some(to)) = (from, to)
            && (self.reversed)(from, to, &zone)
        {
            return None;
        }
        let from = from.map(|from| (self.matches)(from, Some(Bound::From), &zone));
        let to = to.map(|to| (self.matches)(to, Some(Bound::To), &zone));
        match (from, to) {
            (Some(from), Some(to)) => Some(from?.and(to?)),
            (Some(bound), None) | (None, Some(bound)) => bound,
            (None, None) => None,
        }
    }

    /// The bound inputs: a date input for a day, a text input for a bound a date input cannot
    /// hold.
    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        // A date input keeps a calendar day and submits it back. A bound a date input cannot
        // hold — an exact time, or a day spelled with one — stays in a text input, which submits
        // it back unharmed, where a date input would drop it and the rerun would widen the bound
        // to its day.
        let bound =
            |value: String, name: String, aria: String, filter: Option<String>| -> BoxView<'a> {
                let kind = if value.is_empty()
                    || (value.len() == 10 && value.parse::<jiff::civil::Date>().is_ok())
                {
                    "date"
                } else {
                    "text"
                };
                view! {
                    cx =>
                    tablo_ui::input(
                        attrs: attributes! {
                            type=(kind)
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
        let aria = input.label().to_string();
        let held = input.value();
        let control = if self.range {
            // A bare value bounds the range at itself, so both ends spell it: a rerun submits
            // `from=` and `to=` the same, which selects what the bare value selected.
            let (from, to) = match held.single() {
                Some(single) => (single.to_string(), single.to_string()),
                None => (
                    held.part("from").unwrap_or_default().to_string(),
                    held.part("to").unwrap_or_default().to_string(),
                ),
            };
            let (from_param, to_param) = (input.part_param("from"), input.part_param("to"));
            view! {
                cx =>
                <span class="flex items-center gap-2" data-filter-name=(name)>
                    (bound(from, from_param, format!("{aria} from"), None))
                    <span aria-hidden="true">"–"</span>
                    (bound(to, to_param, format!("{aria} to"), None))
                </span>
            }
            .boxed()
        } else {
            let param = input.param().to_string();
            let value = held.single().unwrap_or_default().to_string();
            bound(value, param, aria, Some(name))
        };
        input.labelled(cx, control)
    }

    fn internals(&self) -> &dyn crate::table::filter::FilterInternals<M> {
        self
    }
}

impl<M> crate::table::filter::FilterInternals<M> for DateFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        self.binding.misdeclared()
    }
}

filter_impls! {
    DateFilter, this {
        name: &this.binding.name(),
        label: &this.label.clone().unwrap_or_else(|| this.binding.label()),
        range: &this.range,
    }
    clone { binding, label, matches, reversed, range, model }
}
