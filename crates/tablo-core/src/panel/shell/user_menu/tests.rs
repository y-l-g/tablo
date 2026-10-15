use super::initials;

/// An avatar shows the first letters of the name's first two words.
#[test]
fn initials_take_the_first_two_words() {
    assert_eq!(initials("Ada Lovelace"), "AL");
    assert_eq!(initials("grace brewster murray hopper"), "GB");
    assert_eq!(initials("Ken"), "K");
    assert_eq!(initials("  "), "");
    assert_eq!(initials("éric ölund"), "ÉÖ");
}
