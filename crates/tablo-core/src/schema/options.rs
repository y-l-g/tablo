//! Typed options: one list of `(value, label)` pairs a choice field, a select
//! filter and a column share.

/// A closed set of stored values, each with its label.
///
/// Derive it on a unit-variant enum with
/// [`#[derive(Options)]`](crate::Options): each variant stores its
/// `snake_case` name and reads as its name in sentence case, and
/// `#[option(value = "..", label = "..")]` overrides either.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, status: String }
/// # use tablo_core::{Field, SelectFilter, TextColumn, lens, Options};
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, tablo_core::Options)]
/// enum Status {
///     Draft,
///     #[option(label = "Live")]
///     Published,
/// }
///
/// Field::choice(Post::fields().status()).options(Status::options());
/// SelectFilter::new(Post::fields().status(), Status::options());
/// TextColumn::new(lens!(Post.status)).format(|status| Status::label_of(status));
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

#[cfg(test)]
mod tests {
    use super::Options;
    use crate::Column as _;

    /// One list shared by the form, the filter and the column.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, crate::Options)]
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
            "a value no option stores reads as itself"
        );
    }

    #[tokio::test]
    async fn the_shared_list_feeds_a_choice_a_select_filter_and_a_column() {
        let options = Status::options();
        let schema = crate::Schema::new(
            crate::Field::choice(StatusField::fields().status()).options(options.clone()),
        );
        let mut valid = std::collections::HashMap::new();
        valid.insert("status".to_string(), "draft".to_string());
        let cx = topcoat::context::CxTestBuilder::new().build();
        let errors = schema.validate_async(&cx, &valid).await;
        assert!(errors.is_empty(), "a shared option validates: {errors:?}");
        let mut bogus = std::collections::HashMap::new();
        bogus.insert("status".to_string(), "gone".to_string());
        assert!(
            !schema.validate_async(&cx, &bogus).await.is_empty(),
            "an unlisted value fails"
        );
        let filter = crate::SelectFilter::new(StatusField::fields().status(), options.clone());
        assert_eq!(filter.options(), options.as_slice());
        let column = crate::TextColumn::new(crate::lens!(StatusField.status))
            .format(|status| Status::label_of(status));
        let row = StatusField {
            id: uuid::Uuid::nil(),
            status: "published".to_string(),
        };
        assert_eq!(column.text(&row), "Live");
    }

    #[derive(Debug, Clone, toasty::Model)]
    struct StatusField {
        #[key]
        #[auto]
        id: uuid::Uuid,
        status: String,
    }
}
