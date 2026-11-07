//! [`TextFilter`]: a string field matched by what it contains, starts or ends with, or equals.

use std::{marker::PhantomData, sync::Arc};

use toasty::stmt::{Expr, Path};
use topcoat::{context::Cx, view::*};

use super::{Filter, FilterInput, FilterValue};
use crate::{
    column::{LIKE_ESCAPE, escape_like},
    schema::Binding,
};

/// How a [`TextFilter`] matches its term, which the `f.<name>.op` part names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextMatch {
    /// The field holds the term anywhere: `contains`.
    Contains,
    /// The field holds the term nowhere, or nothing at all: `excludes`.
    Excludes,
    /// The field starts with the term: `starts`.
    StartsWith,
    /// The field ends with the term: `ends`.
    EndsWith,
    /// The field is the term: `is`.
    Is,
}

impl TextMatch {
    /// Every way, in the order the control offers them.
    pub const ALL: [Self; 5] = [
        Self::Contains,
        Self::Excludes,
        Self::StartsWith,
        Self::EndsWith,
        Self::Is,
    ];

    /// The `op` part's spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::Excludes => "excludes",
            Self::StartsWith => "starts",
            Self::EndsWith => "ends",
            Self::Is => "is",
        }
    }

    fn parse(op: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|way| way.as_str() == op)
    }

    fn message(self) -> &'static str {
        match self {
            Self::Contains => "tablo-filter-text-contains",
            Self::Excludes => "tablo-filter-text-excludes",
            Self::StartsWith => "tablo-filter-text-starts",
            Self::EndsWith => "tablo-filter-text-ends",
            Self::Is => "tablo-filter-text-is",
        }
    }

    /// The `LIKE` pattern matching `term` this way; [`Excludes`](Self::Excludes) negates the
    /// [`Contains`](Self::Contains) pattern.
    fn pattern(self, term: &str) -> String {
        let term = escape_like(term);
        match self {
            Self::Contains | Self::Excludes => format!("%{term}%"),
            Self::StartsWith => format!("{term}%"),
            Self::EndsWith => format!("%{term}"),
            Self::Is => term,
        }
    }
}

/// A field type a [`TextFilter`] matches: `String` and `Option<String>`.
pub trait FilterText:
    toasty::schema::Field<Inner = String> + Sized + Send + Sync + 'static
{
    /// The predicate selecting a record whose field `path` holds nothing, `None` for a field
    /// that always holds a value.
    fn none<M>(path: Path<M, Self>) -> Option<Expr<bool>>;
}

impl FilterText for String {
    fn none<M>(_path: Path<M, Self>) -> Option<Expr<bool>> {
        None
    }
}

impl FilterText for Option<String> {
    fn none<M>(path: Path<M, Self>) -> Option<Expr<bool>> {
        Some(path.is_none())
    }
}

/// A text filter's `LIKE` predicate for a pattern, typed through its field.
type LikeFn = Arc<dyn Fn(String) -> Expr<bool> + Send + Sync>;

/// A text filter's predicate for a record holding no value, which only `Excludes` matches.
type NoneFn = Arc<dyn Fn() -> Option<Expr<bool>> + Send + Sync>;

/// Text filter matching a `String` or `Option<String>` field by a term the user types, the way
/// a [`TextMatch`] names.
///
/// `f.<name>=<term>` matches by the first way the filter offers, [`Contains`](TextMatch::Contains)
/// unless [`matching`](Self::matching) says otherwise; `f.<name>.op=<way>` picks another it
/// offers. The match ignores case on every driver, as the table's search does, and `%` and `_`
/// match only themselves. [`Excludes`](TextMatch::Excludes) also matches a record whose optional
/// field holds nothing.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, email: String }
/// # use tablo_core::{TextFilter, TextMatch, lens};
/// TextFilter::new(lens!(User.email)).matching([TextMatch::EndsWith, TextMatch::Is]);
/// ```
pub struct TextFilter<M> {
    binding: Binding,
    label: Option<crate::Text>,
    like: LikeFn,
    none: NoneFn,
    ways: Vec<TextMatch>,
    model: PhantomData<fn() -> M>,
}

