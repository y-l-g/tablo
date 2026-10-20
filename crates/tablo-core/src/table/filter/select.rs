//! [`SelectFilter`]: a field matched exactly against listed options, an [`Options`] enum's, or a
//! relationship's rows.

use std::{marker::PhantomData, sync::Arc};

use toasty::stmt::{Expr, IntoExpr, Path};
use topcoat::{context::Cx, view::*};

use super::{ControlFuture, Filter, FilterInput, FilterValue};
use crate::{
    schema::{Binding, IntoOptions, OptionSource, Options, fields::Relationship},
    values::FormScalar,
};

/// A select filter's predicate for a value, typed through its field.
type MatchFn = Arc<dyn Fn(&str) -> Option<Expr<bool>> + Send + Sync>;

/// A field type a [`SelectFilter`] matches: one the form parses that compares as itself, such as
/// a foreign key's.
pub trait FilterKey: FormScalar + IntoExpr<Self> + Send + Sync + 'static {}

impl<T: FormScalar + IntoExpr<T> + Send + Sync + 'static> FilterKey for T {}

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
    related: Option<Relationship>,
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
    /// A form's relationship choice loads the same rows the same way, so its loader is reused.
    pub fn relationship<R>(lens: crate::Lens<M, impl Sized, impl FilterKey>) -> Self
    where
        R: OptionSource,
    {
        Self {
            related: Some(Relationship::new::<R>(R::label)),
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
    fn to_expr(&self, _cx: &Cx, value: &FilterValue) -> Option<Expr<bool>> {
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
            // so the control can clear them; the loader logs what failed.
            let mut options = related.load()(cx).await.unwrap_or_default();
            for key in input.value().values() {
                if !options.iter().any(|(value, _)| value == key) {
                    options.push((key.clone(), key.clone()));
                }
            }
            self.control_over(cx, input, &options)
        })
    }

    /// The source's type when the request's panel cannot load from it.
    fn unavailable_source(&self, cx: &Cx) -> Option<&'static str> {
        self.related
            .as_ref()
            .and_then(|related| related.unavailable_source(cx))
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
                    .filter(|related| related.has_composite_source())
                    .map(|_| crate::DeclarationErrorKind::CompositeKeyChoice { field: name() })
            })
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
