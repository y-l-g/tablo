//! [`DateColumn`]: a date field rendered as a calendar day, over any [`DateValue`].

use derive_where::derive_where;
use jiff::tz::TimeZone;
use toasty::stmt::OrderByExpr;
use topcoat::{context::Cx, view::*};

use super::{Column, ColumnBase, ColumnWidth, sort_order};
use crate::{Lens, values::FormScalar};

/// A column of a date field, rendered as a calendar day.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, published_at: jiff::Timestamp }
/// tablo_core::DateColumn::new(tablo_core::lens!(Post.published_at)).sortable();
/// ```
///
/// The cell spells the day in the request's locale, `Oct 14, 2026` in English, a timestamp's in
/// the request's [`time_zone`](crate::time_zone), and a `None` renders the table's dash. The export
/// writes the stored spelling, `2026-10-14T09:30:00Z` or `2026-10-14`, as a
/// [`TextColumn`](super::TextColumn) does; a group header follows the field, not the column.
#[derive_where(Clone, Debug)]
pub struct DateColumn<M, T> {
    lens: Lens<M, T>,
    base: ColumnBase,
    sortable: bool,
}

/// The field types a [`DateColumn`] spells as a calendar day: `jiff::civil::Date`,
/// `jiff::civil::DateTime`, `jiff::Timestamp` (read in the request's
/// [`time_zone`](crate::time_zone), as a [`DateFilter`](crate::DateFilter) reads it) and an
/// `Option` of any of them.
pub trait DateValue: Sized + Send + Sync + 'static {
    /// The value's calendar day in `zone`, `None` when it holds none. A civil value names its day
    /// whatever the zone.
    fn calendar_date(&self, zone: &TimeZone) -> Option<jiff::civil::Date>;

    /// The value's stored spelling, which a column's text and export carry. The default spells
    /// the calendar day in UTC as an ISO date.
    fn stored_text(&self) -> String {
        self.calendar_date(&TimeZone::UTC)
            .map(|day| day.to_string())
            .unwrap_or_default()
    }
}

impl DateValue for jiff::civil::Date {
    fn calendar_date(&self, _zone: &TimeZone) -> Option<jiff::civil::Date> {
        Some(*self)
    }

    fn stored_text(&self) -> String {
        FormScalar::to_form(self)
    }
}

impl DateValue for jiff::civil::DateTime {
    fn calendar_date(&self, _zone: &TimeZone) -> Option<jiff::civil::Date> {
        Some(self.date())
    }

    fn stored_text(&self) -> String {
        FormScalar::to_form(self)
    }
}

impl DateValue for jiff::Timestamp {
    fn calendar_date(&self, zone: &TimeZone) -> Option<jiff::civil::Date> {
        Some(self.to_zoned(zone.clone()).date())
    }

    fn stored_text(&self) -> String {
        FormScalar::to_form(self)
    }
}

impl<T: DateValue> DateValue for Option<T> {
    fn calendar_date(&self, zone: &TimeZone) -> Option<jiff::civil::Date> {
        self.as_ref().and_then(|value| value.calendar_date(zone))
    }

    fn stored_text(&self) -> String {
        self.as_ref()
            .map(DateValue::stored_text)
            .unwrap_or_default()
    }
}

impl<M, T> DateColumn<M, T>
where
    M: toasty::schema::Model,
{
    /// Bind the column to the date field `lens` reads.
    pub fn new(lens: Lens<M, T>) -> Self
    where
        T: DateValue,
    {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Auto),
            lens,
            sortable: false,
        }
    }

    base_builders!(sortable);
}

impl<M, T> Column<M> for DateColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: DateValue,
{
    base_column_methods!();

    fn text(&self, _cx: &Cx, row: &M) -> String {
        self.lens.read(row).stored_text()
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let day = self
            .lens
            .read(row)
            .calendar_date(&crate::time_zone(cx))
            .map(|day| {
                crate::t!(
                    cx,
                    "tablo-column-date",
                    year = day.year(),
                    month = day.month(),
                    day = day.day()
                )
            })
            .unwrap_or_default();
        crate::schema::value_cell(cx, &day)
    }

    fn order_by(&self, descending: bool) -> Option<OrderByExpr> {
        self.sortable
            .then(|| sort_order(self.lens.path(), descending))
    }
}

impl<M, T> crate::column::ColumnInternals<M> for DateColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: DateValue,
{
    base_internals!(binding);
}