impl<M> TextFilter<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    /// Filter the string field `lens` binds by a term, every [`TextMatch`] offered.
    pub fn new<T: FilterText>(lens: impl Into<Path<M, T>>) -> Self {
        let lens = lens.into();
        let binding = Binding::of(&lens.clone());
        let like_lens = lens.clone();
        let like: LikeFn =
            Arc::new(move |pattern| like_lens.clone().like_with_escape(pattern, LIKE_ESCAPE));
        let none: NoneFn = Arc::new(move || T::none(lens.clone()));
        Self {
            binding,
            label: None,
            like,
            none,
            ways: TextMatch::ALL.to_vec(),
            model: PhantomData,
        }
    }

    /// The label the control renders, over the field's.
    pub fn label(mut self, label: impl Into<crate::Text>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Offer only `ways`, the first one applying to a term without an `op` part. One way hides
    /// the control's choice of way; none offers [`Contains`](TextMatch::Contains) alone.
    pub fn matching(mut self, ways: impl IntoIterator<Item = TextMatch>) -> Self {
        let mut offered: Vec<TextMatch> = Vec::new();
        for way in ways {
            if !offered.contains(&way) {
                offered.push(way);
            }
        }
        if offered.is_empty() {
            offered.push(TextMatch::Contains);
        }
        self.ways = offered;
        self
    }
}

/// The way `value` asks for among the `offered` ways: its `op` part's, or the first offered.
fn way(offered: &[TextMatch], value: &FilterValue) -> Option<TextMatch> {
    match value.part("op") {
        None => offered.first().copied(),
        Some(op) => TextMatch::parse(op).filter(|way| offered.contains(way)),
    }
}

impl<M> Filter<M> for TextFilter<M>
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

    /// The match of the one term the value holds, the way its `op` part names; `None` for no
    /// term or several, another part, or a way the filter does not offer.
    fn to_expr(&self, _cx: &Cx, value: &FilterValue) -> Option<Expr<bool>> {
        let [term] = value.values() else {
            return None;
        };
        if value.parts().any(|(part, _)| part != "op") {
            return None;
        }
        let way = way(&self.ways, value)?;
        let matched = (self.like)(way.pattern(term));
        Some(match way {
            TextMatch::Excludes => match (self.none)() {
                Some(none) => matched.not().or(none),
                None => matched.not(),
            },
            _ => matched,
        })
    }

    /// An `op` part alone is the control's choice of way submitted with an empty term.
    fn is_noop_value(&self, value: &FilterValue) -> bool {
        value.values().is_empty() && value.parts().all(|(part, _)| part == "op")
    }

    /// A choice of way, when the filter offers several, beside the term's input.
    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        let name = input.name().to_string();
        let label = input.label().to_string();
        let held = input.value();
        let term = held.values().first().cloned().unwrap_or_default();
        let chosen = way(&self.ways, held).or_else(|| self.ways.first().copied());
        let op_param = input.part_param("op");
        let choice = (self.ways.len() > 1).then(|| {
            let options: Vec<BoxView<'a>> = self
                .ways
                .iter()
                .map(|way| {
                    crate::schema::option_view(
                        cx,
                        way.as_str().to_string(),
                        crate::t!(cx, way.message()),
                        Some(*way) == chosen,
                    )
                })
                .collect();
            let aria = crate::t!(cx, "tablo-filter-text-way", label = label.clone());
            view! {
                tablo_ui::select(
                    attrs: attributes! {
                        name=(op_param)
                        aria-label=(aria)
                        class="w-auto shrink-0"
                    },
                    for option in options {
                        (option)
                    }
                )
            }
            .boxed()
        });
        let param = input.param().to_string();
        let control = view! {
            <span class="flex items-center gap-2" data-filter-name=(name)>
                if let Some(choice) = choice {
                    (choice)
                }
                tablo_ui::input(
                    attrs: attributes! {
                        type="search"
                        name=(param)
                        value=(term)
                        aria-label=(label)
                        class="min-w-0 flex-1"
                    }
                )
            </span>
        }
        .boxed();
        input.labelled(cx, control)
    }

    fn internals(&self) -> &dyn crate::table::filter::FilterInternals<M> {
        self
    }
}

impl<M> crate::table::filter::FilterInternals<M> for TextFilter<M>
where
    M: toasty::schema::Model + Send + Sync,
{
    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        self.binding.misdeclared()
    }
}

filter_impls! {
    TextFilter, this {
        name: &this.binding.name(),
        label: &this.label.clone().unwrap_or_else(|| this.binding.label()),
        ways: &this.ways,
    }
    clone { binding, label, like, none, ways, model }
}

#[cfg(test)]
mod tests;
