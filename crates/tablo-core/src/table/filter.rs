//! Table filters: the [`Filter`] trait, the [`FilterValue`] it reads, the four built-in filters,
//! and the [`IntoFilters`] seam.

use std::{collections::BTreeMap, marker::PhantomData, sync::Arc};

use toasty::stmt::{Expr, IntoExpr, Path};
use topcoat::{context::Cx, icon::icon, view::*};

use crate::{
    BoxFuture,
    form::FormScalar,
    schema::{
        Binding, FieldResolver, IntoOptions, OptionLoadError, OptionSource, Options,
        relationship::related_records,
    },
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
///     fn to_expr(&self, value: &FilterValue) -> Option<Expr<bool>> {
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

    /// The predicate `value` selects, or `None` when it selects none. The table warns about a
    /// value that selects none unless [`is_noop_value`](Self::is_noop_value) accepts it.
    fn to_expr(&self, value: &FilterValue) -> Option<Expr<bool>>;

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

    /// What is wrong with this filter's declaration.
    #[doc(hidden)]
    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        None
    }

    /// Bind an embedded path through `resolver`'s app schema.
    #[doc(hidden)]
    fn bind(&self, _resolver: &FieldResolver) {}
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

/// A filter control's label, beside its control.
const FILTER_LABEL_CLASS: StaticClass =
    class!("flex items-center gap-2 text-sm font-medium whitespace-nowrap text-muted-foreground");

/// A checklist's button, sized like the filter bar's selects.
const CHECKLIST_BUTTON_CLASS: StaticClass = class!(
    "flex h-9 max-w-56 min-w-32 items-center justify-between gap-2 rounded-lg border \
     border-border bg-transparent px-3 text-sm font-normal text-foreground outline-none \
     focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 \
     focus-visible:ring-offset-background"
);

/// A checklist's popover, below its button.
const CHECKLIST_POPOVER_CLASS: StaticClass = class!(
    "inset-auto m-0 mt-1 max-h-72 min-w-48 flex-col overflow-y-auto rounded-lg border \
     border-border bg-popover p-1 text-popover-foreground shadow-sm open:flex \
     [position-area:bottom_span-right] [position-try-fallbacks:flip-block]"
);

/// One option of a checklist.
const CHECKLIST_OPTION_CLASS: StaticClass = class!(
    "flex items-center gap-2 rounded-md px-2 py-1.5 text-sm font-normal text-foreground \
     hover:bg-foreground/5"
);

/// What a [`Filter::control`] renders for one request.
#[derive(Debug, Clone)]
pub struct FilterInput {
    name: String,
    param: String,
    label: String,
    value: FilterValue,
}

