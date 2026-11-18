use super::*;

/// A row the next widget does not fit in grows its last widget over the columns it leaves empty.
#[test]
fn a_widget_alone_on_its_row_fills_it() {
    let (half, full) = (Span::Columns(1), Span::Full);
    assert_eq!(
        filled(2, &[full, half, full]),
        [full, full, full],
        "a half between two fulls takes the row"
    );
    assert_eq!(filled(2, &[half, half, half]), [half, half, full]);
    assert_eq!(
        filled(3, &[half, half, full, Span::Columns(2)]),
        [half, Span::Columns(2), full, full],
        "a row of three left one short grows its last widget by one"
    );
    assert_eq!(filled(2, &[]), []);
}

#[derive(Debug, Clone, toasty::Model)]
struct Order {
    #[key]
    #[auto]
    id: uuid::Uuid,
    created_at: Timestamp,
}

/// Each week counts the records created in the seven days before the next week starts, oldest
/// first; a record older than the first week or newer than now falls in none.
#[tokio::test]
async fn weekly_counts_bucket_records_by_the_week_they_were_created_in() {
    let mut db = crate::test_support::memory_db(toasty::models!(Order)).await;
    let now = Timestamp::now();
    for days in [-1, 1, 2, 8, 30] {
        toasty::create!(Order {
            created_at: now - SignedDuration::from_hours(24 * days),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let counts = weekly_counts(
        || Ok(Query::all()),
        Order::fields().created_at(),
        3,
        now,
        &mut db,
    )
    .await
    .unwrap();
    assert_eq!(counts, [0.0, 1.0, 2.0]);
}
