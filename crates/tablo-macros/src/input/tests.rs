use super::*;

/// The refusal `source` produces, or the expansion when none fires.
fn refusal(source: &str) -> String {
    let input: DeriveInput = syn::parse_str(source).expect("the derive input parses");
    match expand_checked(input, Target::Input) {
        Ok(_) => String::from("<expanded>"),
        Err(error) => error.to_string(),
    }
}

fn expansion(source: &str) -> String {
    expansion_as(source, Target::Input)
}

fn expansion_as(source: &str, target: Target) -> String {
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
            attrs: input_attrs(field, target).unwrap(),
        })
        .collect();
    expand_struct(&quote! { ::tablo_core }, &input.ident, &fields, target).to_string()
}

#[test]
fn an_empty_struct_points_at_the_unit_input() {
    let message = refusal("struct F {}");
    assert!(message.contains("type Input = ();"), "{message}");
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
    for key in ["multiline = 3", "placeholder = \"x\"", "email", "password"] {
        let message = refusal(&format!("struct F {{ #[form({key}, options)] a: Status }}"));
        assert!(message.contains("declare one"), "{key}: {message}");
    }
}

#[test]
fn a_text_key_on_a_bool_is_refused() {
    for key in ["multiline = 3", "placeholder = \"x\"", "email", "password"] {
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
fn an_item_writes_each_field_back_under_its_name() {
    let expanded = expansion_as(
        "struct Link { label: String, #[form(optional)] url: String }",
        Target::Item,
    );
    assert!(expanded.contains("RepeaterItem for Link"), "{expanded}");
    assert!(expanded.contains("fn write"), "{expanded}");
    assert!(expanded.contains("String :: from (\"url\")"), "{expanded}");
    assert!(!expansion("struct F { a: String }").contains("fn write"));

    let input: DeriveInput = syn::parse_str("struct Link {}").expect("the derive input parses");
    let message = expand_checked(input, Target::Item).unwrap_err().to_string();
    assert!(
        message.contains("RepeaterItem") && !message.contains("type Input"),
        "{message}"
    );
}

#[test]
fn email_and_password_modify_the_text_control() {
    let out = expansion("struct F { #[form(email)] a: String, #[form(password)] b: String }");
    assert!(out.contains(". email ()"), "{out}");
    assert!(out.contains(". password ()"), "{out}");
    assert!(out.contains("parse_password"), "{out}");
    let message = refusal("struct F { #[form(password)] a: u32 }");
    assert!(message.contains("String"), "{message}");
    let message = refusal("struct F { #[form(password, multiline = 2)] a: String }");
    assert!(message.contains("declare one"), "{message}");
    let input: DeriveInput =
        syn::parse_str("struct Row { #[form(password)] a: String }").expect("parses");
    let message = expand_checked(input, Target::Item).unwrap_err().to_string();
    assert!(message.contains("unknown"), "{message}");
}