impl FilterInput {
    pub(crate) fn new(name: &str, param: String, label: &str, value: FilterValue) -> Self {
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

    /// The form field name the control submits a value as; a control submitting several values
    /// repeats it.
    pub fn param(&self) -> &str {
        &self.param
    }

    /// The form field name the control submits `part` as: [`param`](Self::param)`.{part}`.
    pub fn part_param(&self, part: &str) -> String {
        format!("{}.{part}", self.param)
    }

    /// The filter's label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The value the request carries for this filter.
    pub fn value(&self) -> &FilterValue {
        &self.value
    }

    /// A labelled `<select>` over `options`, led by the empty "All" option, which is selected
    /// while the filter holds no value. A select submits one value, so a filter holding several
    /// (`?f.status=draft&f.status=review`) marks the first; the rest keep applying until the form
    /// is submitted again.
    pub fn select<'a>(self, cx: &'a Cx, options: Vec<(String, String)>) -> BoxView<'a> {
        let option_views: Vec<BoxView<'a>> = std::iter::once((String::new(), "All".to_string()))
            .chain(options)
            .map(|(value, text)| {
                let selected = if value.is_empty() {
                    self.value.is_empty()
                } else {
                    self.value.values().first() == Some(&value)
                };
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

    /// A labelled button opening a checkbox per option in `options`, each submitting its value
    /// under [`param`](Self::param) and checked while the filter holds it. The button reads the
    /// checked options' labels, or "All" while none is checked.
    ///
    /// The list is a popover, so it stays open while the table reruns as each box changes.
    pub fn checklist<'a>(self, cx: &'a Cx, options: Vec<(String, String)>) -> BoxView<'a> {
        let checked: Vec<&str> = options
            .iter()
            .filter(|(value, _)| self.value.holds(None, value))
            .map(|(_, label)| label.as_str())
            .collect();
        let summary = if checked.is_empty() {
            "All".to_string()
        } else {
            checked.join(", ")
        };
        let boxes: Vec<BoxView<'a>> = options
            .iter()
            .map(|(value, text)| {
                let checked = self.value.holds(None, value);
                let (name, value, text) = (self.param.clone(), value.clone(), text.clone());
                view! {
                    cx =>
                    <label class=(CHECKLIST_OPTION_CLASS)>
                        tablo_ui::checkbox(
                            attrs: attributes! { name=(name) value=(value) checked=(checked) }
                        )
                        <span>(text)</span>
                    </label>
                }
                .boxed()
            })
            .collect();
        let empty = boxes.is_empty();
        let Self {
            name, param, label, ..
        } = self;
        let list = format!("{param}-options");
        let label_id = format!("{param}-label");
        // The popover sits under the button that anchors it, whatever opened it.
        let anchor: String = list
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        view! {
            cx =>
            <div class=(FILTER_LABEL_CLASS) data-filter-name=(name)>
                <span id=(label_id.clone())>(label)</span>
                <button
                    type="button"
                    popovertarget=(list.clone())
                    aria-labelledby=(format!("{label_id} {list}-summary"))
                    class=(CHECKLIST_BUTTON_CLASS)
                    style=(format!("anchor-name: --{anchor}"))
                >
                    <span id=(format!("{list}-summary")) class="truncate">
                        (summary)
                    </span>
                    icon(
                        data: tablo_ui::icons::CHEVRON_DOWN,
                        attrs: attributes! { class="size-4 shrink-0 opacity-50" }
                    )
                </button>
                <div
                    id=(list)
                    popover=""
                    role="group"
                    aria-labelledby=(label_id)
                    class=(CHECKLIST_POPOVER_CLASS)
                    style=(format!("position-anchor: --{anchor}"))
                >
                    for checkbox in boxes {
                        (checkbox)
                    }
                    if empty {
                        <p class="px-2 py-1.5 text-sm text-muted-foreground">
                            "No options"
                        </p>
                    }
                </div>
            </div>
        }
        .boxed()
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

/// A select filter's predicate for a value, typed through its field.
type MatchFn = Arc<dyn Fn(&str) -> Option<Expr<bool>> + Send + Sync>;

/// A field type a [`SelectFilter`] matches: one the form parses that compares as itself, such as
/// a foreign key's.
pub trait FilterKey: FormScalar + IntoExpr<Self> + Send + Sync + 'static {}

impl<T: FormScalar + IntoExpr<T> + Send + Sync + 'static> FilterKey for T {}

/// Loads a relationship filter's `(key, label)` options.
type OptionLoader = Arc<
    dyn Fn(&Cx) -> BoxFuture<'static, Result<Vec<(String, String)>, OptionLoadError>> + Send + Sync,
>;

/// The rows a relationship filter offers.
#[derive(Clone)]
struct Related {
    load: OptionLoader,
    /// Whether the source's primary key is composite, which no option value can spell.
    composite: bool,
}

/// Select filter matching a field exactly: a `String`, any [`FormScalar`] such as an [`Options`]
/// enum, or a foreign key to a [`relationship`](Self::relationship)'s rows. Several values match
/// a record holding any of them, which [`multiple`](Self::multiple) lets the control choose.
///
/// ```rust
/// # #[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo_core::Options)]
/// # enum Status { Draft, Published }
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, kind: String, status: Status }
/// # use tablo_core::{SelectFilter, lens};
/// SelectFilter::new(lens!(Post.kind), ["news", "review"]);
/// SelectFilter::of(lens!(Post.status)).multiple();
/// ```
pub struct SelectFilter<M> {
    binding: Binding,
    label: Option<String>,
    /// The predicate for a trimmed, listed value, or `None` when the field's type refuses it.
    matches: MatchFn,
    /// `(value, label)` pairs.
    options: Vec<(String, String)>,
    /// The first listed value the field's type does not parse.
    unparsed: Option<String>,
    related: Option<Related>,
    multiple: bool,
    model: PhantomData<fn() -> M>,
}

impl<M> SelectFilter<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    /// Filter the field `lens` binds to one of `options`, each value in the field type's form
    /// spelling.
    pub fn new<T>(lens: impl Into<Path<M, T>>, options: impl IntoOptions) -> Self
    where
        T: FormScalar + IntoExpr<T> + Send + Sync + 'static,
    {
        let path: Path<M, T> = lens.into();
        let binding = Binding::of(&path);
        let matches = Arc::new(move |value: &str| {
            T::parse_form(value)
                .ok()
                .map(|value| path.clone().eq(value))
        });
        let options = options.into_options();
        // The control offers each value as written, but the parse trims what a request submits,
        // so the option the predicate lists must be the trimmed spelling: an option declared
        // with padding is otherwise declared clean and never matches.
        let options: Vec<(String, String)> = options
            .into_iter()
            .map(|(value, label)| (value.trim().to_string(), label))
            .collect();
        // An empty value is the control's "All", which selects no predicate.
        let unparsed = options
            .iter()
            .map(|(value, _)| value.as_str())
            .find(|value| !value.is_empty() && T::parse_form(value).is_err())
            .map(str::to_string);
        Self {
            binding,
            label: None,
            matches,
            options,
            unparsed,
            related: None,
            multiple: false,
            model: PhantomData,
        }
    }

