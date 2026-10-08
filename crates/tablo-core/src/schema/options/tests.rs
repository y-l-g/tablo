use super::Options;
use crate::{
    FormScalar,
    extend::{Column as _, Filter as _},
};

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
        previous: None,
    };
    assert_eq!(column.text(&crate::test_support::cx(), &row), "Live");
}

#[test]
fn an_optional_options_enum_reads_empty_as_none() {
    assert_eq!(
        Option::<Status>::parse_form("arch"),
        Ok(Some(Status::Archived))
    );
    assert_eq!(Some(Status::Archived).to_label(), "Archive");
    assert_eq!(None::<Status>.to_form(), "");
    let filter = crate::SelectFilter::of(StatusField::fields().previous());
    assert_eq!(filter.options(), Status::options().as_slice());
    assert!(filter.to_expr("arch").is_some());
    assert!(filter.misdeclared().is_none());
}

#[test]
fn a_select_option_the_field_type_does_not_parse_is_misdeclared() {
    let filter = crate::SelectFilter::new(
        StatusField::fields().status(),
        vec![
            (String::new(), "All".to_string()),
            ("draft".to_string(), "Draft".to_string()),
            ("Archive".to_string(), "Archive".to_string()),
        ],
    );
    assert_eq!(
        filter.misdeclared(),
        Some(crate::DeclarationErrorKind::UnparsedFilterOption {
            filter: "status".to_string(),
            value: "Archive".to_string(),
        })
    );
}

#[derive(Debug, Clone, toasty::Model)]
struct StatusField {
    #[key]
    #[auto]
    id: uuid::Uuid,
    status: Status,
    previous: Option<Status>,
}
