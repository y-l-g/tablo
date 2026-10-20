//! [`TernaryFilter`]: a `bool` field matched true, false, or either.

use toasty::stmt::{Expr, Path};
use topcoat::{context::Cx, view::*};

use super::{Filter, FilterInput, FilterValue};
use crate::schema::Binding;

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

    fn name(&self) -> &str {
        self.binding.name()
    }

    fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(self.binding.label())
    }

    fn to_expr(&self, _cx: &Cx, value: &FilterValue) -> Option<Expr<bool>> {
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
