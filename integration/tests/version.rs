//! A `#[version]` model through the edit form: a save another save overtook re-renders the form
//! and writes nothing, and saving that re-rendered form overwrites.

use tablo::{
    Ability, DeclarationErrorKind, Resource, ResourceDef, Table, TextColumn, lens,
    testing::{input_value, stale_form},
};
use toasty::Db;
use topcoat::context::Cx;
use uuid::Uuid;

use crate::common::{TestClient, body_string, mount, panel, panel_router, refusal, test_db};

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

    let response = client.submit(&url, "title=mine&record_version=v2").await;
    assert_eq!(response.status(), 400);

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
        ) -> Result<Note, tablo::WriteError<NoteFormField>> {
            // Another save, after the load: `record` now holds the version it replaced.
            record
                .clone()
                .update()
                .title("theirs")
                .exec(&mut *ex)
                .await?;
            Ok(tablo::write_update::<Self>(cx, record, posted, ex).await?)
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

/// A refused save renders the version it opened on, not the one its rolled-back write took, so
/// saving the refused form again lands.
#[tokio::test]
async fn a_refused_save_carries_the_version_it_opened_on() {
    struct RefusingResource;

    impl Resource for RefusingResource {
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
        ) -> Result<Note, tablo::WriteError<NoteFormField>> {
            let refused = posted.title == "refused";
            let written = tablo::write_update::<Self>(cx, record, posted, ex).await?;
            if refused {
                return Err(tablo::WriteError::field(
                    NoteFormField::Title,
                    "Not that one",
                ));
            }
            Ok(written)
        }
    }

    let db = test_db(toasty::models!(Note)).await;
    let router = panel_router::<RefusingResource>(db.clone());
    let client = TestClient::new(&router);
    let record = note(&db).await;
    let url = format!("/admin/notes/{}/edit", record.id);
    let opened = record.version.to_string();

    let response = client
        .submit(&url, &format!("title=refused&record_version={opened}"))
        .await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert!(!stale_form(&html), "{html}");
    assert_eq!(input_value(&html, "record_version"), Some(opened.clone()));
    assert_eq!(title(&db, record.id).await, "first");

    let response = client
        .submit(&url, &format!("title=mine&record_version={opened}"))
        .await;
    assert!(response.status().is_redirection(), "{}", response.status());
    assert_eq!(title(&db, record.id).await, "mine");
}

/// A failed condition on a model without a version is no save overtaking this one: the edit
/// fails as any write does, without the alert.
#[tokio::test]
async fn a_failed_condition_on_an_unversioned_model_is_no_stale_form() {
    #[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
    struct Plain {
        #[key]
        #[auto]
        id: Uuid,
        title: String,
    }

    #[derive(tablo::RecordForm)]
    #[form(model = Plain)]
    struct PlainForm {
        title: String,
    }

    struct Conditional;

    impl Resource for Conditional {
        type Model = Plain;
        type Form = PlainForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("plains")
                .policy_fn(|_cx, _ability| true)
                .table(Table::new(TextColumn::new(lens!(Plain.title))))
        }

        async fn update_record(
            _cx: &Cx,
            _record: Plain,
            _posted: tablo::Posted<PlainForm>,
            _ex: &mut dyn toasty::Executor,
        ) -> Result<Plain, tablo::WriteError<PlainFormField>> {
            Err(toasty::Error::condition_failed("an app's own condition").into())
        }
    }

    let db = test_db(toasty::models!(Plain)).await;
    let router = panel_router::<Conditional>(db.clone());
    let record = toasty::create!(Plain {
        title: "first".to_string(),
    })
    .exec(&mut db.clone())
    .await
    .unwrap();
    let response = TestClient::new(&router)
        .submit(&format!("/admin/plains/{}/edit", record.id), "title=mine")
        .await;
    assert!(response.status().is_server_error(), "{}", response.status());
    assert!(!stale_form(&body_string(response).await));
}

/// The edit POST carries `record_version` beside the form's keys: a field of that name would
/// read the token, so mounting refuses it.
#[tokio::test]
async fn mounting_refuses_a_form_field_named_record_version() {
    #[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
    struct Release {
        #[key]
        #[auto]
        id: Uuid,
        record_version: String,
    }

    #[derive(tablo::RecordForm)]
    #[form(model = Release)]
    struct ReleaseForm {
        record_version: String,
    }

    struct Releases;

    impl Resource for Releases {
        type Model = Release;
        type Form = ReleaseForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().table(Table::new(TextColumn::new(lens!(Release.record_version))))
        }
    }

    let db = test_db(toasty::models!(Release)).await;
    let errors = refusal(mount(db, panel().resource::<Releases>()));
    assert!(
        errors.iter().any(|error| error.kind
            == DeclarationErrorKind::ReservedFormField {
                field: "record_version".to_string(),
            }),
        "{errors:?}"
    );
}
