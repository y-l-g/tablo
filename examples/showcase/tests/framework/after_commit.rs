use http::header::LOCATION;
use tablo_core::{
    Ability, Allow, Committed, Field, Mutation, Resource, ResourceDef, Schema, Table, TextColumn,
    lens,
};
use toasty::Db;
use topcoat::{context::Cx, router::Body};
use uuid::Uuid;

use crate::framework::common::{body_string, memory_db, panel_router, post_fields};

#[derive(Debug, toasty::Model, Clone)]
struct Note {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
}

#[derive(Debug, toasty::Model, Clone)]
struct Audit {
    #[key]
    #[auto]
    id: Uuid,
    mutation: String,
    row_key: String,
    title: String,
    rows: i64,
}

fn mutation_name(mutation: Mutation) -> &'static str {
    match mutation {
        Mutation::Create => "create",
        Mutation::Update => "update",
        Mutation::Delete => "delete",
        Mutation::Action(name) => name,
        _ => "other",
    }
}

async fn audit(cx: &Cx, committed: &Committed<Note>) -> topcoat::Result<()> {
    let first = committed.records().first();
    let mut db = tablo_core::db::db(cx);
    toasty::create!(Audit {
        mutation: mutation_name(committed.mutation()).to_string(),
        row_key: first.map(|note| note.id.to_string()).unwrap_or_default(),
        title: first.map(|note| note.title.clone()).unwrap_or_default(),
        rows: committed.records().len() as i64,
    })
    .exec(&mut db)
    .await
    .map_err(|error| -> topcoat::Error { error.into() })?;
    Ok(())
}

struct AuditedResource;

impl Resource for AuditedResource {
    type Model = Note;
    type Form = AuditedForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("notes")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Note.title))).paginate(25))
            .form(Schema::new(Field::text(Note::fields().title())))
    }

    async fn after_commit(cx: &Cx, committed: Committed<Note>) -> topcoat::Result<()> {
        audit(cx, &committed).await
    }
}
#[derive(tablo_core::RecordForm)]
#[form(model = Note)]
struct AuditedForm {
    title: String,
}
struct PlainResource;

impl Resource for PlainResource {
    type Model = Note;
    type Form = PlainForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("plain-notes")
            .policy(|_cx: &Cx, ability: Ability<'_, Note>| {
                matches!(ability, Ability::ViewAny | Ability::Create)
            })
            .table(Table::new(TextColumn::new(lens!(Note.title))))
            .form(Schema::new(Field::text(Note::fields().title())))
    }
}
#[derive(tablo_core::RecordForm)]
#[form(model = Note)]
struct PlainForm {
    title: String,
}
struct FailingWriteResource;

impl Resource for FailingWriteResource {
    type Model = Note;
    type Form = FailingWriteForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("failing-writes")
            .policy(|_cx: &Cx, ability: Ability<'_, Note>| {
                matches!(ability, Ability::ViewAny | Ability::Create)
            })
            .table(Table::new(TextColumn::new(lens!(Note.title))))
            .form(Schema::new(Field::text(Note::fields().title())))
    }

    async fn create_record(
        _cx: &Cx,
        _form: FailingWriteForm,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Note> {
        Err(std::io::Error::other("the write refused itself").into())
    }

    async fn after_commit(cx: &Cx, committed: Committed<Note>) -> topcoat::Result<()> {
        audit(cx, &committed).await
    }
}
#[derive(tablo_core::RecordForm)]
#[form(model = Note)]
struct FailingWriteForm {
    title: String,
}
struct FailingHookResource;

impl Resource for FailingHookResource {
    type Model = Note;
    type Form = FailingHookForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("failing-hooks")
            .policy(|_cx: &Cx, ability: Ability<'_, Note>| {
                matches!(ability, Ability::ViewAny | Ability::Create)
            })
            .table(Table::new(TextColumn::new(lens!(Note.title))))
            .form(Schema::new(Field::text(Note::fields().title())))
    }

    async fn after_commit(cx: &Cx, committed: Committed<Note>) -> topcoat::Result<()> {
        audit(cx, &committed).await?;
        Err(std::io::Error::other("the webhook is down").into())
    }
}
#[derive(tablo_core::RecordForm)]
#[form(model = Note)]
struct FailingHookForm {
    title: String,
}
async fn seeded_db() -> Db {
    memory_db(toasty::models!(Note, Audit)).await
}

async fn seed_note(db: &Db, title: &str) -> Note {
    let mut db = db.clone();
    toasty::create!(Note {
        title: title.to_string(),
    })
    .exec(&mut db)
    .await
    .expect("seed note")
}

async fn notes(db: &Db) -> Vec<Note> {
    let mut db = db.clone();
    Note::all().exec(&mut db).await.expect("query notes")
}

async fn audits(db: &Db) -> Vec<Audit> {
    let mut db = db.clone();
    Audit::all().exec(&mut db).await.expect("query audits")
}

