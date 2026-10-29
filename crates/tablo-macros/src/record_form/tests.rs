use super::{variant::pascal_case, *};

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
fn a_blank_answer_on_a_multiple_choice_is_refused() {
    let message = refusal(
        "#[form(model = M)] struct F { #[form(options = Channel, blank = Vec::new())] a: Vec<String> }",
    );
    assert!(message.contains("min_items"), "{message}");
}

#[test]
fn tags_are_a_list_of_strings_and_their_own_control() {
    let message = refusal("#[form(model = M)] struct F { #[form(tags)] a: Vec<i64> }");
    assert!(message.contains("Vec<String>"), "{message}");
    let message = refusal("#[form(model = M)] struct F { #[form(tags, file)] a: Vec<String> }");
    assert!(message.contains("declare one"), "{message}");
    let message =
        refusal("#[form(model = M)] struct F { #[form(tags, options = Topic)] a: Vec<String> }");
    assert!(
        !message.contains("declare one"),
        "options are a tags field's suggestions: {message}"
    );
}

#[test]
fn a_blank_answer_on_a_file_list_is_refused() {
    let message =
        refusal("#[form(model = M)] struct F { #[form(file, blank = Vec::new())] a: Vec<String> }");
    assert!(message.contains("file list"), "{message}");
    let message = refusal("#[form(model = M)] struct F { #[form(file, optional)] a: Vec<String> }");
    assert!(message.contains("optional"), "{message}");
}

#[test]
fn a_file_list_of_anything_but_strings_is_refused() {
    let message = refusal("#[form(model = M)] struct F { #[form(file)] a: Vec<i64> }");
    assert!(message.contains("Vec<String>"), "{message}");
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

#[test]
fn two_fields_making_one_variant_or_a_name_making_none_are_refused() {
    for (source, expected) in [
        (
            "#[form(model = M)] struct F { foo_bar: i64, foo__bar: i64 }",
            "`foo__bar` and `foo_bar` both make the `..Field` variant `FooBar`",
        ),
        (
            "#[form(model = M)] struct F { x: i64, _x: i64 }",
            "`_x` and `x` both make the `..Field` variant `X`",
        ),
        (
            "#[form(model = M)] struct F { _1: i64 }",
            "`_1` names no `..Field` variant",
        ),
    ] {
        let message = refusal(source);
        assert!(message.contains(expected), "{source}: {message}");
    }
}
