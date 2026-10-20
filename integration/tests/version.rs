//! A `#[version]` model through the edit form: a save another save overtook re-renders the form
//! and writes nothing, and saving that re-rendered form overwrites.

use tablo::{
    Ability, Resource, ResourceDef, Table, TextColumn, lens,
    testing::{input_value, stale_form},
};
use toasty::Db;
use topcoat::context::Cx;
use uuid::Uuid;

use crate::common::{TestClient, body_string, panel_router, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Note {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    #[version]
    version: u64,
}

#[derive(tablo::RecordForm)]
#[form(model = Note)]
struct NoteForm {
    title: String,
}

fn declare<R: Resource<Model = Note>>() -> ResourceDef<R> {
    ResourceDef::new()
        .slug("notes")
        .policy_fn(|_cx, ability| {
            matches!(
                ability,
                Ability::ViewAny | Ability::View(_) | Ability::Update(_)
            )
        })
        .table(Table::new(TextColumn::new(lens!(Note.title))))
}

struct NoteResource;

impl Resource for NoteResource {
    type Model = Note;
    type Form = NoteForm;

    fn declare() -> ResourceDef<Self> {
        declare()
    }
}

async fn note(db: &Db) -> Note {
    toasty::create!(Note {
        title: "first".to_string(),
    })
    .exec(&mut db.clone())
    .await
    .unwrap()
}

async fn title(db: &Db, id: Uuid) -> String {
    Note::get_by_id(&mut db.clone(), id).await.unwrap().title
}

/// The form carries the version it opened on; a save after another one refuses, keeps the other
/// save's write, and carries the version that now stands, so saving again overwrites.
#[tokio::test]
async fn a_save_another_save_overtook_is_refused_until_saved_again() {
    let db = test_db(toasty::models!(Note)).await;
    let router = panel_router::<NoteResource>(db.clone());
    let client = TestClient::new(&router);
    let mut record = note(&db).await;
    let url = format!("/admin/notes/{}/edit", record.id);
    let html = body_string(client.get(&url).await).await;
    let opened = input_value(&html, "record_version").expect("the form carries the version");
    assert_eq!(opened, record.version.to_string());

    record
        .update()
        .title("theirs")
        .exec(&mut db.clone())
        .await
        .unwrap();
    let response = client
        .submit(&url, &format!("title=mine&record_version={opened}"))
        .await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert!(stale_form(&html), "{html}");
    assert_eq!(title(&db, record.id).await, "theirs");

    let now = input_value(&html, "record_version").expect("the form carries the version");
    assert_eq!(now, record.version.to_string());
    let response = client
        .submit(&url, &format!("title=mine&record_version={now}"))
        .await;
    assert!(response.status().is_redirection(), "{}", response.status());
    assert_eq!(title(&db, record.id).await, "mine");
}

/// A save that lands between the edit's load and its write fails Toasty's version condition:
/// the form re-renders as overtaken rather than failing the request.
#[tokio::test]
async fn a_write_the_version_condition_refuses_re_renders_the_form() {
    struct RacedResource;

    impl Resource for RacedResource {
        type Model = Note;
        type Form = NoteForm;

        fn declare() -> ResourceDef<Self> {
            declare()
        }

        async fn update_record(
            cx: &Cx,
            record: Note,
            posted: tablo::Posted<NoteForm>,
            ex: &mut dyn toasty::Executor,
        ) -> topcoat::Result<Note> {
            // Another save, after the load: `record` now holds the version it replaced.
            record
                .clone()
                .update()
                .title("theirs")
                .exec(&mut *ex)
                .await?;
            tablo::write_update::<Self>(cx, record, posted, ex).await
        }
    }

    let db = test_db(toasty::models!(Note)).await;
    let router = panel_router::<RacedResource>(db.clone());
    let record = note(&db).await;
    let response = TestClient::new(&router)
        .submit(&format!("/admin/notes/{}/edit", record.id), "title=mine")
        .await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert!(stale_form(&html), "{html}");
    assert_eq!(title(&db, record.id).await, "first");
}
