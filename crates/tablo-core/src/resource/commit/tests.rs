use super::*;

#[derive(Debug, Clone, PartialEq)]
struct Row {
    id: u32,
}

#[test]
fn committed_names_the_mutation_and_its_rows() {
    let created = Committed::created(Row { id: 1 });
    assert_eq!(created.mutation(), Mutation::Create);
    assert_eq!(created.records(), [Row { id: 1 }]);

    let updated = Committed::updated(Row { id: 2 });
    assert_eq!(updated.mutation(), Mutation::Update);

    // A bulk delete is one value however many rows it took.
    let deleted = Committed::deleted(vec![Row { id: 3 }, Row { id: 4 }]);
    assert_eq!(deleted.mutation(), Mutation::Delete);
    assert_eq!(
        deleted.records(),
        [Row { id: 3 }, Row { id: 4 }],
        "the rows keep the order the handler had them"
    );
}