#[tokio::test]
async fn a_create_audits_the_row_it_committed_exactly_once() {
    let db = seeded_db().await;
    let router = panel_router::<AuditedResource>(db.clone());

    let response = post_fields(&router, "/admin/notes/create", &[("title", "Alpha")]).await;
    assert_eq!(response.status(), 303, "a valid create redirects");

    let created = notes(&db).await;
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].title, "Alpha");

    let audits = audits(&db).await;
    assert_eq!(audits.len(), 1, "one commit is one hook call");
    assert_eq!(audits[0].mutation, "create");
    assert_eq!(audits[0].row_key, created[0].id.to_string());
    assert_eq!(audits[0].rows, 1);
}

#[tokio::test]
async fn an_edit_and_a_delete_each_audit_the_row_they_named() {
    let db = seeded_db().await;
    let router = panel_router::<AuditedResource>(db.clone());
    let note = seed_note(&db, "Alpha").await;

    let response = post_fields(
        &router,
        &format!("/admin/notes/{}/edit", note.id),
        &[("title", "Beta")],
    )
    .await;
    assert_eq!(response.status(), 303, "a valid edit redirects");
    assert_eq!(notes(&db).await[0].title, "Beta");

    let after_edit = audits(&db).await;
    assert_eq!(after_edit.len(), 1);
    assert_eq!(after_edit[0].mutation, "update");
    assert_eq!(after_edit[0].row_key, note.id.to_string());
    assert_eq!(
        after_edit[0].title, "Beta",
        "an update hands over the committed row, not the loaded snapshot"
    );

    let response = post_fields(
        &router,
        &format!("/admin/notes/{}/delete", note.id),
        &[("confirm", "1")],
    )
    .await;
    assert_eq!(response.status(), 303, "a confirmed delete redirects");
    assert!(notes(&db).await.is_empty());

    let after_delete = audits(&db).await;
    assert_eq!(after_delete.len(), 2, "two writes, two calls");
    assert_eq!(after_delete[1].mutation, "delete");
    assert_eq!(after_delete[1].row_key, note.id.to_string());
}

#[tokio::test]
async fn a_bulk_delete_is_one_call_for_the_whole_batch() {
    let db = seeded_db().await;
    let router = panel_router::<AuditedResource>(db.clone());
    let first = seed_note(&db, "Alpha").await;
    let second = seed_note(&db, "Beta").await;

    let ids = format!("{},{}", first.id, second.id);
    let response = post_fields(
        &router,
        "/admin/notes/bulk-delete",
        &[("ids", &ids), ("confirm", "1")],
    )
    .await;
    assert_eq!(response.status(), 303);
    assert!(notes(&db).await.is_empty());

    let audits = audits(&db).await;
    assert_eq!(
        audits.len(),
        1,
        "one bulk write is one hook call, not one per row"
    );
    assert_eq!(audits[0].mutation, "delete");
    assert_eq!(audits[0].rows, 2, "both rows travel in the one call");
}

#[tokio::test]
async fn a_refused_submit_never_reaches_the_hook() {
    let db = seeded_db().await;
    let router = panel_router::<AuditedResource>(db.clone());

    let response = post_fields(&router, "/admin/notes/create", &[("title", "")]).await;
    assert_eq!(response.status(), 200, "the form re-renders");

    assert!(notes(&db).await.is_empty());
    assert!(
        audits(&db).await.is_empty(),
        "a rollback must not produce the side effect"
    );
}

#[tokio::test]
async fn a_failed_write_never_reaches_the_hook() {
    let db = seeded_db().await;
    let router = panel_router::<FailingWriteResource>(db.clone());

    let response = post_fields(
        &router,
        "/admin/failing-writes/create",
        &[("title", "Alpha")],
    )
    .await;
    assert!(
        response.status().is_server_error(),
        "a record fn that refuses is an error, got {}",
        response.status()
    );

    assert!(notes(&db).await.is_empty());
    assert!(
        audits(&db).await.is_empty(),
        "the transaction rolled back, so nothing committed"
    );
}

#[tokio::test]
async fn a_failing_hook_does_not_undo_the_write() {
    let db = seeded_db().await;
    let router = panel_router::<FailingHookResource>(db.clone());

    let response = post_fields(
        &router,
        "/admin/failing-hooks/create",
        &[("title", "Alpha")],
    )
    .await;
    assert_eq!(
        response.status(),
        303,
        "the write is committed, so the response reports it"
    );

    assert_eq!(
        notes(&db).await.len(),
        1,
        "a failed side effect cannot roll back a committed write"
    );
    assert_eq!(
        audits(&db).await.len(),
        1,
        "the hook ran; only its failure is swallowed"
    );
}

#[tokio::test]
async fn a_resource_without_the_hook_writes_exactly_as_before() {
    let db = seeded_db().await;
    let router = panel_router::<PlainResource>(db.clone());

    let response = post_fields(&router, "/admin/plain-notes/create", &[("title", "Alpha")]).await;
    assert_eq!(response.status(), 303, "the default hook is a no-op");
    let response = router
        .handle(
            http::Request::builder()
                .uri("/admin/plain-notes")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await;
    assert!(response.status().is_success(), "the list still renders");
    assert!(
        response.headers().get(LOCATION).is_none(),
        "a plain GET is not a redirect"
    );
    let html = body_string(response).await;
    assert!(
        html.contains("Alpha"),
        "the list shows the created row: {html}"
    );

    assert_eq!(notes(&db).await.len(), 1);
}
