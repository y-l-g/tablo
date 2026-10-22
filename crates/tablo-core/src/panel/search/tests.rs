use super::{MAX_WORDS, words};

#[test]
fn a_term_splits_into_its_distinct_words() {
    assert_eq!(words("  ada   lovelace ada "), ["ada", "lovelace"]);
}

#[test]
fn a_blank_term_has_no_words() {
    assert!(words(" \t ").is_empty());
}

#[test]
fn words_past_the_cap_are_ignored() {
    let term = (0..MAX_WORDS + 3)
        .map(|n| format!("w{n}"))
        .collect::<Vec<_>>()
        .join(" ");
    let words = words(&term);
    assert_eq!(words.len(), MAX_WORDS);
    assert_eq!(words.last().map(String::as_str), Some("w7"));
}
