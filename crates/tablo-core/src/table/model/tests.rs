use super::ModelColumns;

/// A derived date column exports the stored value, so a derived table keeps a timestamp's time.
#[test]
fn a_derived_date_column_exports_the_stored_timestamp() {
    #[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
    struct Stamped {
        #[key]
        #[auto]
        id: uuid::Uuid,
        seen: jiff::Timestamp,
    }

    let cx = crate::test_support::cx();
    let row = Stamped {
        id: uuid::Uuid::nil(),
        seen: "2026-10-14T23:30:00Z".parse().expect("a timestamp"),
    };
    assert!(
        Stamped::table()
            .csv_row(&cx, &row)
            .contains("2026-10-14T23:30:00Z")
    );
}
