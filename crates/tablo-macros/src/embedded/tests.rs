use super::*;

/// The expansion of `source`, as the proc-macro entry point would emit it.
///
/// A unit test carries no consumer manifest, so `proc_macro_crate` cannot
/// resolve `tablo-core` and an input that passes the attribute checks
/// expands to that error instead of the impl. Only inputs the checks
/// themselves refuse produce a message to assert on; `label` and the member
/// expansions are tested directly.
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
    let krate = quote! { ::tablo_core };
    let owner: syn::Ident = syn::parse_str("Seo").unwrap();
    let (_, field) = first_field("struct Seo { r#type: String }");
    let member = &members([&field]).unwrap()[0];
    let add = build_member(&krate, &owner, member, 0, None).to_string();
    assert!(add.contains(r#"label ("Type")"#), "{add}");

    let (_, field) = first_field(r#"struct Seo { #[form(label = "Kind")] r#type: String }"#);
    let member = &members([&field]).unwrap()[0];
    let add = build_member(&krate, &owner, member, 0, None).to_string();
    assert!(add.contains(r#"label ("Kind")"#), "{add}");
}

/// A scalar of a type that is not a form scalar fails at a bound spanned on
/// the field's type: the generated read asserts `FormScalar` for it.
#[test]
fn a_scalar_carries_a_form_scalar_assertion() {
    let member = Member {
        ident: syn::parse_str("tags").unwrap(),
        ty: syn::parse_str("Vec<String>").unwrap(),
        attrs: FormAttrs::default(),
        shared: false,
    };
    let krate = quote! { ::tablo_core };
    let read = read_member(&krate, &member, 0, &quote! { None }).to_string();
    assert!(
        read.contains("assert_form_scalar :: < Vec < String > >"),
        "the read asserts the bound for the field's type, got {read}"
    );
}

/// An embedded member delegates to its own impl and asserts nothing.
#[test]
fn an_embedded_member_delegates_to_its_own_impl() {
    let member = Member {
        ident: syn::parse_str("seo").unwrap(),
        ty: syn::parse_str("Seo").unwrap(),
        attrs: FormAttrs {
            embed: true,
            ..FormAttrs::default()
        },
        shared: false,
    };
    let krate = quote! { ::tablo_core };
    let read = read_member(&krate, &member, 0, &quote! { None }).to_string();
    assert!(read.contains("read_node"), "{read}");
    assert!(!read.contains("assert_form_scalar"), "{read}");
}

/// A misspelled key is refused whatever the field's type.
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
