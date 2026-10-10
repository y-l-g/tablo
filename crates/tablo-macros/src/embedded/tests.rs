use super::*;

fn expansion(source: &str) -> String {
    let input: DeriveInput = syn::parse_str(source).expect("the derive input parses");
    expand_tokens(input).to_string()
}

#[test]
fn a_raw_identifier_keeps_its_spelling_without_the_raw_prefix() {
    let ident: syn::Ident = syn::parse_str("r#type").expect("a raw identifier");
    assert_eq!(label(&ident), "Type");
    let ident: syn::Ident = syn::parse_str("canonical_url").expect("an identifier");
    assert_eq!(label(&ident), "Canonical url");
}

/// A scalar of a type that is not a form scalar fails at a bound spanned on
/// the field's type: the generated schema asserts `FormScalar` for it before
/// the leaf's field, whose only bound is `FormScalar` too.
#[test]
fn a_scalar_carries_a_form_scalar_assertion_first() {
    let member = Member {
        ident: syn::parse_str("tags").unwrap(),
        ty: syn::parse_str("Vec<String>").unwrap(),
        attrs: FormAttrs::default(),
        shared: false,
    };
    let krate = quote! { ::tablo_core };
    let owner: syn::Ident = syn::parse_str("Seo").unwrap();
    let add = build_member(&krate, &owner, &member, 0, None).to_string();
    let assert = add
        .find("assert_form_scalar :: < Vec < String > >")
        .unwrap_or_else(|| panic!("the schema asserts the bound, got {add}"));
    let leaf = add.find("embedded_leaf").expect("the leaf's field");
    assert!(assert < leaf, "the assertion comes first, got {add}");
}

#[test]
fn an_unknown_form_key_is_refused() {
    let error = expansion("struct Seo { #[form(textarea)] body: String }");
    assert!(error.contains("unknown `#[form(..)]` key"), "got {error}");
    let error = expansion("enum Kind { Draft { #[form(rows = 3)] body: String } }");
    assert!(
        error.contains("unknown `#[form(..)]` key"),
        "an enum payload is checked too, got {error}"
    );
}
