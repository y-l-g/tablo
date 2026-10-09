use super::*;

/// The refusal `source` produces, or the expansion when none fires.
fn refusal(source: &str) -> String {
    let input: DeriveInput = syn::parse_str(source).expect("the derive input parses");
    match expand_checked(input) {
        Ok(_) => String::from("<expanded>"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn a_generic_form_is_refused() {
    let message = refusal("#[form(model = M)] struct F<T> { a: T }");
    assert!(message.contains("generic"), "{message}");
}

#[test]
fn a_tuple_struct_or_an_empty_struct_is_refused() {
    let message = refusal("#[form(model = M)] struct F(String);");
    assert!(message.contains("named fields"), "{message}");
    let message = refusal("#[form(model = M)] struct F {}");
    assert!(message.contains("at least one field"), "{message}");
}

#[test]
fn a_missing_model_is_refused() {
    let message = refusal("struct F { a: String }");
    assert!(message.contains("model = <Model>"), "{message}");
}

#[test]
fn a_relation_field_is_refused() {
    let message = refusal("#[form(model = M)] struct F { author: Deferred<Author> }");
    assert!(message.contains("foreign key"), "{message}");
}

#[test]
fn blank_on_an_option_or_an_embed_is_refused() {
    let message = refusal("#[form(model = M)] struct F { #[form(blank = None)] a: Option<i64> }");
    assert!(message.contains("`None`"), "{message}");
    let message = refusal("#[form(model = M)] struct F { #[form(embed, blank = 1)] a: Seo }");
    assert!(message.contains("embedded value"), "{message}");
}

#[test]
fn a_blank_answer_on_a_many_to_many_field_is_refused() {
    let message = refusal(
        "#[form(model = M)] struct F { #[form(relationship = Tags, blank = Vec::new())] tags: Vec<Uuid> }",
    );
    assert!(message.contains("many-to-many"), "{message}");
}

#[test]
fn an_unknown_key_is_refused() {
    let message = refusal("#[form(model = M)] struct F { #[form(blnk = 1)] a: i64 }");
    assert!(message.contains("unknown"), "{message}");
    let message = refusal("#[form(model = M, tenant)] struct F { a: i64 }");
    assert!(message.contains("unknown"), "{message}");
}

#[test]
fn field_variants_are_pascal_case() {
    assert_eq!(pascal_case("author_id"), "AuthorId");
    assert_eq!(pascal_case("r#type"), "Type");
    assert_eq!(pascal_case("seo"), "Seo");
}
