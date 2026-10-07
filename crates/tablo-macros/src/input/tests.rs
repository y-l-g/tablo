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
            attrs: form_attrs(field, Derive::Input).unwrap(),
        })
        .collect();
    expand_struct(&quote! { ::tablo_core }, &input.ident, &fields).to_string()
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
fn multiline_and_options_are_refused_together() {
    let message = refusal("struct F { #[form(multiline = 3, options)] a: Status }");
    assert!(message.contains("declare one"), "{message}");
}

#[test]
fn each_field_picks_its_control_and_its_requirement() {
    let out = expansion(
        r#"struct F {
            #[form(multiline = 4, label = "Why")] reason: String,
            notify: bool,
            #[form(options)] status: Status,
            #[form(options = Status)] code: Option<String>,
            note: Option<String>,
        }"#,
    );
    assert!(
        out.contains(
            r#"required_input (:: tablo_core :: __macro :: Field :: text_input :: < String > ("reason") . multiline (4) . label ("Why"))"#
        ),
        "{out}"
    );
    assert!(
        out.contains(
            r#"Field :: from (:: tablo_core :: __macro :: Field :: toggle_input ("notify"))"#
        ),
        "{out}"
    );
    assert!(
        out.contains(r#"required_input (:: tablo_core :: __macro :: Field :: choice_input ("status") . options (< Status as"#),
        "{out}"
    );
    assert!(
        out.contains(r#"Field :: from (:: tablo_core :: __macro :: Field :: choice_input ("code") . options (< Status as"#),
        "{out}"
    );
    assert!(
        out.contains(r#"Field :: from (:: tablo_core :: __macro :: Field :: text_input :: < Option < String > > ("note"))"#),
        "{out}"
    );
}
