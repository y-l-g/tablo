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

#[test]
fn naming_invariants_hold() {
    // GH #136 §5 property candidates: kebab is lowercase + hyphen-only,
    // pluralize never empties.
    use super::{kebab_case, pluralize};
    for word in [
        "User", "BlogPost", "APIKey", "Category", "Box", "Person", "",
    ] {
        let kebab = kebab_case(word);
        assert!(
            kebab
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                || kebab.is_empty(),
            "kebab must be lower-hyphen, got {kebab:?} from {word:?}"
        );
        let plural = pluralize(word);
        assert!(
            word.is_empty() || !plural.is_empty(),
            "plural must not empty {word:?}"
        );
    }
    // kebab round-trips through slug vocabulary (no underscores).
    assert!(!kebab_case("Audit_Log").contains('_'));
}

/// A multi-word label keeps its head noun's rules: only the last word
/// inflects, so an irregular, uncountable or `f`-exception noun at the end of
/// a label pluralizes as it would alone.
#[test]
fn pluralize_inflects_the_last_word_of_a_label() {
    use super::pluralize;
    assert_eq!(pluralize("Sales Person"), "Sales People");
    assert_eq!(pluralize("Company News"), "Company News");
    assert_eq!(pluralize("Staff Chief"), "Staff Chiefs");
    assert_eq!(pluralize("Blog Post"), "Blog Posts");
    assert_eq!(pluralize("Product Category"), "Product Categories");
}
