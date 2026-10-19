use crate::{
    DeclarationErrorKind, lens,
    table::{
        Sort, Table, TableState, TextColumn,
        tests::{Task, titles_in_order},
    },
    test_support::memory_db,
};

/// A declared default sort orders a request whose sort parameter names no sortable column, and is
/// the sort its header marks; a parameter naming one overrides it.
#[tokio::test]
async fn a_default_sort_orders_a_request_naming_no_sortable_column() {
    let mut db = memory_db(toasty::models!(Task)).await;
    for (title, status) in [("Bravo", "a"), ("Alpha", "c"), ("Charlie", "b")] {
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
    let table = Table::<Task>::new((
        TextColumn::new(lens!(Task.title)).sortable(),
        TextColumn::new(lens!(Task.status)).sortable(),
    ))
    .default_sort_desc(lens!(Task.title));
    let sorted = |sort: Sort| TableState {
        sort: Some(sort),
        ..TableState::default()
    };

    let unsorted = TableState::default();
    assert_eq!(table.sort_for(&unsorted), Some(Sort::desc("title")));
    assert_eq!(
        titles_in_order(&mut db, &table, &unsorted).await,
        ["Charlie", "Bravo", "Alpha"]
    );
    let unknown = sorted(Sort::asc("nope"));
    assert_eq!(table.sort_for(&unknown), Some(Sort::desc("title")));
    assert_eq!(
        titles_in_order(&mut db, &table, &unknown).await,
        ["Charlie", "Bravo", "Alpha"],
        "a parameter naming no sortable column reads the default, direction included"
    );
    let by_status = sorted(Sort::asc("status"));
    assert_eq!(table.sort_for(&by_status), Some(Sort::asc("status")));
    assert_eq!(
        titles_in_order(&mut db, &table, &by_status).await,
        ["Bravo", "Charlie", "Alpha"]
    );
}

/// A default sort on no sortable column is a declaration error, whether the table declares no
/// column for the field or declares one that is not sortable.
#[test]
fn a_default_sort_naming_no_sortable_column_is_misdeclared() {
    let table = || {
        Table::<Task>::new((
            TextColumn::new(lens!(Task.title)).sortable(),
            TextColumn::new(lens!(Task.status)),
        ))
    };
    for (errors, column) in [
        (
            table()
                .default_sort(lens!(Task.created_at))
                .declaration_errors(),
            "created_at",
        ),
        (
            table()
                .default_sort(lens!(Task.status))
                .declaration_errors(),
            "status",
        ),
    ] {
        assert_eq!(
            errors,
            [DeclarationErrorKind::UnsortableDefaultSort {
                column: column.to_string()
            }]
        );
    }
}
