//! [`QueryFilter`]: named predicates the app declares, offered as options.

use toasty::stmt::Expr;
use topcoat::{context::Cx, view::*};

use super::{Filter, FilterInput, FilterValue};

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
