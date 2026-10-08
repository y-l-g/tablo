//! Upstream behavior Tablo rests on, pinned so a dependency bump that changes it fails here, named
//! for what changed, rather than as a symptom elsewhere.

use crate::framework::common::memory_db;

#[derive(Debug, Clone, toasty::Model)]
struct Note {
    #[key]
    #[auto]
    id: uuid::Uuid,
    body: String,
}

/// Toasty refuses an update that assigns nothing, which is why `RecordForm::into_update` answers
/// `None` when a submission names no field.
#[tokio::test]
#[should_panic(expected = "RecvError")]
async fn toasty_refuses_an_update_with_no_assignment() {
    let mut db = memory_db(toasty::models!(Note)).await;
    let mut note = toasty::create!(Note {
        body: "kept".to_string(),
    })
    .exec(&mut db)
    .await
    .expect("seed a note");
    let _ = note.update().exec(&mut db).await;
}
