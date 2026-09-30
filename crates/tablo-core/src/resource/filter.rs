//! Table filters: the four typed filter kinds plus the [`Filter`] seam.

use toasty::stmt::Expr;

use crate::schema::{FieldLens, lens_field, lens_label};

/// Generate a filter's label accessors, its `Clone`, and its metadata-only
/// `Debug`.
///
/// Every filter declares a `name` and a `label`, so the accessors need no list.
/// `debug` pairs each field `Debug` prints with the expression that renders it;
/// that expression reads the receiver through the `this` bound alongside the
/// type, because a macro body's own `self` is not visible to a call-site
/// expression. `Clone` copies the `clone` list, lens included, because a cloned
/// filter still builds the same predicate. `#[derive]` would put
/// `M: Clone + Debug` on every impl, which `toasty::schema::Model` does not
/// carry; `Path`'s `Clone` and `Debug` are unconditional at the pinned toasty
/// rev, so the expanded impls stay bound-free.
macro_rules! filter_impls {
    (
        $ty:ident, $this:ident {
            $( $field:ident: $value:expr ),* $(,)?
        }
        clone { $( $clone:ident ),* $(,)? }
    ) => {
        impl<M> $ty<M>
        where
            M: toasty::schema::Model,
        {
            pub fn name(&self) -> &str {
                &self.name
            }

            pub fn label_str(&self) -> &str {
                &self.label
            }
        }

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

/// Select filter — exact match on a `String` field (e.g. `status = "published"`).
pub struct SelectFilter<M> {
    name: String,
    label: String,
    lens: FieldLens<M, String>,
    options: Vec<String>,
}

impl<M> SelectFilter<M>
where
    M: toasty::schema::Model,
{
    /// Call sites read `SelectFilter::for(Post::fields().status(), vec![...])`.
    pub fn r#for(lens: FieldLens<M, String>, options: Vec<String>) -> Self {
        let field = lens_field(lens.clone(), &M::schema());
        let (name, label) = (field.name.app_unwrap().to_string(), lens_label(&field));
        Self {
            name,
            label,
            lens,
            options,
        }
    }

    pub fn to_expr(&self, value: &str) -> Option<Expr<bool>> {
        let v = value.trim();
        if v.is_empty() {
            return None;
        }
        // Only allow values in options; otherwise ignore (no filter).
        if !self.options.is_empty() && !self.options.contains(&v.to_string()) {
            return None;
        }
        Some(self.lens.clone().eq(v.to_string()))
    }

    pub fn options(&self) -> &[String] {
        &self.options
    }
}

filter_impls! {
    SelectFilter, this {
        name: &this.name,
        label: &this.label,
        options: &this.options,
    }
    clone { name, label, lens, options }
}

/// Ternary filter — `true` / `false` / `all` (no filter) on a `bool` field.
pub struct TernaryFilter<M> {
    name: String,
    label: String,
    lens: FieldLens<M, bool>,
}

impl<M> TernaryFilter<M>
where
    M: toasty::schema::Model,
{
    pub fn r#for(lens: FieldLens<M, bool>) -> Self {
        let field = lens_field(lens.clone(), &M::schema());
        let (name, label) = (field.name.app_unwrap().to_string(), lens_label(&field));
        Self { name, label, lens }
    }

    pub fn to_expr(&self, value: &str) -> Option<Expr<bool>> {
        match value.trim() {
            "true" => Some(self.lens.clone().eq(true)),
            "false" => Some(self.lens.clone().eq(false)),
            _ => None,
        }
    }

    /// The documented no-op value: `all` selects no predicate, and
    /// — unlike any other rejected value — it is neutral, never `"invalid
    /// value"`. `to_expr` still returns `None` for it (there is no predicate
    /// to build); `Table::unapplied_filters` consults this so the no-op is
    /// never flagged and the export never refuses it.
    pub fn is_noop_value(value: &str) -> bool {
        value.trim() == "all"
    }
}

filter_impls! {
    TernaryFilter, this {
        name: &this.name,
        label: &this.label,
    }
    clone { name, label, lens }
}

