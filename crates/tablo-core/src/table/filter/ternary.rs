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
    label: Option<crate::Text>,
    lens: Path<M, bool>,
    /// The options' labels for `true` and `false`.
    labels: (crate::Text, crate::Text),
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
            labels: (
                crate::Text::message("tablo-filter-true"),
                crate::Text::message("tablo-filter-false"),
            ),
        }
    }

    /// The label the control renders, over the field's.
    pub fn label(mut self, label: impl Into<crate::Text>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The options' labels for `true` and `false`, over "True" and "False". The URL values stay
    /// `true` and `false`.
    pub fn labels(
        mut self,
        true_label: impl Into<crate::Text>,
        false_label: impl Into<crate::Text>,
    ) -> Self {
        self.labels = (true_label.into(), false_label.into());
        self
    }
}

impl<M> Filter<M> for TernaryFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn reads(&self) -> Option<crate::lens::Reads<M>> {
        Some(self.binding.reads())
    }

    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        self.binding.misdeclared()
    }

    fn name(&self) -> &str {
        self.binding.name()
    }

    fn label(&self) -> crate::Text {
        self.label.clone().unwrap_or_else(|| self.binding.label())
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
            vec![
                ("true".to_string(), yes.localize(cx)),
                ("false".to_string(), no.localize(cx)),
            ],
        )
    }
}

filter_impls! {
    TernaryFilter, this {
        name: &this.binding.name(),
        label: &this.label.clone().unwrap_or_else(|| this.binding.label()),
        labels: &this.labels,
    }
    clone { binding, label, lens, labels }
}
