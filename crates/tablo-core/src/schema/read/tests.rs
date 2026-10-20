use super::*;
use crate::{
    schema::{Field, tests::tags},
    test_support::{DummyUser, cx},
};

fn posted(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn a_scalar_reads_its_last_value_and_an_undeclared_key_stays_unclaimed() {
    let schema = Schema::new(Field::text(DummyUser::fields().name()));
    let (values, unclaimed) = schema
        .read_unclaimed(
            &cx(),
            &posted(&[
                ("role", "admin"),
                ("name", "Ada"),
                ("confirm", "1"),
                ("name", "Grace"),
                ("role", "owner"),
            ]),
        )
        .unwrap();
    assert_eq!(values, FormValues::from_iter([("name", "Grace")]));
    assert_eq!(unclaimed, ["confirm", "role"]);
}

#[test]
fn a_field_posting_nothing_holds_nothing() {
    let schema = Schema::new((Field::text(DummyUser::fields().name()), tags()));
    assert!(
        schema
            .read(&cx(), &posted(&[("other", "x")]))
            .unwrap()
            .is_empty()
    );
}

proptest::proptest! {
    /// A multiple choice holds each value it posted once, trimmed, in the order it first posted
    /// it, and no blank: the hidden empty value it posts first drops.
    #[test]
    fn a_multiple_choice_holds_each_posted_value_once(
        items in proptest::collection::vec("[ ab]{0,3}", 0..8),
    ) {
        let pairs: Vec<(String, String)> =
            items.iter().map(|item| ("tags".to_string(), item.clone())).collect();
        let values = tags().read(&cx(), &pairs).unwrap();
        if items.is_empty() {
            proptest::prop_assert!(values.is_empty());
            return Ok(());
        }
        let held = values.list("tags").expect("a posted choice holds a list");
        let mut first_seen: Vec<&str> = Vec::new();
        for item in items.iter().map(|item| item.trim()) {
            if !item.is_empty() && !first_seen.contains(&item) {
                first_seen.push(item);
            }
        }
        proptest::prop_assert_eq!(held, first_seen.as_slice());
    }
}