/// Date filter — same-calendar-day match on a `Timestamp` field
/// (e.g. `created_at = "2024-01-15"` selects that whole day).
/// Range (`from`/`to`) support is future.
pub struct DateFilter<M> {
    name: String,
    label: String,
    lens: FieldLens<M, jiff::Timestamp>,
}

impl<M> DateFilter<M>
where
    M: toasty::schema::Model,
{
    pub fn r#for(lens: FieldLens<M, jiff::Timestamp>) -> Self {
        let field = lens_field(lens.clone(), &M::schema());
        let (name, label) = (field.name.app_unwrap().to_string(), lens_label(&field));
        Self { name, label, lens }
    }

    /// Build the predicate for a submitted value.
    ///
    /// Full RFC3339 timestamps match the exact instant (documented); a
    /// date-only `YYYY-MM-DD` matches the whole UTC day
    /// (`>= midnight AND < next midnight`), so rows stamped with any
    /// time-of-day still match. A day whose end lies past
    /// `jiff::Timestamp::MAX` (9999-12-30) has no instant for the upper bound
    /// to exclude, so it matches `>= midnight` alone.
    pub fn to_expr(&self, value: &str) -> Option<Expr<bool>> {
        let v = value.trim();
        if v.is_empty() {
            return None;
        }
        // Accept RFC3339 or YYYY-MM-DD (whole UTC day).
        if let Ok(ts) = v.parse::<jiff::Timestamp>() {
            return Some(self.lens.clone().eq(ts));
        }
        // Query decoding turns `+` into space, destroying numeric offsets
        // (`?filters=created_at:2024-01-15T09:30:00+02:00` arrives with a
        // space). A timestamp never legitimately contains a space, so retry
        // with `+` restored before giving up.
        if v.contains(' ')
            && let Ok(ts) = v.replace(' ', "+").parse::<jiff::Timestamp>()
        {
            return Some(self.lens.clone().eq(ts));
        }
        if let Ok(date) = v.parse::<jiff::civil::Date>() {
            let start: jiff::Timestamp = format!("{date}T00:00:00Z").parse().ok()?;
            // The day's end can lie past `Timestamp::MAX` (9999-12-30): `+` would panic
            // on a user-supplied URL, so the last day is bounded below only — no instant
            // exists past the maximum for the upper bound to exclude.
            return Some(match start.checked_add(jiff::Span::new().hours(24)) {
                Ok(end) => self.lens.clone().ge(start).and(self.lens.clone().lt(end)),
                Err(_) => self.lens.clone().ge(start),
            });
        }
        None
    }
}

filter_impls! {
    DateFilter, this {
        name: &this.name,
        label: &this.label,
    }
    clone { name, label, lens }
}

/// Variant filter — exact match on an embedded-enum variant (e.g. `vehicule = "Moto"`).
///
/// Unlike [`SelectFilter`] (a `String` lens + options), a variant has no single
/// lens: Toasty stores it as one discriminant column plus one nullable column
/// per variant field. The caller therefore supplies prebuilt expressions —
/// typically `User::fields().vehicule().is_moto()` — one per option. Display
/// stays `TextColumn::computed` (see).
pub struct VariantFilter<M> {
    name: String,
    label: String,
    options: Vec<(String, Expr<bool>)>,
    _marker: std::marker::PhantomData<M>,
}

