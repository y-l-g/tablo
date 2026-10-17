use super::*;

#[test]
fn a_test_source_answers_to_the_test_limit() {
    assert_eq!(limit("crates/a/src/b/tests.rs"), MAX_TEST_LINES);
    assert_eq!(limit("crates/a/src/test_support.rs"), MAX_TEST_LINES);
    assert_eq!(limit("integration/tests/c.rs"), MAX_TEST_LINES);
    assert_eq!(limit("crates/a/src/contests.rs"), MAX_LINES);
}

#[test]
fn the_ratchet_refuses_growth_and_a_stale_entry() {
    let path = "crates/a/src/b.rs";
    assert!(violation(path, MAX_LINES, None).is_none());
    assert!(violation(path, MAX_LINES + 1, None).is_some());
    assert!(violation(path, 700, Some(700)).is_none());
    assert!(
        violation(path, 701, Some(700)).is_some(),
        "a capped file grew"
    );
    assert!(
        violation(path, MAX_LINES, Some(700)).is_some(),
        "a file back under its limit stays listed"
    );
}

#[test]
fn a_shrunk_file_lowers_its_cap() {
    let refused = violation("crates/a/src/b.rs", 650, Some(700)).expect("the cap stays slack");
    assert!(refused.contains("lower the cap to 650"), "{refused}");
}

#[test]
fn the_list_stays_sorted_with_each_file_once() {
    assert!(unsorted(OVERSIZED).is_none());
    assert!(unsorted(&[("a.rs", 700), ("b.rs", 700)]).is_none());
    assert!(unsorted(&[("b.rs", 700), ("a.rs", 700)]).is_some());
    assert!(unsorted(&[("a.rs", 700), ("a.rs", 800)]).is_some());
}
