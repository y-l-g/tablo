#[test]
fn type_stems_drop_the_suffix_unless_nothing_is_left() {
    use super::{sentence_case, type_stem};
    struct UserResource;
    struct Resource;
    assert_eq!(type_stem::<UserResource>("Resource"), "User");
    assert_eq!(type_stem::<Resource>("Resource"), "Resource");
    assert_eq!(sentence_case("MediaLibrary"), "Media library");
    assert_eq!(sentence_case("Dashboard"), "Dashboard");
}

#[test]
fn pluralize_and_kebab_follow_english_rules() {
    use super::{kebab_case, pluralize};
    // rules
    assert_eq!(pluralize("User"), "Users");
    assert_eq!(pluralize("Category"), "Categories");
    assert_eq!(pluralize("Dummy"), "Dummies");
    assert_eq!(pluralize("Day"), "Days");
    assert_eq!(pluralize("Box"), "Boxes");
    assert_eq!(pluralize("Bus"), "Buses");
    assert_eq!(pluralize("Church"), "Churches");
    assert_eq!(pluralize("Knife"), "Knives");
    assert_eq!(pluralize("Roof"), "Roofs");
    // irregulars (case preserved)
    assert_eq!(pluralize("Person"), "People");
    assert_eq!(pluralize("Child"), "Children");
    assert_eq!(pluralize("Index"), "Indices");
    // kebab
    assert_eq!(kebab_case("Users"), "users");
    assert_eq!(kebab_case("BlogPost"), "blog-post");
    assert_eq!(kebab_case("APIKey"), "api-key");
    // The heck delegate: digit boundaries split (`User2FA` →
    // `user2-fa`) and underscores split words — pinned so a heck upgrade
    // cannot silently change slugs.
    assert_eq!(kebab_case("User2FA"), "user2-fa");
    assert_eq!(kebab_case("Blog_Post"), "blog-post");
}

proptest::proptest! {
    /// Any Rust type name makes a URL slug of lowercase words and hyphens, and a non-empty label
    /// never pluralizes to nothing.
    #[test]
    fn a_type_name_makes_a_slug_and_a_plural(word in "[A-Za-z][A-Za-z0-9_]{0,24}") {
        let kebab = super::kebab_case(&word);
        proptest::prop_assert!(
            kebab.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{kebab:?} from {word:?}"
        );
        proptest::prop_assert!(!kebab.starts_with('-') && !kebab.ends_with('-'), "{kebab:?}");
        proptest::prop_assert!(!super::pluralize(&word).is_empty());
    }
}
