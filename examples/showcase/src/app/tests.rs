use super::*;

#[test]
fn a_client_filename_is_reduced_to_a_basename() {
    assert_eq!(basename("../../etc/passwd"), "passwd");
    assert_eq!(basename("/abs/path/cover.png"), "cover.png");
    assert_eq!(basename("C:\\fakepath\\cover.png"), "cover.png");
    assert_eq!(basename("  cover.png  "), "cover.png");
    assert_eq!(basename("cover\u{7}.png"), "cover.png");
    assert_eq!(basename("   "), "");
    // Caps by bytes, keeping the tail.
    let long = format!("{}{}", "a".repeat(300), ".png");
    let capped = basename(&long);
    assert_eq!(capped.len(), MAX_BASENAME_BYTES);
    assert!(capped.ends_with(".png"), "got {capped}");
}

#[test]
fn a_multibyte_filename_caps_on_a_char_boundary() {
    let long = format!("{}.png", "é".repeat(300));
    let capped = basename(&long);
    assert!(capped.len() <= MAX_BASENAME_BYTES);
    assert!(capped.ends_with(".png"), "got {capped}");
    assert!(
        capped
            .chars()
            .all(|c| c == 'é' || c == '.' || c == 'p' || c == 'n' || c == 'g')
    );
}

#[test]
fn a_stored_name_becomes_a_url_path_segment() {
    assert_eq!(url_segment("cover.png"), "cover.png");
    assert_eq!(url_segment("cover #1.png"), "cover%20%231.png");
    assert_eq!(
        url_segment("quote%22 onerror=%22boom.png"),
        "quote%2522%20onerror%3D%2522boom.png"
    );
    assert_eq!(url_segment("100%.png"), "100%25.png");
    assert_eq!(url_segment("a+b&c.png"), "a%2Bb%26c.png");
    // No delimiter reaches the URL.
    for name in ["#", "?", "\"", " ", "%", "&", "+", "/", "\\"] {
        let encoded = url_segment(name);
        assert!(
            encoded
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"%.-_~".contains(&b)),
            "{name:?} encoded to {encoded}"
        );
    }
}
