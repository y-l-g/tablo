//! Table filters: the [`Filter`] trait, the four built-in filters, and the
//! [`IntoFilters`] seam.

use std::sync::Arc;

use toasty::stmt::Expr;
use topcoat::{context::Cx, view::*};

use crate::schema::{FieldLens, lens_field, lens_label};

/// One table filter: a control in the filter bar, and the predicate its
/// submitted value selects.
///
/// The value travels as `?f.<name>=<value>`. The built-in [`SelectFilter`],
/// [`TernaryFilter`], [`DateFilter`] and [`VariantFilter`] implement this
/// trait and nothing more, so an app filter has the same reach: implement
/// it, and pass the value to [`Table::filters`](super::Table::filters).
///
/// ```ignore
/// struct Adults;
///
/// impl Filter<User> for Adults {
///     fn name(&self) -> &str { "adults" }
///     fn label(&self) -> &str { "Adults" }
///     fn to_expr(&self, value: &str) -> Option<Expr<bool>> {
///         (value == "yes").then(|| User::fields().age().ge(18))
///     }
///     fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
///         input.select(cx, vec![("yes".into(), "Adults only".into())])
///     }
/// }
/// ```
pub trait Filter<M>: Send + Sync {
    /// The filter's identifier, distinct within its table: the `<name>` in
    /// `?f.<name>=`.
    fn name(&self) -> &str;

    /// The label the control renders beside it.
    fn label(&self) -> &str;

    /// The predicate `value` selects, or `None` for a value the filter
    /// refuses. A refused value is reported above the table, and refuses
    /// the export, unless [`is_noop_value`](Self::is_noop_value) accepts it.
    fn to_expr(&self, value: &str) -> Option<Expr<bool>>;

    /// Whether `value` is a documented "no filter" value, for which
    /// [`to_expr`](Self::to_expr) returns `None` without the value being
    /// invalid. Defaults to `false`.
    fn is_noop_value(&self, _value: &str) -> bool {
        false
    }

    /// The control in the filter bar. [`FilterInput`] carries the
    /// parameter name the control submits and the current value, and
    /// renders the built-in select.
    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a>;
}

/// A filter control's label, beside its control.
const FILTER_LABEL_CLASS: StaticClass =
    class!("flex items-center gap-2 text-sm font-medium whitespace-nowrap text-muted-foreground");

/// What a [`Filter::control`] renders for one request: the parameter its
/// field submits, the current value, and the filter's label.
///
/// A control is a real field of the filter bar's GET form, named
/// [`param`](Self::param), and carries `data-filter-name` set to
/// [`name`](Self::name): `filters.js` submits the form on change, or, on a
/// live table, rewrites the filter in the query. [`select`](Self::select)
/// and [`labelled`](Self::labelled) write both.
#[derive(Debug, Clone)]
pub struct FilterInput {
    name: String,
    param: String,
    label: String,
    value: String,
}

impl FilterInput {
    pub(crate) fn new(name: &str, param: String, label: &str, value: String) -> Self {
        Self {
            name: name.to_string(),
            param,
            label: label.to_string(),
            value,
        }
    }

    /// The filter's name, which the control carries as `data-filter-name`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The form field name the control submits: `f.<name>`, prefixed for a
    /// relation table.
    pub fn param(&self) -> &str {
        &self.param
    }

    /// The filter's label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The value the request carries for this filter, empty when none.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// A labelled `<select>` over `options`, `(value, text)` pairs, led by
    /// the empty "All" option that clears the filter.
    ///
    /// The empty value is reserved for that option. Every pair renders
    /// verbatim, so a filter whose options can include `""` supplies the text
    /// that option shows.
    pub fn select<'a>(self, cx: &'a Cx, options: Vec<(String, String)>) -> BoxView<'a> {
        let option_views: Vec<BoxView<'a>> = std::iter::once((String::new(), "All".to_string()))
            .chain(options)
            .map(|(value, text)| {
                let selected = self.value == value;
                crate::schema::option_view(cx, value, text, selected)
            })
            .collect();
        let Self {
            name, param, label, ..
        } = self;
        let aria = label.clone();
        let control = view! {
            cx =>
            tablo_ui::select(
                attrs: attributes! {
                    class="min-w-32"
                    name=(param)
                    data-filter-name=(name)
                    aria-label=(aria)
                },
                for option in option_views {
                    (option)
                }
            )
        }
        .boxed();
        labelled(cx, label, control)
    }

    /// `control` beside the filter's label, as the filter bar lays out every
    /// control. The control itself carries [`param`](Self::param) as its
    /// `name` and [`name`](Self::name) as its `data-filter-name`.
    pub fn labelled<'a>(self, cx: &'a Cx, control: BoxView<'a>) -> BoxView<'a> {
        labelled(cx, self.label, control)
    }
}

