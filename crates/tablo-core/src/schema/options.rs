//! Typed options: one list of `(value, label)` pairs a choice field, a select
//! filter and a column share.

/// A closed set of values, each with its label.
///
/// Derive it on a unit-variant enum with
/// [`#[derive(Options)]`](crate::Options): each variant's value is its
/// `snake_case` name and its label that name in sentence case, and
/// `#[option(value = "..", label = "..")]` overrides either.
///
/// The derive also makes the enum a [`FormScalar`](crate::FormScalar) that a
/// form posts as its value and a column reads as its label, so a model field
/// of the enum's type, derived `toasty::Embed`, binds without a string in
/// between:
///
/// ```rust
/// # use tablo_core::{Field, SelectFilter, Table, TextColumn, lens, Options};
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo_core::Options)]
/// enum Status {
///     Draft,
///     #[option(label = "Live")]
///     Published,
/// }
///
/// #[derive(Debug, Clone, toasty::Model)]
/// struct Post {
///     #[key]
///     #[auto]
///     id: uuid::Uuid,
///     status: Status,
/// }
///
/// Field::choice(lens!(Post.status)).options(Status::options());
/// SelectFilter::of(lens!(Post.status));
/// Table::new(TextColumn::new(lens!(Post.status))).group_by(lens!(Post.status));
/// Post::filter(Post::fields().status().eq(Status::Published));
/// ```
///
/// Toasty stores the variant under its own discriminant, `snake_case` by
/// default; the option's value is only the form's spelling. A `String` field
/// takes the same list, storing the value itself:
/// `SelectFilter::new(lens!(Post.kind), Kind::options())`.
///
/// The derive also gives the enum `value()`, `label()` and `from_value()`.
pub trait Options {
    /// Every option as `(value, label)`, in declaration order.
    fn options() -> Vec<(String, String)>;

    /// The label of a `value`, or `value` itself when no option has it: a
    /// `String` row written before an option was renamed still reads.
    fn label_of(value: &str) -> String {
        Self::options()
            .into_iter()
            .find(|(stored, _)| stored == value)
            .map_or_else(|| value.to_string(), |(_, label)| label)
    }
}

/// An optional field offers its type's options; an empty submission reads as `None`.
impl<T: Options> Options for Option<T> {
    fn options() -> Vec<(String, String)> {
        T::options()
    }
}

#[cfg(test)]
mod tests;
