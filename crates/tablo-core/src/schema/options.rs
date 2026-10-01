//! Typed options: one list of `(value, label)` pairs a choice field, a select
//! filter and a column share.

/// A closed set of stored values, each with its label.
///
/// Derive it on a unit-variant enum with
/// [`#[derive(Options)]`](crate::Options): each variant stores its
/// `snake_case` name and reads as its name in sentence case, and
/// `#[option(value = "..", label = "..")]` overrides either.
///
/// ```ignore
/// #[derive(tablo::Options)]
/// enum Status {
///     Draft,
///     #[option(label = "Live")]
///     Published,
/// }
///
/// Field::choice(Post::fields().status()).options(Status::options())
/// SelectFilter::r#for(Post::fields().status(), Status::options())
/// TextColumn::r#for(Post::fields().status(), |p| Status::label_of(&p.status))
/// ```
///
/// The derive also gives the enum `value()`, `label()` and `from_value()`, so
/// code that writes the column names a variant rather than a string.
pub trait Options {
    /// Every option as `(value, label)`, in declaration order.
    fn options() -> Vec<(String, String)>;

    /// The label of a stored `value`, or `value` itself when no option
    /// stores it: a row written before an option was renamed still reads.
    fn label_of(value: &str) -> String {
        Self::options()
            .into_iter()
            .find(|(stored, _)| stored == value)
            .map_or_else(|| value.to_string(), |(_, label)| label)
    }
}
