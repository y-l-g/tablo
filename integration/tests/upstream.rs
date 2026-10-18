//! Upstream behavior Tablo rests on, pinned so a dependency bump that changes it fails here, named
//! for what changed, rather than as a symptom elsewhere.

use crate::common::test_db;

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Note {
    #[key]
    #[auto]
    id: uuid::Uuid,
    body: String,
}

/// Toasty does not refuse an update that assigns nothing: its connection worker panics, and the
/// caller panics on the dropped reply. `RecordForm::into_update` answers `None` when a submission
/// names no field, so no such update is sent.
#[tokio::test]
#[should_panic(expected = "RecvError")]
async fn toasty_panics_on_an_update_with_no_assignment() {
    let mut db = test_db(toasty::models!(Note)).await;
    let mut note = toasty::create!(Note {
        body: "kept".to_string(),
    })
    .exec(&mut db)
    .await
    .expect("seed a note");
    let _ = note.update().exec(&mut db).await;
}

#[derive(Debug, toasty::Model, tablo::ModelColumns)]
struct Subscriber {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[unique]
    email: String,
}

/// The driver's default collation decides whether case variants are one value: SQLite, Turso and
/// PostgreSQL keep them apart, MySQL's merges them. The unique index and an equality probe follow
/// the same collation, so the app-side `unique()` probe agrees with the index it guards.
#[tokio::test]
async fn a_unique_index_and_its_probe_agree_on_case() {
    let mut db = test_db(toasty::models!(Subscriber)).await;
    toasty::create!(Subscriber {
        email: "alice@example.com",
    })
    .exec(&mut db)
    .await
    .expect("seed a subscriber");
    let probed = Subscriber::all()
        .filter(Subscriber::fields().email().eq("ALICE@EXAMPLE.COM"))
        .exec(&mut db)
        .await
        .expect("probe");
    let refused = toasty::create!(Subscriber {
        email: "ALICE@EXAMPLE.COM",
    })
    .exec(&mut db)
    .await
    .is_err();
    assert_eq!(
        probed.len() == 1,
        refused,
        "the probe found {} rows, the index refused: {refused}",
        probed.len()
    );
}
