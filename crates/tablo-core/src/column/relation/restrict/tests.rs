use super::*;

#[test]
fn a_denied_source_warns_once() {
    assert!(first_denial("restrict::tests::Source"));
    assert!(!first_denial("restrict::tests::Source"));
    assert!(first_denial("restrict::tests::Other"));
}
