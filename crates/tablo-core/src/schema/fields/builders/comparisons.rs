//! The text builder's comparisons with another field of its form.

use super::{
    super::super::comparison::{Comparison, Relation},
    TextField,
};

impl<F> TextField<F> {
    fn compare(mut self, relation: Relation, other: &TextField<F>) -> Self {
        let other = other.name().to_string();
        self.text().comparisons.push(Comparison { relation, other });
        self
    }

    /// Refuse a value not above `other`'s, a field of the same form and type that orders: a
    /// number, or a `jiff` date, time, date and time, or timestamp.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Trip {
    /// #     #[key] #[auto] id: uuid::Uuid,
    /// #     starts_on: jiff::civil::Date,
    /// #     ends_on: Option<jiff::civil::Date>,
    /// # }
    /// # use tablo_core::{Field, Schema, lens};
    /// let starts = Field::text(lens!(Trip.starts_on));
    /// let ends = Field::text(lens!(Trip.ends_on)).gte(&starts);
    /// Schema::new((starts, ends));
    /// ```
    ///
    /// - Only the server checks it, in the same round as the controls' other rules, and the error
    ///   renders on this field, naming the other by its label.
    /// - The other's value is its posted one, else the one the record stores, as a field disabled
    ///   on edit posts none. A blank value on either side, or another field a condition hides,
    ///   skips the comparison: whether a value is required is the field's type to say.
    /// - An edit that changes neither field does not check them again, as a value the record
    ///   already stores is not; changing either one does.
    /// - A field of `Option<T>` compares with one of `T`. Mounting refuses another field the schema
    ///   does not place, one of another type or control, and a type without an order. A repeater's
    ///   row field compares with another field of the same row.
    pub fn gt(self, other: &TextField<F>) -> Self {
        self.compare(Relation::Gt, other)
    }

    /// Refuse a value below `other`'s: [`gt`](Self::gt) has the rules.
    pub fn gte(self, other: &TextField<F>) -> Self {
        self.compare(Relation::Gte, other)
    }

    /// Refuse a value not below `other`'s: [`gt`](Self::gt) has the rules.
    pub fn lt(self, other: &TextField<F>) -> Self {
        self.compare(Relation::Lt, other)
    }

    /// Refuse a value above `other`'s: [`gt`](Self::gt) has the rules.
    pub fn lte(self, other: &TextField<F>) -> Self {
        self.compare(Relation::Lte, other)
    }

    /// Refuse a value equal to `other`'s, as the field's type reads both: `01` equals `1` for an
    /// integer. [`gt`](Self::gt) has the rules, but any type compares.
    pub fn different(self, other: &TextField<F>) -> Self {
        self.compare(Relation::Different, other)
    }
}
