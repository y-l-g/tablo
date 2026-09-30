use super::*;

/// The expansion of `source`, as the proc-macro entry point would emit it.
///
/// A unit test carries no consumer manifest, so `proc_macro_crate` cannot
/// resolve `tablo-core` and an input that passes the attribute checks
/// expands to that error instead of the impl. Only inputs the checks
/// themselves refuse produce a message to assert on; `label` and
/// `field_label` are tested directly.
fn expansion(source: &str) -> String {
    let input: DeriveInput = syn::parse_str(source).expect("the derive input parses");
    expand_tokens(input).to_string()
}

/// The first field of the struct `source` declares.
fn first_field(source: &str) -> (DeriveInput, syn::Field) {
    let input: DeriveInput = syn::parse_str(source).expect("the derive input parses");
    let field = match &input.data {
        Data::Struct(data) => data
            .fields
            .iter()
            .next()
            .expect("the struct declares a field")
            .clone(),
        _ => panic!("the source declares a struct"),
    };
    (input, field)
}

#[test]
fn a_raw_identifier_keeps_its_spelling_without_the_raw_prefix() {
    let ident: syn::Ident = syn::parse_str("r#type").expect("a raw identifier");
    assert_eq!(label(&ident), "Type");
    let ident: syn::Ident = syn::parse_str("canonical_url").expect("an identifier");
    assert_eq!(label(&ident), "Canonical Url");
}

/// The default label of a `r#type` field is `Type`: the humanizer runs on
/// the identifier's own spelling, and `#[form(label = ..)]` still wins.
#[test]
fn a_raw_identifier_field_is_labelled_without_the_raw_prefix() {
    let (_, field) = first_field("struct Seo { r#type: String }");
    let ident = field.ident.as_ref().expect("a named field");
    assert_eq!(field_label(&field, ident), "Type");

    let (_, field) = first_field(r#"struct Seo { #[form(label = "Kind")] r#type: String }"#);
    let ident = field.ident.as_ref().expect("a named field");
    assert_eq!(field_label(&field, ident), "Kind");
}

/// `textarea` renders a `Textarea`, which binds a `String` leaf: on any
/// other type the derive refuses it at the attribute rather than failing
/// inside the generated code.
#[test]
fn textarea_on_a_non_string_leaf_is_refused_at_the_attribute() {
    let error = expansion("struct Seo { #[form(textarea)] rank: i64 }");
    assert!(
        error.contains("textarea") && error.contains("`String` field"),
        "the refusal must name the attribute and the type it needs, got {error}"
    );
}

/// The same attribute on a `String` leaf passes the check: whatever else
/// the expansion emits, it is not the textarea refusal.
#[test]
fn textarea_on_a_string_leaf_passes_the_attribute_check() {
    let tokens = expansion("struct Seo { #[form(textarea)] body: String }");
    assert!(
        !tokens.contains("`String` field"),
        "a `String` textarea must not be refused, got {tokens}"
    );
}

/// The refusal fires for a payload field of an enum variant too: the check
/// walks every field the derive will bind.
#[test]
fn textarea_on_a_non_string_enum_payload_is_refused() {
    let error = expansion("enum Kind { Draft { #[form(textarea)] rank: i64 } }");
    assert!(
        error.contains("`String` field"),
        "an enum payload must be checked too, got {error}"
    );
}
