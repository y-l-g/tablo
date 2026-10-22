use syn::Data;

use super::*;

/// The refusal `source` produces, or the expansion when none fires.
fn refusal(source: &str) -> String {
    let input: DeriveInput = syn::parse_str(source).expect("the derive input parses");
    match expand_checked(input) {
        Ok(_) => String::from("<expanded>"),
        Err(error) => error.to_string(),
    }
}

fn expansion(source: &str) -> String {
    let input: DeriveInput = syn::parse_str(source).expect("the derive input parses");
    let named = match &input.data {
        Data::Struct(data) => data.fields.iter().cloned().collect::<Vec<_>>(),
        _ => panic!("the source declares a struct"),
    };
    let fields: Vec<FieldSpec> = named
        .iter()
        .map(|field| FieldSpec {
            ident: field.ident.clone().unwrap(),
            ty: field.ty.clone(),
            attrs: input_attrs(field).unwrap(),
        })
        .collect();
    expand_struct(&quote! { ::tablo_core }, &input.ident, &fields).to_string()
}

#[test]
fn an_empty_struct_points_at_the_unit_input() {
    let message = refusal("struct F {}");
    assert!(message.contains("`()` asks for nothing"), "{message}");
    let message = refusal("struct F(String);");
    assert!(message.contains("named fields"), "{message}");
    let message = refusal("struct F<T> { a: T }");
    assert!(message.contains("generic"), "{message}");
}

#[test]
fn a_record_form_key_is_refused() {
    for key in ["embed", "choice", "file", "model = M"] {
        let message = refusal(&format!("struct F {{ #[form({key})] a: String }}"));
        assert!(
            message.contains("unknown `#[form(..)]` key"),
            "{key}: {message}"
        );
    }
}

#[test]
fn a_text_key_with_options_is_refused() {
    for key in [
        "multiline = 3",
        "placeholder = \"x\"",
        "email",
        "url",
        "tel",
        "password",
    ] {
        let message = refusal(&format!("struct F {{ #[form({key}, options)] a: Status }}"));
        assert!(message.contains("declare one"), "{key}: {message}");
    }
}

#[test]
fn a_text_key_on_a_bool_is_refused() {
    for key in [
        "multiline = 3",
        "placeholder = \"x\"",
        "email",
        "url",
        "tel",
        "password",
    ] {
        let message = refusal(&format!("struct F {{ #[form({key})] a: bool }}"));
        assert!(message.contains("checkbox"), "{key}: {message}");
    }
    let out = expansion("struct F { a: ::core::primitive::bool }");
    assert!(
        out.contains("toggle_input"),
        "a spelled-out bool is a checkbox: {out}"
    );
}

#[test]
fn each_field_but_a_password_writes_back_under_its_name() {
    let expanded = expansion(
        "struct Link { label: String, #[form(optional)] url: String, #[form(password)] secret: String }",
    );
    assert!(expanded.contains("FormInput for Link"), "{expanded}");
    let write = &expanded[expanded.find("fn write").expect("a write")..];
    assert!(write.contains("String :: from (\"url\")"), "{write}");
    assert!(
        !write.contains("secret"),
        "a password never writes back: {write}"
    );
}

#[test]
fn email_and_password_modify_the_text_control() {
    let out = expansion(
        "struct F { #[form(email)] a: String, #[form(password)] b: String, \
         #[form(url)] c: String, #[form(tel)] d: String }",
    );
    assert!(out.contains(". email ()"), "{out}");
    assert!(out.contains(". url ()"), "{out}");
    assert!(out.contains(". tel ()"), "{out}");
    assert!(out.contains(". password ()"), "{out}");
    assert!(out.contains("parse_password"), "{out}");
    let message = refusal("struct F { #[form(password)] a: u32 }");
    assert!(message.contains("String"), "{message}");
    let message = refusal("struct F { #[form(password, multiline = 2)] a: String }");
    assert!(message.contains("declare one"), "{message}");
    let message = refusal("struct F { #[form(email, url)] a: String }");
    assert!(message.contains("declare one"), "{message}");
}
