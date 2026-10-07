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
/// Field::choice(Post::fields().status()).options(Status::options());
/// SelectFilter::of(Post::fields().status());
/// Table::new(TextColumn::new(lens!(Post.status))).group_by(lens!(Post.status));
/// Post::filter(Post::fields().status().eq(Status::Published));
/// ```
///
/// Toasty stores the variant under its own discriminant, `snake_case` by
/// default; the option's value is only the form's spelling. A `String` field
/// takes the same list, storing the value itself:
/// `SelectFilter::new(Post::fields().kind(), Kind::options())`.
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

#[cfg(test)]
mod tests {
    use super::Options;
    use crate::{Column as _, Filter as _, FormScalar};

    /// One list shared by the form, the filter and the column.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, crate::Options)]
    enum Status {
        Draft,
        #[option(label = "Live")]
        Published,
        #[option(value = "arch", label = "Archive")]
        Archived,
    }

    #[test]
    fn an_options_enum_shares_one_value_label_list() {
        assert_eq!(
            Status::options(),
            vec![
                ("draft".to_string(), "Draft".to_string()),
                ("published".to_string(), "Live".to_string()),
                ("arch".to_string(), "Archive".to_string()),
            ]
        );
        assert_eq!(Status::Draft.value(), "draft");
        assert_eq!(Status::Published.label(), "Live");
        assert_eq!(Status::from_value("draft"), Some(Status::Draft));
        assert_eq!(Status::from_value("arch"), Some(Status::Archived));
        assert_eq!(Status::from_value("gone"), None);
        assert_eq!(Status::label_of("published"), "Live");
        assert_eq!(
            Status::label_of("renamed"),
            "renamed",
            "a value no option has reads as itself"
        );
    }

    #[test]
    fn an_options_enum_posts_its_value_and_reads_as_its_label() {
        assert_eq!(Status::parse_form("arch"), Ok(Status::Archived));
        assert!(
            Status::parse_form("archived").is_err(),
            "the form spells a variant by its option value, not its name"
        );
        assert_eq!(Status::Archived.to_form(), "arch");
        assert_eq!(Status::Archived.to_label(), "Archive");
    }

    #[tokio::test]
    async fn a_typed_field_takes_its_options_and_labels_from_its_type() {
        let schema = crate::Schema::new(
            crate::Field::choice(StatusField::fields().status()).options(Status::options()),
        );
        let mut valid = std::collections::HashMap::new();
        valid.insert("status".to_string(), "arch".to_string());
        let cx = topcoat::context::CxTestBuilder::new().build();
        let errors = schema.checked(&cx, &valid).await;
        assert!(errors.is_empty(), "a listed option validates: {errors:?}");
        let mut bogus = std::collections::HashMap::new();
        bogus.insert("status".to_string(), "gone".to_string());
        assert!(
            !schema.checked(&cx, &bogus).await.is_empty(),
            "an unlisted value fails"
        );
        let filter = crate::SelectFilter::of(StatusField::fields().status());
        assert_eq!(filter.options(), Status::options().as_slice());
        assert!(filter.to_expr("arch").is_some());
        assert!(
            filter.to_expr("gone").is_none(),
            "an unlisted value selects nothing"
        );
        let column = crate::TextColumn::new(crate::lens!(StatusField.status));
        let row = StatusField {
            id: uuid::Uuid::nil(),
            status: Status::Published,
        };
        assert_eq!(column.text(&row), "Live");
    }

    #[derive(Debug, Clone, toasty::Model)]
    struct StatusField {
        #[key]
        #[auto]
        id: uuid::Uuid,
        status: Status,
    }
}