    /// Filter the foreign key `lens` binds to one of the rows of the related source `R`, each
    /// valued by its primary key and labelled by the source's [`label`](OptionSource::label), as
    /// a form's relationship choice offers them.
    ///
    /// The control lists at most
    /// [`MAX_RELATIONSHIP_OPTIONS`](crate::schema::MAX_RELATIONSHIP_OPTIONS) rows, those the
    /// user may view in the request's tenant; past the cap, or when the source refuses the
    /// load, it lists the chosen keys alone. The filter matches any key the field's
    /// type parses: it narrows rows the table shows already, but does not hide that they link
    /// to a row the user cannot view (y-l-g/tablo#618). When that link is itself sensitive,
    /// scope the table's query, or offer the filter only to roles that may view the source.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Author { #[key] #[auto] id: uuid::Uuid, name: String }
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, author_id: uuid::Uuid }
    /// # struct AuthorResource;
    /// # impl tablo_core::extend::OptionSource for AuthorResource {
    /// #     type Model = Author;
    /// #     fn scoped_query(_cx: &topcoat::context::Cx)
    /// #         -> topcoat::Result<toasty::stmt::Query<toasty::stmt::List<Author>>>
    /// #     {
    /// #         Ok(toasty::stmt::Query::all())
    /// #     }
    /// #     fn label(_cx: &topcoat::context::Cx, author: &Author) -> String {
    /// #         author.name.clone()
    /// #     }
    /// # }
    /// use tablo_core::{SelectFilter, lens};
    ///
    /// SelectFilter::relationship::<AuthorResource>(lens!(Post.author_id)).label("Author");
    /// ```
    pub fn relationship<R>(lens: crate::Lens<M, impl Sized, impl FilterKey>) -> Self
    where
        R: OptionSource,
    {
        let load: OptionLoader = Arc::new(|cx: &Cx| {
            let cx = cx.clone();
            Box::pin(async move {
                let records = related_records::<R>(&cx, crate::tenancy::tenant_id(&cx))
                    .await
                    .map_err(|error| error.clone())?;
                Ok(records
                    .iter()
                    .map(|record| {
                        (
                            crate::toasty_compat::pk::pk_text(record),
                            R::label(&cx, record),
                        )
                    })
                    .collect())
            }) as BoxFuture<'static, _>
        });
        Self {
            related: Some(Related {
                load,
                composite: crate::toasty_compat::pk::pk_is_composite::<R::Model>(),
            }),
            ..Self::new(lens, Vec::<String>::new())
        }
    }

    /// Filter the field `lens` binds to one of its [`Options`] type's options.
    pub fn of<T>(lens: impl Into<Path<M, T>>) -> Self
    where
        T: Options + FormScalar + IntoExpr<T> + Send + Sync + 'static,
    {
        Self::new(lens, T::options())
    }

    /// The `(value, label)` options in declaration order; a relationship filter's load per
    /// request.
    pub fn options(&self) -> &[(String, String)] {
        &self.options
    }

    /// The label the control renders, over the field's.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Choose several options: the control is a checklist, and the filter keeps the records
    /// holding any checked value.
    pub fn multiple(mut self) -> Self {
        self.multiple = true;
        self
    }
}

