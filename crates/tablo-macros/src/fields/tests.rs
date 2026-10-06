use super::*;

fn attrs(source: &str, derive: Derive) -> syn::Result<FormAttrs> {
    let input: syn::DeriveInput = syn::parse_str(source).expect("the derive input parses");
    let syn::Data::Struct(data) = &input.data else {
        panic!("the source declares a struct");
    };
    let field = data
        .fields
        .iter()
        .next()
        .expect("the struct declares a field");
    form_attrs(field, derive)
}

fn refusal(source: &str, derive: Derive) -> String {
    match attrs(source, derive) {
        Ok(_) => String::from("<accepted>"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn embed_marks_an_embedded_value_in_both_derives() {
    for derive in [Derive::Embedded, Derive::Record] {
        assert!(
            attrs("struct F { #[form(embed)] seo: Seo }", derive)
                .unwrap()
                .embed
        );
        assert!(!attrs("struct F { seo: Seo }", derive).unwrap().embed);
    }
}

#[test]
fn a_key_the_derive_does_not_read_is_refused() {
    let message = refusal(
        "struct F { #[form(multiline = 3)] a: String }",
        Derive::Record,
    );
    assert!(message.contains("unknown"), "{message}");
    let message = refusal("struct F { #[form(textarea)] a: String }", Derive::Embedded);
    assert!(message.contains("unknown"), "{message}");
}

#[test]
fn a_control_key_on_an_embedded_value_is_refused() {
    let message = refusal(
        "struct F { #[form(embed, multiline = 3)] seo: Seo }",
        Derive::Embedded,
    );
    assert!(message.contains("embedded value"), "{message}");
    let message = refusal(
        "struct F { #[form(embed, blank = 1)] seo: Seo }",
        Derive::Record,
    );
    assert!(message.contains("embedded value"), "{message}");
}

#[test]
fn a_blank_answer_on_an_option_leaf_is_refused() {
    for derive in [Derive::Embedded, Derive::Record] {
        let message = refusal("struct F { #[form(blank = 0)] a: Option<i64> }", derive);
        assert!(message.contains("`None`"), "{message}");
    }
}

#[test]
fn the_keys_each_derive_reads_are_parsed() {
    let read = attrs(
        r#"struct F { #[form(label = "Body", multiline = 3)] body: String }"#,
        Derive::Embedded,
    )
    .unwrap();
    assert_eq!(read.label.as_deref(), Some("Body"));
    assert_eq!(read.multiline, Some(3));
    for derive in [Derive::Embedded, Derive::Record] {
        let read = attrs("struct F { #[form(blank = 0)] age: i64 }", derive).unwrap();
        assert!(read.blank.is_some());
    }
}

#[test]
fn optional_reads_on_a_string_and_is_refused_elsewhere() {
    for derive in [Derive::Embedded, Derive::Record] {
        assert!(
            attrs("struct F { #[form(optional)] a: String }", derive)
                .unwrap()
                .optional
        );
        let message = refusal("struct F { #[form(optional)] a: i64 }", derive);
        assert!(message.contains("`blank = <value>`"), "{message}");
        let message = refusal("struct F { #[form(optional)] a: Option<String> }", derive);
        assert!(message.contains("optional already"), "{message}");
        let message = refusal(
            r#"struct F { #[form(optional, blank = "x")] a: String }"#,
            derive,
        );
        assert!(message.contains("declare one"), "{message}");
    }
}

/// A field is required exactly when nothing answers an empty submission: its declared `blank`,
/// `optional` on a `String`, an `Option`, or a `bool`.
#[test]
fn a_field_without_a_blank_answer_is_required() {
    let required = |source: &str| {
        let input: syn::DeriveInput = syn::parse_str(source).expect("the derive input parses");
        let syn::Data::Struct(data) = &input.data else {
            panic!("the source declares a struct");
        };
        let field = data.fields.iter().next().expect("a field");
        let attrs = form_attrs(field, Derive::Record).expect("the attributes read");
        blank_answer(&field.ty, &attrs).is_none()
    };
    assert!(required("struct F { a: String }"));
    assert!(required("struct F { a: i64 }"));
    assert!(!required("struct F { #[form(optional)] a: String }"));
    assert!(!required("struct F { #[form(blank = 0)] a: i64 }"));
    assert!(!required("struct F { a: Option<i64> }"));
    assert!(!required("struct F { a: bool }"));
}
