//! Upstream behavior Tablo rests on, pinned so a dependency bump that changes it fails here, named
//! for what changed, rather than as a symptom elsewhere.

use crate::common::memory_db;

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
    let mut db = memory_db(toasty::models!(Note)).await;
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

/// SQLite's default BINARY collation treats case variants as distinct, in the unique index and in
/// an equality probe alike, so the app-side `unique()` probe agrees with the index it guards.
#[tokio::test]
async fn sqlite_unique_and_its_probe_agree_on_case() {
    let mut db = memory_db(toasty::models!(Subscriber)).await;
    for email in ["alice@example.com", "Alice@example.com"] {
        toasty::create!(Subscriber { email })
            .exec(&mut db)
            .await
            .expect("a case variant is a distinct value");
    }
    for (probe, found) in [("alice@example.com", 1), ("ALICE@EXAMPLE.COM", 0)] {
        let matches = Subscriber::all()
            .filter(Subscriber::fields().email().eq(probe))
            .exec(&mut db)
            .await
            .expect("probe");
        assert_eq!(matches.len(), found, "{probe}");
    }
}
