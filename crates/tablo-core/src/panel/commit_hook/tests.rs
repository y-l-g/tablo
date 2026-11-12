use super::changed_fields;

#[derive(Debug, Clone, toasty::Model, crate::PanelModel)]
struct Owner {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
}

#[derive(Debug, Clone, toasty::Model, crate::PanelModel)]
struct Ticket {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    #[index]
    owner_id: uuid::Uuid,
    #[belongs_to(key = owner_id, references = id)]
    owner: toasty::Deferred<Owner>,
}

/// Moving a record to another owner is a change, though the default table shows no foreign key.
#[test]
fn a_foreign_key_the_table_leaves_out_is_still_a_change() {
    let cx = crate::test_support::cx();
    let (alice, bob) = (uuid::Uuid::from_u128(1), uuid::Uuid::from_u128(2));
    let ticket = |owner_id| Ticket {
        id: uuid::Uuid::nil(),
        title: "Leak".to_owned(),
        owner_id,
        owner: Default::default(),
    };
    let changed = changed_fields(&cx, Some(&ticket(alice)), Some(&ticket(bob)));
    let named: Vec<_> = changed
        .iter()
        .map(|field| (field.name(), field.before(), field.after()))
        .collect();
    let (alice_key, bob_key) = (alice.to_string(), bob.to_string());
    assert_eq!(
        named,
        [("owner_id", Some(alice_key.as_str()), Some(bob_key.as_str()))]
    );
    assert!(changed_fields(&cx, Some(&ticket(alice)), Some(&ticket(alice))).is_empty());
}
