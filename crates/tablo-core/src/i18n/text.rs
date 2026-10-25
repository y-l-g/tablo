//! [`Text`]: a word a panel renders, plain or a catalog's message.

use std::borrow::Cow;

use topcoat::{
    context::Cx,
    view::{AttributeValueViewParts, NodeViewParts, PartsWriter},
};

use crate::naming::{capitalize, derived_id, pluralize, sentence_case, slug_case, type_short_name};

/// A word a panel renders: plain text, or a message the request's
/// [`Locale`](topcoat::l10n::Locale) spells.
///
/// Every builder that takes a label, a description or a placeholder takes `impl Into<Text>`, so a
/// string literal stays plain and a message translates:
///
/// ```rust
/// # use tablo_core::{Text, TextColumn, lens, text};
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
/// TextColumn::new(lens!(Post.title)).label("Headline"); // plain: every locale reads "Headline"
/// TextColumn::new(lens!(Post.title)).label(Text::message("post-headline"));
/// TextColumn::new(lens!(Post.title)).label(text!("post-headline")); // the same message
/// ```
///
/// A message names a Fluent message of the panel's catalogs, or one of its attributes as
/// `id.attribute`. Each argument fills the placeable of its name: a number selects a plural
/// variant, and a `Text` argument renders in the same locale first.
///
/// ```ftl
/// post-headline = Headline
/// posts-published = { $count ->
///     [one] One post published
///    *[other] { $count } posts published
/// }
/// ```
///
/// A message no catalog of the request's locale holds reads from the default locale's, then from
/// English, and one no catalog holds at all renders its id, so a missing translation shows on the
/// page rather than an empty label.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Text(pub(super) Repr);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Repr {
    Plain(Cow<'static, str>),
    /// A number argument, which a message's plural selector reads as a number.
    Integer(i128),
    Message(Box<Message>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct Message {
    /// The message's id, with `.attribute` when it names an attribute.
    pub(super) id: Cow<'static, str>,
    pub(super) args: Vec<(Cow<'static, str>, Text)>,
    /// What renders when no catalog holds the message: a derived label's English spelling.
    pub(super) fallback: Option<Cow<'static, str>>,
}

impl Text {
    /// The message `id` of the panel's catalogs, or its attribute as `id.attribute`.
    #[must_use]
    pub fn message(id: impl Into<Cow<'static, str>>) -> Self {
        Self(Repr::Message(Box::new(Message {
            id: id.into(),
            args: Vec::new(),
            fallback: None,
        })))
    }

    /// Fills the message's placeable `name` with `value`.
    ///
    /// # Panics
    ///
    /// In a debug build, on plain text, which has no placeable to fill. A release build ignores
    /// the argument.
    #[must_use]
    pub fn arg(mut self, name: impl Into<Cow<'static, str>>, value: impl Into<Self>) -> Self {
        let name = name.into();
        match &mut self.0 {
            Repr::Message(message) => message.args.push((name, value.into())),
            _ => debug_assert!(
                false,
                "`Text::arg({name:?})` on plain text, which reads none"
            ),
        }
        self
    }

    /// The text as the request's [`Locale`](topcoat::l10n::Locale) spells it: in a panel, the one
    /// it negotiated; outside every panel, English.
    pub fn localize(&self, cx: &Cx) -> String {
        let (catalog, locale) = super::catalog_and_locale(cx);
        catalog.format(locale, self)
    }

    /// The message `id`, which renders `fallback` when no catalog holds it: the label a
    /// declaration derives from a name, which an app translates by defining `id`.
    pub(crate) fn derived(id: String, fallback: String) -> Self {
        Self(Repr::Message(Box::new(Message {
            id: id.into(),
            args: Vec::new(),
            fallback: Some(fallback.into()),
        })))
    }

    /// The name a column takes after its label: the plain text in lowercase, or the message's id.
    pub(crate) fn key(&self) -> String {
        match &self.0 {
            Repr::Plain(text) => text.to_lowercase(),
            Repr::Integer(n) => n.to_string(),
            Repr::Message(message) => message.id.to_string(),
        }
    }

    /// The label Tablo derives for the field, column or filter of `name`: the message
    /// `field-{name}`, else `name` capitalized, `word_count` reading `Word count`. A field, a
    /// column and a filter of one name share it.
    pub(crate) fn field(name: &str) -> Self {
        Self::derived(derived_id("field", name), capitalize(name))
    }

    /// The label Tablo derives for the variant `variant` of the enum `enum_name`, both in snake
    /// case: the message attribute `option-{enum}.{variant}`, the enum in kebab case, else the
    /// variant capitalized. `PostStatus::PublishedLate` reads `option-post-status.published_late`,
    /// else `Published late`.
    pub(crate) fn option(enum_name: &str, variant: &str) -> Self {
        let id = format!("option-{}.{variant}", enum_name.replace('_', "-"));
        Self::derived(id, capitalize(variant))
    }

    /// The label Tablo derives for the model `M`'s records: the message `resource-{model}`, else
    /// the model's type name in sentence case. `BlogPost` reads `resource-blog-post`, else
    /// `Blog post`.
    pub(crate) fn model<M: ?Sized>() -> Self {
        let noun = sentence_case(type_short_name::<M>());
        Self::derived(derived_id("resource", &slug_case(&noun)), noun)
    }

    /// The plural of a resource's label: plain text pluralized in English, and a message's
    /// attribute `plural`, with its arguments, which renders its fallback pluralized when no
    /// catalog holds it.
    pub(crate) fn plural(&self) -> Self {
        match &self.0 {
            Repr::Plain(text) => Self::from(pluralize(text)),
            Repr::Integer(_) => self.clone(),
            Repr::Message(message) => Self(Repr::Message(Box::new(Message {
                id: format!("{}.plural", message.id).into(),
                args: message.args.clone(),
                fallback: message
                    .fallback
                    .as_deref()
                    .map(|fallback| pluralize(fallback).into()),
            }))),
        }
    }

    /// The English spelling, as Tablo's own catalog writes it, for what has no request: a
    /// declaration error, a log line.
    pub(crate) fn english(&self) -> String {
        super::Catalog::english().format(&super::ENGLISH, self)
    }
}

/// The label `#[derive(Options)]` gives a variant it declares no label for: the message
/// `option-{enum}.{variant}`, else the variant capitalized.
pub fn option_label(enum_name: &str, variant: &str) -> Text {
    Text::option(enum_name, variant)
}

/// A text renders in the request's locale, in a `view!`'s content or an attribute's value.
impl NodeViewParts for Text {
    fn into_view_parts(self, cx: &Cx, parts: &mut PartsWriter<'_>) {
        parts.push_string(self.localize(cx));
    }
}

impl NodeViewParts for &Text {
    fn into_view_parts(self, cx: &Cx, parts: &mut PartsWriter<'_>) {
        parts.push_string(self.localize(cx));
    }
}

impl AttributeValueViewParts for Text {
    fn attribute_present(&self) -> bool {
        true
    }

    fn into_view_parts(self, cx: &Cx, parts: &mut PartsWriter<'_>) {
        parts.push_string(self.localize(cx));
    }
}

impl AttributeValueViewParts for &Text {
    fn attribute_present(&self) -> bool {
        true
    }

    fn into_view_parts(self, cx: &Cx, parts: &mut PartsWriter<'_>) {
        parts.push_string(self.localize(cx));
    }
}

/// A test compares a text with its English spelling.
#[cfg(test)]
impl PartialEq<&str> for Text {
    fn eq(&self, other: &&str) -> bool {
        self.english() == *other
    }
}

/// The empty plain text.
impl Default for Text {
    fn default() -> Self {
        Self(Repr::Plain(Cow::Borrowed("")))
    }
}

impl From<&str> for Text {
    fn from(text: &str) -> Self {
        Self(Repr::Plain(Cow::Owned(text.to_owned())))
    }
}

impl From<&Text> for Text {
    fn from(text: &Text) -> Self {
        text.clone()
    }
}

impl From<&String> for Text {
    fn from(text: &String) -> Self {
        Self(Repr::Plain(Cow::Owned(text.clone())))
    }
}

impl From<String> for Text {
    fn from(text: String) -> Self {
        Self(Repr::Plain(Cow::Owned(text)))
    }
}

impl From<Cow<'static, str>> for Text {
    fn from(text: Cow<'static, str>) -> Self {
        Self(Repr::Plain(text))
    }
}

macro_rules! from_integer {
    ($($int:ty)*) => {$(
        impl From<$int> for Text {
            fn from(n: $int) -> Self {
                Self(Repr::Integer(i128::from(n)))
            }
        }
    )*};
}
from_integer!(i8 i16 i32 i64 u8 u16 u32 u64);

impl From<usize> for Text {
    fn from(n: usize) -> Self {
        Self(Repr::Integer(i128::try_from(n).unwrap_or(i128::MAX)))
    }
}

impl From<isize> for Text {
    fn from(n: isize) -> Self {
        Self(Repr::Integer(i128::try_from(n).unwrap_or(i128::MAX)))
    }
}

/// A [`Text`] message with its arguments: `text!("id", name = value, ..)` is
/// `Text::message("id").arg("name", value)..`.
///
/// ```rust
/// # use tablo_core::text;
/// let shipped = text!("orders-shipped", count = 3);
/// ```
#[macro_export]
macro_rules! text {
    ($id:expr $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::Text::message($id)$(.arg(stringify!($name), $value))*
    };
}

/// A message spelled in the request's locale: `t!(cx, "id", name = value, ..)` is
/// `text!("id", name = value, ..).localize(cx)`, for what takes a `String`, such as a
/// [`Notification`](crate::Notification)'s title.
///
/// ```rust
/// # use tablo_core::{Notification, t};
/// # fn shipped(cx: &topcoat::context::Cx) -> Notification {
/// Notification::success(t!(cx, "orders-shipped", count = 3))
/// # }
/// ```
#[macro_export]
macro_rules! t {
    ($cx:expr, $id:expr $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::text!($id $(, $name = $value)*).localize($cx)
    };
}