impl<M> Filter<M> for SelectFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn name(&self) -> &str {
        self.binding.name()
    }

    fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(self.binding.label())
    }

    /// Any of the values, each listed and parsed by the field's type; `None` when one is not, or
    /// when the value names a part.
    fn to_expr(&self, value: &FilterValue) -> Option<Expr<bool>> {
        if !value.parts.is_empty() {
            return None;
        }
        let listed = |v: &str| {
            self.related.is_some()
                || self.options.is_empty()
                || self.options.iter().any(|(o, _)| o == v)
        };
        value
            .values()
            .iter()
            .map(|v| listed(v).then(|| (self.matches)(v)).flatten())
            .reduce(|any, one| Some(any?.or(one?)))
            .flatten()
    }

    /// A select over the options, or a checklist when the filter is
    /// [`multiple`](SelectFilter::multiple).
    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        self.control_over(cx, input, &self.options)
    }

    /// The control over the relationship's rows, once they have loaded.
    fn load_control<'s, 'a: 's>(&'s self, cx: &'a Cx, input: FilterInput) -> ControlFuture<'s, 'a> {
        Box::pin(async move {
            let Some(related) = &self.related else {
                return self.control(cx, input);
            };
            // Past the cap, or when the source refuses the load, the chosen keys stay checkable
            // so the control can clear them.
            let mut options = (related.load)(cx).await.unwrap_or_default();
            for key in input.value().values() {
                if !options.iter().any(|(value, _)| value == key) {
                    options.push((key.clone(), key.clone()));
                }
            }
            self.control_over(cx, input, &options)
        })
    }

    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        let name = || self.binding.name().to_string();
        self.binding
            .misdeclared()
            .or_else(|| {
                self.unparsed.clone().map(|value| {
                    crate::DeclarationErrorKind::UnparsedFilterOption {
                        filter: name(),
                        value,
                    }
                })
            })
            .or_else(|| {
                self.related
                    .as_ref()
                    .filter(|related| related.composite)
                    .map(|_| crate::DeclarationErrorKind::CompositeKeyChoice { field: name() })
            })
    }

    fn bind(&self, resolver: &FieldResolver) {
        self.binding.bind(resolver);
    }
}

impl<M> SelectFilter<M> {
    /// The control over `options`: a select, or a checklist when the filter is
    /// [`multiple`](Self::multiple).
    fn control_over<'a>(
        &self,
        cx: &'a Cx,
        input: FilterInput,
        options: &[(String, String)],
    ) -> BoxView<'a> {
        // An empty value is the select's own "All".
        let options = options
            .iter()
            .filter(|(value, _)| !value.is_empty())
            .cloned()
            .collect();
        if self.multiple {
            input.checklist(cx, options)
        } else {
            input.select(cx, options)
        }
    }
}

filter_impls! {
    SelectFilter, this {
        name: &this.binding.name(),
        label: &this.label.as_deref().unwrap_or(this.binding.label()),
        options: &this.options,
        relationship: &this.related.is_some(),
        multiple: &this.multiple,
    }
    clone { binding, label, matches, options, unparsed, related, multiple, model }
}

