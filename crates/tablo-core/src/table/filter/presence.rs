//! [`PresenceFilter`]: an optional field matched by whether it holds a value.

use toasty::stmt::{Expr, Path};
use topcoat::{context::Cx, view::*};

use super::{Filter, FilterInput, FilterValue};
use crate::schema::Binding;

/// A presence filter's predicates: the field holds a value, and the field holds none.
type PresenceFn = std::sync::Arc<dyn Fn(bool) -> Expr<bool> + Send + Sync>;

/// Presence filter matching an `Option` field by whether it holds a value: `f.<name>=set` or
/// `f.<name>=empty`.
///
/// Over an optional foreign key it lists the records with or without a related record, such as
/// the posts with no author, without naming any related record.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, cover: Option<String> }
/// # use tablo_core::{PresenceFilter, lens};
/// PresenceFilter::new(lens!(Post.cover)).labels("With a cover", "Without a cover");
/// ```
pub struct PresenceFilter<M> {
    binding: Binding,
    label: Option<crate::Text>,
    matches: PresenceFn,
    /// The options' labels for `set` and `empty`.
    labels: (crate::Text, crate::Text),
    model: std::marker::PhantomData<fn() -> M>,
}

impl<M> PresenceFilter<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    /// Filter the optional field `lens` binds by whether it holds a value.
    pub fn new<T: Send + Sync + 'static>(lens: impl Into<Path<M, Option<T>>>) -> Self {
        let lens = lens.into();
        let binding = Binding::of(&lens.clone());
        let matches: PresenceFn = std::sync::Arc::new(move |set| {
            if set {
                lens.clone().is_some()
            } else {
                lens.clone().is_none()
            }
        });
        Self {
            binding,
            label: None,
            matches,
            labels: (
                crate::Text::message("tablo-filter-set"),
                crate::Text::message("tablo-filter-empty"),
            ),
            model: std::marker::PhantomData,
        }
    }

    /// The label the control renders, over the field's.
    pub fn label(mut self, label: impl Into<crate::Text>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The options' labels for `set` and `empty`, over "Has a value" and "Empty". The URL values
    /// stay `set` and `empty`.
    pub fn labels(
        mut self,
        set_label: impl Into<crate::Text>,
        empty_label: impl Into<crate::Text>,
    ) -> Self {
        self.labels = (set_label.into(), empty_label.into());
        self
    }
}

impl<M> Filter<M> for PresenceFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn reads(&self) -> Option<crate::lens::Reads<M>> {
        Some(self.binding.reads())
    }

    fn name(&self) -> &str {
        self.binding.name()
    }

    fn label(&self) -> crate::Text {
        self.label.clone().unwrap_or_else(|| self.binding.label())
    }

    fn to_expr(&self, _cx: &Cx, value: &FilterValue) -> Option<Expr<bool>> {
        match value.single()? {
            "set" => Some((self.matches)(true)),
            "empty" => Some((self.matches)(false)),
            _ => None,
        }
    }

    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        let (set, empty) = self.labels.clone();
        input.select(
            cx,
            vec![
                ("set".to_string(), set.localize(cx)),
                ("empty".to_string(), empty.localize(cx)),
            ],
        )
    }

    fn internals(&self) -> &dyn crate::table::filter::FilterInternals<M> {
        self
    }
}

impl<M> crate::table::filter::FilterInternals<M> for PresenceFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        self.binding.misdeclared()
    }
}

filter_impls! {
    PresenceFilter, this {
        name: &this.binding.name(),
        label: &this.label.clone().unwrap_or_else(|| this.binding.label()),
        labels: &this.labels,
    }
    clone { binding, label, matches, labels, model }
}

#[cfg(test)]
mod tests;
