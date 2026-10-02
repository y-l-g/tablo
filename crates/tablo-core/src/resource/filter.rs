//! Table filters: the [`Filter`] trait, the four built-in filters, and the
//! [`IntoFilters`] seam.

use std::sync::Arc;

use toasty::stmt::Expr;
use topcoat::{context::Cx, view::*};

use crate::schema::{FieldLens, IntoOptions, LensBinding};

/// One table filter declares a control and its predicate.
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
    /// The filter's identifier, distinct within its table.
    fn name(&self) -> &str;

    /// The label the control renders beside it.
    fn label(&self) -> &str;

    /// The predicate `value` selects.
    fn to_expr(&self, value: &str) -> Option<Expr<bool>>;

    /// Whether `value` selects no predicate without being invalid.
    fn is_noop_value(&self, _value: &str) -> bool {
        false
    }

    /// The control in the filter bar.
    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a>;

    /// What is wrong with this filter's declaration.
    #[doc(hidden)]
    fn misdeclared(&self) -> Option<String> {
        None
    }
}

/// A filter control's label, beside its control.
const FILTER_LABEL_CLASS: StaticClass =
    class!("flex items-center gap-2 text-sm font-medium whitespace-nowrap text-muted-foreground");

/// What a [`Filter::control`] renders for one request.
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

    /// The form field name the control submits.
    pub fn param(&self) -> &str {
        &self.param
    }

    /// The filter's label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The value the request carries for this filter.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// A labelled `<select>` over `options`, led by the empty "All" option.
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

    /// `control` beside the filter's label.
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

/// Select filter matching a `String` field exactly.
pub struct SelectFilter<M> {
    name: String,
    label: String,
    lens: FieldLens<M, String>,
    /// `(value, label)` pairs.
    options: Vec<(String, String)>,
    misdeclared: Option<String>,
}

impl<M> SelectFilter<M>
where
    M: toasty::schema::Model,
{
    /// Build a select filter from a lens and its options.
    pub fn r#for(lens: FieldLens<M, String>, options: impl IntoOptions) -> Self {
        let binding = LensBinding::of(lens.clone());
        Self {
            name: binding.name,
            label: binding.label,
            lens,
            options: options.into_options(),
            misdeclared: binding.misdeclared,
        }
    }

    /// The `(value, label)` options in declaration order.
    pub fn options(&self) -> &[(String, String)] {
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
        if !self.options.is_empty() && !self.options.iter().any(|(value, _)| value == v) {
            return None;
        }
        Some(self.lens.clone().eq(v.to_string()))
    }

    /// A select over the options.
    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        let options = self
            .options
            .iter()
            .map(|(value, label)| {
                let label = if value.is_empty() {
                    "All"
                } else {
                    label.as_str()
                };
                (value.clone(), label.to_string())
            })
            .collect();
        input.select(cx, options)
    }

    fn misdeclared(&self) -> Option<String> {
        self.misdeclared.clone()
    }
}

filter_impls! {
    SelectFilter, this {
        name: &this.name,
        label: &this.label,
        options: &this.options,
    }
    clone { name, label, lens, options, misdeclared }
}

/// Ternary filter matching a `bool` field.
pub struct TernaryFilter<M> {
    name: String,
    label: String,
    lens: FieldLens<M, bool>,
    misdeclared: Option<String>,
}

impl<M> TernaryFilter<M>
where
    M: toasty::schema::Model,
{
    pub fn r#for(lens: FieldLens<M, bool>) -> Self {
        let binding = LensBinding::of(lens.clone());
        Self {
            name: binding.name,
            label: binding.label,
            lens,
            misdeclared: binding.misdeclared,
        }
    }
}

impl<M> Filter<M> for TernaryFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn misdeclared(&self) -> Option<String> {
        self.misdeclared.clone()
    }

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

    /// The documented no-op value selecting no predicate.
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
    clone { name, label, lens, misdeclared }
}

/// Date filter matching a `Timestamp` field by calendar day.
pub struct DateFilter<M> {
    name: String,
    label: String,
    lens: FieldLens<M, jiff::Timestamp>,
    misdeclared: Option<String>,
}

impl<M> DateFilter<M>
where
    M: toasty::schema::Model,
{
    pub fn r#for(lens: FieldLens<M, jiff::Timestamp>) -> Self {
        let binding = LensBinding::of(lens.clone());
        Self {
            name: binding.name,
            label: binding.label,
            lens,
            misdeclared: binding.misdeclared,
        }
    }
}

impl<M> Filter<M> for DateFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn misdeclared(&self) -> Option<String> {
        self.misdeclared.clone()
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn label(&self) -> &str {
        &self.label
    }

    /// Build the predicate for a submitted value.
    fn to_expr(&self, value: &str) -> Option<Expr<bool>> {
        let v = value.trim();
        if v.is_empty() {
            return None;
        }
        if let Ok(ts) = v.parse::<jiff::Timestamp>() {
            return Some(self.lens.clone().eq(ts));
        }
        // Query decoding turns `+` into space.
        if v.contains(' ')
            && let Ok(ts) = v.replace(' ', "+").parse::<jiff::Timestamp>()
        {
            return Some(self.lens.clone().eq(ts));
        }
        if let Ok(date) = v.parse::<jiff::civil::Date>() {
            let start: jiff::Timestamp = format!("{date}T00:00:00Z").parse().ok()?;
            return Some(match start.checked_add(jiff::Span::new().hours(24)) {
                Ok(end) => self.lens.clone().ge(start).and(self.lens.clone().lt(end)),
                Err(_) => self.lens.clone().ge(start),
            });
        }
        None
    }

    /// A date input.
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
    clone { name, label, lens, misdeclared }
}

/// Variant filter matching an embedded-enum variant.
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
    /// Build a variant filter from a name, a label and its options.
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

    /// A select over the variant keys.
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

into_filters_single!(SelectFilter, TernaryFilter, DateFilter, VariantFilter);

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

#[cfg(test)]
mod tests;
