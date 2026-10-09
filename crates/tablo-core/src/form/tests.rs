use super::*;

proptest::proptest! {
    /// A multiple choice's values read back as written, whatever they hold: a key may carry a
    /// comma, a quote or a bracket.
    #[test]
    fn a_list_reads_back_as_written(items in proptest::collection::vec("\\PC*", 0..8)) {
        proptest::prop_assert_eq!(decode_list(&encode_list(&items)), Some(items));
    }
}

#[test]
fn a_blank_value_holds_no_item_and_another_shape_none() {
    assert_eq!(decode_list(""), Some(Vec::new()));
    assert_eq!(decode_list("a,b"), None);
    assert_eq!(decode_list("{\"a\":1}"), None);
}