/// `control` inside the label element the filter bar lays out.
fn labelled<'a>(cx: &'a Cx, label: String, control: BoxView<'a>) -> BoxView<'a> {
    view! {
        cx =>
        <label class=(FILTER_LABEL_CLASS)>
            (label)
            (control)
        </label>
    }
    .boxed()
}

/// Generate a built-in filter's `Clone` and its metadata-only `Debug`.
///
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

    pub fn options(&self) -> &[String] {
        &self.options
    }
}

impl<M> Filter<M> for SelectFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn to_expr(&self, value: &str) -> Option<Expr<bool>> {
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

    /// A select over the options. A declared empty option is the
    /// clear-filter value, so it renders as the "All" option.
    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        let options = self
            .options
            .iter()
            .map(|opt| {
                let label = if opt.is_empty() { "All" } else { opt.as_str() };
                (opt.clone(), label.to_string())
            })
            .collect();
        input.select(cx, options)
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
}

impl<M> Filter<M> for TernaryFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn to_expr(&self, value: &str) -> Option<Expr<bool>> {
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
    fn is_noop_value(&self, value: &str) -> bool {
        value.trim() == "all"
    }

    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        input.select(
            cx,
            vec![
                ("true".to_string(), "True".to_string()),
                ("false".to_string(), "False".to_string()),
            ],
        )
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
}

impl<M> Filter<M> for DateFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn label(&self) -> &str {
        &self.label
    }

    /// Build the predicate for a submitted value.
    ///
    /// Full RFC3339 timestamps match the exact instant (documented); a
    /// date-only `YYYY-MM-DD` matches the whole UTC day
    /// (`>= midnight AND < next midnight`), so rows stamped with any
    /// time-of-day still match. A day whose end lies past
    /// `jiff::Timestamp::MAX` (9999-12-30) has no instant for the upper bound
    /// to exclude, so it matches `>= midnight` alone.
    fn to_expr(&self, value: &str) -> Option<Expr<bool>> {
        let v = value.trim();
        if v.is_empty() {
            return None;
        }
        // Accept RFC3339 or YYYY-MM-DD (whole UTC day).
        if let Ok(ts) = v.parse::<jiff::Timestamp>() {
            return Some(self.lens.clone().eq(ts));
        }
        // Query decoding turns `+` into space, destroying numeric offsets
        // (`?f.created_at=2024-01-15T09:30:00+02:00` arrives with a
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

    /// A date input. `<input type=date>` takes `YYYY-MM-DD`, so an RFC3339
    /// value is cut at its `T`.
    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        let value = input.value();
        let date_value = value.split('T').next().unwrap_or(value).to_string();
        let param = input.param().to_string();
        let name = input.name().to_string();
        let aria = input.label().to_string();
        let control = view! {
            cx =>
            tablo_ui::input(
                attrs: attributes! {
                    type="date"
                    name=(param)
                    data-filter-name=(name)
                    value=(date_value)
                    aria-label=(aria)
                    class="w-auto!"
                }
            )
        }
        .boxed();
        input.labelled(cx, control)
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

    pub fn options(&self) -> &[(String, Expr<bool>)] {
        &self.options
    }
}

impl<M> Filter<M> for VariantFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn to_expr(&self, value: &str) -> Option<Expr<bool>> {
        let v = value.trim();
        if v.is_empty() {
            return None;
        }
        self.options
            .iter()
            .find(|(k, _)| k == v)
            .map(|(_, e)| e.clone())
    }

    /// A select over the variant keys, each shown as itself.
    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        let options = self
            .options
            .iter()
            .map(|(key, _)| (key.clone(), key.clone()))
            .collect();
        input.select(cx, options)
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

/// A table's filters, as the table stores them.
pub(crate) type BoxFilter<M> = Arc<dyn Filter<M>>;

/// Convert a single built-in filter, or a tuple of any [`Filter`]s, into a
/// table's filter list.
///
/// A tuple takes filters of any type, an app's own among them; a single app
/// filter is a one-element tuple, `(MyFilter,)`. Arity eight is the shared
/// ceiling [`IntoColumns`](super::IntoColumns) documents.
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

into_filters_single!(SelectFilter, TernaryFilter, DateFilter, VariantFilter);

/// Generate the tuple impls of [`IntoFilters`] from one list per arity.
///
/// One invocation builds the destructured bindings and the converted vector
/// from the same list, so an element cannot reach one and not the other.
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

#[cfg(test)]
mod tests;
