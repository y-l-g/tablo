use super::*;

/// The attributes of the first field of the struct `source` declares.
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

/// `embed` is the one classifier both derives read; nothing else marks a
/// field embedded.
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

/// Each derive reads its own keys: a key the other derive owns is as unknown
/// as a typo, so it cannot be a silent no-op.
#[test]
fn a_key_the_derive_does_not_read_is_refused() {
    let message = refusal("struct F { #[form(blank = 1)] a: i64 }", Derive::Embedded);
    assert!(message.contains("unknown"), "{message}");
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
fn the_keys_each_derive_reads_are_parsed() {
    let read = attrs(
        r#"struct F { #[form(label = "Body", multiline = 3)] body: String }"#,
        Derive::Embedded,
    )
    .unwrap();
    assert_eq!(read.label.as_deref(), Some("Body"));
    assert_eq!(read.multiline, Some(3));
    let read = attrs("struct F { #[form(blank = 0)] age: i64 }", Derive::Record).unwrap();
    assert!(read.blank.is_some());
}
