use crate::{
    DeclarationErrorKind, TextColumn, lens,
    table::{
        Sort, Table, TableState,
        tests::{Task, titles_in_order},
    },
    test_support::memory_db,
};

fn grouped(group_by: &str) -> TableState {
    TableState {
        group_by: Some(group_by.to_string()),
        ..TableState::default()
    }
}

/// A grouping orders the rows by its field ahead of the sort, so each group's rows run together
/// whatever the page; `?group_by=` naming no grouping leaves the sort alone.
#[tokio::test]
async fn a_grouping_orders_the_rows_by_its_field_ahead_of_the_sort() {
    let mut db = memory_db(toasty::models!(Task)).await;
    for (title, status) in [
        ("Alpha", "b"),
        ("Bravo", "a"),
        ("Charlie", "b"),
        ("Delta", "a"),
    ] {
        toasty::create!(Task {
            title,
            status,
            featured: false,
            created_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let table = Table::<Task>::new(TextColumn::new(lens!(Task.title)).sortable())
        .group_by(lens!(Task.status))
        .group_by(lens!(Task.featured));

    assert_eq!(
        titles_in_order(&mut db, &table, &grouped("status")).await,
        ["Bravo", "Delta", "Alpha", "Charlie"]
    );
    let descending = TableState {
        sort: Some(Sort::desc("title")),
        ..grouped("status")
    };
    assert_eq!(
        titles_in_order(&mut db, &table, &descending).await,
        ["Delta", "Bravo", "Charlie", "Alpha"],
        "the sort orders the rows within each group"
    );
    assert_eq!(
        titles_in_order(&mut db, &table, &grouped("title")).await,
        ["Alpha", "Bravo", "Charlie", "Delta"]
    );
    assert_eq!(
        table
            .normalize_state(&grouped("featured"))
            .group_by
            .as_deref(),
        Some("featured")
    );
    assert_eq!(table.normalize_state(&grouped("title")).group_by, None);
}

#[test]
fn two_groupings_by_one_field_are_misdeclared() {
    let table = Table::<Task>::new(TextColumn::new(lens!(Task.title)))
        .group_by(lens!(Task.status))
        .group_by(lens!(Task.status));
    assert_eq!(
        table.declaration_errors(),
        [DeclarationErrorKind::DuplicateGrouping {
            name: "status".to_string()
        }]
    );
}