/// Ternary filter matching a `bool` field.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, featured: bool }
/// # use tablo_core::{TernaryFilter, lens};
/// TernaryFilter::new(lens!(Post.featured)).labels("Featured", "Not featured");
/// ```
pub struct TernaryFilter<M> {
    binding: Binding,
    label: Option<String>,
    lens: Path<M, bool>,
    /// The options' labels for `true` and `false`.
    labels: (String, String),
}

impl<M> TernaryFilter<M>
where
    M: toasty::schema::Model,
{
    /// Filter the `bool` field `lens` binds to true, false, or either.
    pub fn new(lens: impl Into<Path<M, bool>>) -> Self {
        let lens = lens.into();
        let binding = Binding::of(&lens.clone());
        Self {
            binding,
            label: None,
            lens,
            labels: ("True".to_string(), "False".to_string()),
        }
    }

    /// The label the control renders, over the field's.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The options' labels for `true` and `false`, over "True" and "False". The URL values stay
    /// `true` and `false`.
    pub fn labels(mut self, true_label: impl Into<String>, false_label: impl Into<String>) -> Self {
        self.labels = (true_label.into(), false_label.into());
        self
    }
}

impl<M> Filter<M> for TernaryFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        self.binding.misdeclared()
    }

    fn bind(&self, resolver: &FieldResolver) {
        self.binding.bind(resolver);
    }

    fn name(&self) -> &str {
        self.binding.name()
    }

    fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(self.binding.label())
    }

    fn to_expr(&self, value: &FilterValue) -> Option<Expr<bool>> {
        match value.single()? {
            "true" => Some(self.lens.clone().eq(true)),
            "false" => Some(self.lens.clone().eq(false)),
            _ => None,
        }
    }

    /// The documented no-op value selecting no predicate.
    fn is_noop_value(&self, value: &FilterValue) -> bool {
        value.single() == Some("all")
    }

    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        let (yes, no) = self.labels.clone();
        input.select(
            cx,
            vec![("true".to_string(), yes), ("false".to_string(), no)],
        )
    }
}

filter_impls! {
    TernaryFilter, this {
        name: &this.binding.name(),
        label: &this.label.as_deref().unwrap_or(this.binding.label()),
        labels: &this.labels,
    }
    clone { binding, label, lens, labels }
}

/// A field type a [`DateFilter`] matches by calendar day: `jiff::Timestamp` (the day in UTC),
/// `jiff::civil::Date`, `jiff::civil::DateTime`, and an `Option` of any of them.
pub trait FilterDate: Sized + Send + Sync + 'static {
    /// The value the field compares against.
    type Value: IntoExpr<Self>;

    /// The first instant of `date`, or `None` past the type's range.
    fn start_of(date: jiff::civil::Date) -> Option<Self::Value>;

    /// A submitted value naming one exact value rather than a day.
    fn exact(value: &str) -> Option<Self::Value> {
        let _ = value;
        None
    }
}

impl FilterDate for jiff::Timestamp {
    type Value = Self;

    fn start_of(date: jiff::civil::Date) -> Option<Self> {
        date.to_zoned(jiff::tz::TimeZone::UTC)
            .ok()
            .map(|day| day.timestamp())
    }

    fn exact(value: &str) -> Option<Self> {
        // Query decoding turns `+` into space.
        value
            .parse()
            .or_else(|_| value.replace(' ', "+").parse())
            .ok()
    }
}

impl FilterDate for jiff::civil::Date {
    type Value = Self;

    fn start_of(date: jiff::civil::Date) -> Option<Self> {
        Some(date)
    }
}

impl FilterDate for jiff::civil::DateTime {
    type Value = Self;

    fn start_of(date: jiff::civil::Date) -> Option<Self> {
        Some(date.to_datetime(jiff::civil::Time::midnight()))
    }

    fn exact(value: &str) -> Option<Self> {
        value.contains('T').then(|| value.parse().ok()).flatten()
    }
}

impl<T: FilterDate<Value = T> + IntoExpr<T>> FilterDate for Option<T> {
    type Value = T;