impl<M> VariantFilter<M>
where
    M: toasty::schema::Model,
{
    /// Convenience alias so call sites read `VariantFilter::for("vehicule", "Véhicule",
    /// vec![...])`.
    pub fn r#for(
        name: impl Into<String>,
        label: impl Into<String>,
        options: Vec<(String, Expr<bool>)>,
    ) -> Self {
        Self {
            name: name.into(),
            label: label.into(),
            options,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn to_expr(&self, value: &str) -> Option<Expr<bool>> {
        let v = value.trim();
        if v.is_empty() {
            return None;
        }
        self.options
            .iter()
            .find(|(k, _)| k == v)
            .map(|(_, e)| e.clone())
    }

    pub fn options(&self) -> &[(String, Expr<bool>)] {
        &self.options
    }
}

filter_impls! {
    VariantFilter, this {
        name: &this.name,
        label: &this.label,
        options: &this.options.iter().map(|(k, _)| k).collect::<Vec<_>>(),
    }
    clone { name, label, options, _marker }
}

/// Filter enum — the `Table::filters` seam.
#[derive(Debug, Clone)]
pub enum Filter<M> {
    Select(SelectFilter<M>),
    Ternary(TernaryFilter<M>),
    Date(DateFilter<M>),
    Variant(VariantFilter<M>),
}

impl<M> From<SelectFilter<M>> for Filter<M> {
    fn from(v: SelectFilter<M>) -> Self {
        Filter::Select(v)
    }
}
impl<M> From<TernaryFilter<M>> for Filter<M> {
    fn from(v: TernaryFilter<M>) -> Self {
        Filter::Ternary(v)
    }
}
impl<M> From<DateFilter<M>> for Filter<M> {
    fn from(v: DateFilter<M>) -> Self {
        Filter::Date(v)
    }
}
impl<M> From<VariantFilter<M>> for Filter<M> {
    fn from(v: VariantFilter<M>) -> Self {
        Filter::Variant(v)
    }
}

impl<M> Filter<M>
where
    M: toasty::schema::Model,
{
    pub fn name(&self) -> &str {
        match self {
            Filter::Select(f) => f.name(),
            Filter::Ternary(f) => f.name(),
            Filter::Date(f) => f.name(),
            Filter::Variant(f) => f.name(),
        }
    }
    pub fn to_expr(&self, value: &str) -> Option<Expr<bool>> {
        match self {
            Filter::Select(f) => f.to_expr(value),
            Filter::Ternary(f) => f.to_expr(value),
            Filter::Date(f) => f.to_expr(value),
            Filter::Variant(f) => f.to_expr(value),
        }
    }
    /// Whether this value is a documented no-op for this filter:
    /// only `TernaryFilter`'s `all` qualifies — every other rejected value
    /// is genuinely invalid.
    pub fn is_noop_value(&self, value: &str) -> bool {
        match self {
            Filter::Ternary(_) => TernaryFilter::<M>::is_noop_value(value),
            _ => false,
        }
    }
}

/// Convert a single filter or tuple of filters into `Vec<Filter<M>>`.
pub trait IntoFilters<M> {
    fn into_filters(self) -> Vec<Filter<M>>;
}

impl<M> IntoFilters<M> for Filter<M> {
    fn into_filters(self) -> Vec<Filter<M>> {
        vec![self]
    }
}
impl<M> IntoFilters<M> for SelectFilter<M> {
    fn into_filters(self) -> Vec<Filter<M>> {
        vec![self.into()]
    }
}
impl<M> IntoFilters<M> for TernaryFilter<M> {
    fn into_filters(self) -> Vec<Filter<M>> {
        vec![self.into()]
    }
}
impl<M> IntoFilters<M> for DateFilter<M> {
    fn into_filters(self) -> Vec<Filter<M>> {
        vec![self.into()]
    }
}
impl<M> IntoFilters<M> for VariantFilter<M> {
    fn into_filters(self) -> Vec<Filter<M>> {
        vec![self.into()]
    }
}
/// Generate the tuple impls of [`IntoFilters`] from one list per arity.
///
/// One invocation builds the destructured bindings and the converted vector
/// from the same list, so an element cannot reach one and not the other. Arity
/// eight is the shared ceiling [`IntoColumns`](super::IntoColumns) documents.
macro_rules! into_filters_tuples {
    ($($T:ident => $v:ident),+ $(,)?) => {
        impl<M, $($T),+> IntoFilters<M> for ($($T,)+)
        where
            $($T: Into<Filter<M>>,)+
        {
            fn into_filters(self) -> Vec<Filter<M>> {
                let ($($v,)+) = self;
                vec![$($v.into(),)+]
            }
        }
    };
}

into_filters_tuples!(A => a, B => b);
into_filters_tuples!(A => a, B => b, C => c);
into_filters_tuples!(A => a, B => b, C => c, D => d);
into_filters_tuples!(A => a, B => b, C => c, D => d, E => e);
into_filters_tuples!(A => a, B => b, C => c, D => d, E => e, F => f);
into_filters_tuples!(A => a, B => b, C => c, D => d, E => e, F => f, G => g);
into_filters_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h
);

#[cfg(test)]
mod tests;
