use super::{expand_checked, sentence_case, snake_case};

fn refusal(source: &str) -> String {
    let input: syn::DeriveInput = syn::parse_str(source).expect("the derive input parses");
    match expand_checked(&input) {
        Ok(_) => String::from("<accepted>"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn two_variants_sharing_a_value_or_a_label_are_refused() {
    let message = refusal(r#"enum S { A, #[option(value = "a")] B }"#);
    assert!(message.contains("both store"), "{message}");
    let message = refusal(r#"enum S { A, #[option(label = "A")] B }"#);
    assert!(message.contains("both read as"), "{message}");
}

#[test]
fn a_variant_stores_its_snake_case_name_and_reads_in_sentence_case() {
    assert_eq!(snake_case("Draft"), "draft");
    assert_eq!(snake_case("PublishedLate"), "published_late");
    assert_eq!(sentence_case("published_late"), "Published late");
}