    fn start_of(date: jiff::civil::Date) -> Option<T> {
        T::start_of(date)
    }

    fn exact(value: &str) -> Option<T> {
        T::exact(value)
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

/// A date filter's predicate for a value, which is one day, or one side of a range.
type BoundFn = Arc<dyn Fn(&str, Option<Bound>) -> Option<Expr<bool>> + Send + Sync>;

/// Date filter matching a date or time field by calendar day, or by a range of days; see
/// [`FilterDate`] for the types.
///
/// A value names one day, `f.<name>=2024-01-15`; the parts `f.<name>.from` and `f.<name>.to`
/// name the first and the last day of a range, either one alone leaving it open. A value naming
/// an exact time, such as a `jiff::Timestamp`'s `2024-01-15T09:30:00Z`, matches that time, or
/// bounds the range at it.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, created_at: jiff::Timestamp }
/// # use tablo_core::{DateFilter, lens};
/// DateFilter::new(lens!(Post.created_at)).range();
/// ```
pub struct DateFilter<M> {
    binding: Binding,
    label: Option<String>,
    /// The predicate for a value.
    matches: BoundFn,
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
        let matches: BoundFn = Arc::new(move |value: &str, bound: Option<Bound>| {
            if let Some(exact) = T::exact(value) {
                return Some(match bound {
                    None => lens.clone().eq(exact),
                    Some(Bound::From) => lens.clone().ge(exact),
                    Some(Bound::To) => lens.clone().le(exact),
                });
            }
            let date = value.parse::<jiff::civil::Date>().ok()?;
            let start = || T::start_of(date).map(|start| lens.clone().ge(start));
            // The day after the last day is past the type's range when there is none.
            let end = || {
                date.tomorrow()
                    .ok()
                    .and_then(T::start_of)
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
        Self {
            binding,
            label: None,
            matches,
            range: false,
            model: PhantomData,
        }
    }

    /// The label the control renders, over the field's.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Choose a range of days: the control is two date inputs, submitting the `from` and `to`
    /// parts.
    pub fn range(mut self) -> Self {
        self.range = true;
        self
    }
}

impl<M> Filter<M> for DateFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        self.binding.misdeclared()
    }

    fn bind(&self, resolver: &FieldResolver) {
        self.binding.bind(resolver);
    }

    fn name(&self) -> &str {
        self.binding.name()
    }

    fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(self.binding.label())
    }

    /// One day for a single value, or the range its `from` and `to` parts bound; `None` for both,
    /// another part, or a range ending before it starts.
    fn to_expr(&self, value: &FilterValue) -> Option<Expr<bool>> {
        if value.parts.is_empty() {
            return (self.matches)(value.single()?, None);
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
            && let (Ok(first), Ok(last)) = (
                from.parse::<jiff::civil::Date>(),
                to.parse::<jiff::civil::Date>(),
            )
            && last < first
        {
            return None;
        }
        let from = from.map(|from| (self.matches)(from, Some(Bound::From)));
        let to = to.map(|to| (self.matches)(to, Some(Bound::To)));
        match (from, to) {
            (Some(from), Some(to)) => Some(from?.and(to?)),
            (Some(bound), None) | (None, Some(bound)) => bound,
            (None, None) => None,
        }
    }

    /// A date input, or two bounding a range.
    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        let day = |value: Option<&str>| {
            let value = value.unwrap_or_default();
            value.split('T').next().unwrap_or(value).to_string()
        };
        let name = input.name().to_string();
        let aria = input.label().to_string();
        let control = if self.range {
            let (from, to) = (
                day(input.value().part("from")),
                day(input.value().part("to")),
            );
            let (from_param, to_param) = (input.part_param("from"), input.part_param("to"));
            view! {
                cx =>
                <span class="flex items-center gap-1" data-filter-name=(name)>
                    tablo_ui::input(
                        attrs: attributes! {
                            type="date"
                            name=(from_param)
                            value=(from)
                            aria-label=(format!("{aria} from"))
                            class="w-auto!"
                        }
                    )
                    <span aria-hidden="true">"–"</span>
                    tablo_ui::input(
                        attrs: attributes! {
                            type="date"
                            name=(to_param)
                            value=(to)
                            aria-label=(format!("{aria} to"))
                            class="w-auto!"
                        }
                    )
                </span>
            }
            .boxed()
        } else {
            let value = day(input.value().single());
            let param = input.param().to_string();
            view! {
                cx =>
                tablo_ui::input(
                    attrs: attributes! {
                        type="date"
                        name=(param)
                        data-filter-name=(name)
                        value=(value)
                        aria-label=(aria)
                        class="w-auto!"
                    }
                )
            }
            .boxed()
        };
        input.labelled(cx, control)
    }
}

filter_impls! {
    DateFilter, this {
        name: &this.binding.name(),
        label: &this.label.as_deref().unwrap_or(this.binding.label()),
        range: &this.range,
    }
    clone { binding, label, matches, range, model }
}

/// A filter offering named predicates, such as an embedded-enum variant or any query the app
/// names. Several values match a record any of their predicates selects, which
/// [`multiple`](Self::multiple) lets the control choose.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, featured: bool }
/// # use tablo_core::QueryFilter;
/// # let _promoted: QueryFilter<Post> =
/// QueryFilter::new("promoted", "Promoted")
///     .option("Promoted", Post::fields().featured().eq(true))
///     .option_keyed(
///         "backlog",
///         "In the backlog",
///         Post::fields().featured().eq(false),
///     );
/// # let _ = _promoted;
/// ```
pub struct QueryFilter<M> {
    name: String,
    label: String,
    /// `(key, label, predicate)` triples.
    options: Vec<(String, String, Expr<bool>)>,
    multiple: bool,
    _marker: std::marker::PhantomData<M>,
}

impl<M> QueryFilter<M>
where
    M: toasty::schema::Model,
{
    /// A filter whose URL parameter is `name` and whose control reads `label`, with no option
    /// yet.
    pub fn new(name: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            label: label.into(),
            options: Vec::new(),
            multiple: false,
            _marker: std::marker::PhantomData,
        }
    }

