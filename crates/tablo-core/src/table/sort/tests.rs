use crate::{
    DeclarationErrorKind, TextColumn, lens,
    table::{
        Sort, Table, TableState,
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

/// A default sort on a field no sortable column shows orders the rows by it all the same, and
/// names the sort no header marks; a sort parameter naming a sortable column overrides it.
#[tokio::test]
async fn a_default_sort_orders_by_a_field_no_sortable_column_shows() {
    let mut db = memory_db(toasty::models!(Task)).await;
    for (title, status) in [("Alpha", "b"), ("Bravo", "c"), ("Charlie", "a")] {
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
        TextColumn::new(lens!(Task.status)),
    ))
    .default_sort_desc(lens!(Task.status));
    assert_eq!(table.declaration_errors(), []);

    let unsorted = TableState::default();
    assert_eq!(table.sort_for(&unsorted), Some(Sort::desc("status")));
    assert_eq!(
        titles_in_order(&mut db, &table, &unsorted).await,
        ["Bravo", "Alpha", "Charlie"]
    );
    let by_status = TableState {
        sort: Some(Sort::asc("status")),
        ..TableState::default()
    };
    assert_eq!(
        table.sort_for(&by_status),
        Some(Sort::desc("status")),
        "the URL sorts by sortable columns only"
    );
    let by_title = TableState {
        sort: Some(Sort::asc("title")),
        ..TableState::default()
    };
    assert_eq!(
        titles_in_order(&mut db, &table, &by_title).await,
        ["Alpha", "Bravo", "Charlie"]
    );
}

/// A default sort by name finds the sortable column of that name, as a column of your own needs,
/// and a name no sortable column has is a declaration error.
#[test]
fn a_default_sort_by_name_finds_its_sortable_column() {
    let table = || Table::<Task>::new(TextColumn::new(lens!(Task.title)).sortable());

    let named = table().default_sort_by(Sort::desc("title"));
    assert_eq!(named.declaration_errors(), []);
    assert_eq!(
        named.sort_for(&TableState::default()),
        Some(Sort::desc("title"))
    );
    assert_eq!(
        table()
            .default_sort_by(Sort::asc("revenue"))
            .declaration_errors(),
        [DeclarationErrorKind::UnsortableDefaultSort {
            column: "revenue".to_string()
        }]
    );
}