    /// Offer `predicate` as the option `label`, which is also its URL value.
    pub fn option(self, label: impl Into<String>, predicate: Expr<bool>) -> Self {
        let label = label.into();
        self.option_keyed(label.clone(), label, predicate)
    }

    /// Offer `predicate` as the option `label`, whose URL value is `key`: a link keeps working
    /// when the label changes.
    pub fn option_keyed(
        mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        predicate: Expr<bool>,
    ) -> Self {
        self.options.push((key.into(), label.into(), predicate));
        self
    }

    /// Choose several options: the control is a checklist, and the filter keeps the records any
    /// checked option's predicate selects.
    pub fn multiple(mut self) -> Self {
        self.multiple = true;
        self
    }
}

impl<M> Filter<M> for QueryFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn label(&self) -> &str {
        &self.label
    }

    /// Any of the values' predicates; `None` when one names no option, or when the value names a
    /// part.
    fn to_expr(&self, value: &FilterValue) -> Option<Expr<bool>> {
        if !value.parts.is_empty() {
            return None;
        }
        let predicate = |v: &String| {
            self.options
                .iter()
                .find(|(key, ..)| key == v)
                .map(|(.., predicate)| predicate.clone())
        };
        value
            .values()
            .iter()
            .map(predicate)
            .reduce(|any, one| Some(any?.or(one?)))
            .flatten()
    }

    /// A select over the option labels, or a checklist when the filter is
    /// [`multiple`](QueryFilter::multiple).
    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        let options = self
            .options
            .iter()
            .map(|(key, label, _)| (key.clone(), label.clone()))
            .collect();
        if self.multiple {
            input.checklist(cx, options)
        } else {
            input.select(cx, options)
        }
    }
}

filter_impls! {
    QueryFilter, this {
        name: &this.name,
        label: &this.label,
        options: &this.options.iter().map(|(key, ..)| key).collect::<Vec<_>>(),
        multiple: &this.multiple,
    }
    clone { name, label, options, multiple, _marker }
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
