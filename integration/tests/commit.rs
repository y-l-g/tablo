use tablo::{Ability, Allow, Change, Resource, ResourceDef, Table, TextColumn, lens};
use toasty::Db;
use topcoat::context::Cx;
use uuid::Uuid;

use crate::common::{panel_router, post_fields, test_db};

#[derive(Debug, toasty::Model, tablo::ModelColumns, Clone)]
struct Note {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
}

/// One hook call: which hook, what changed, the first row it names, and how many it names.
#[derive(Debug, toasty::Model, Clone)]
struct Audit {
    #[key]
    #[auto]
    id: Uuid,
    hook: String,
    change: String,
    row_key: String,
    title: String,
    before: Option<String>,
    rows: i64,
}

async fn audit(
    hook: &str,
    change: &Change<Note>,
    ex: &mut dyn toasty::Executor,
) -> topcoat::Result<()> {
    let (kind, before) = match change {
        Change::Create(_) => ("create", None),
        Change::Update { before, .. } => ("update", Some(before.title.clone())),
        Change::Delete(_) => ("delete", None),
        _ => ("other", None),
    };
    let first = change.records().first();
    toasty::create!(Audit {
        hook: hook.to_string(),
        change: kind.to_string(),
        row_key: first.map(|note| note.id.to_string()).unwrap_or_default(),
        title: first.map(|note| note.title.clone()).unwrap_or_default(),
        before,
        rows: change.records().len() as i64,
    })
    .exec(ex)
    .await?;
    Ok(())
}

/// Audits every change inside its transaction.
struct AuditedResource;

impl Resource for AuditedResource {
    type Model = Note;
    type Form = NoteForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("notes")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Note.title))).paginate(25))
    }

    async fn before_commit(
        _cx: &Cx,
        change: &Change<Note>,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        audit("before", change, ex).await
    }
}

/// A record fn that refuses every create, under hooks that would audit it.
struct FailingWriteResource;

impl Resource for FailingWriteResource {
    type Model = Note;
    type Form = NoteForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("failing-writes")
            .policy_fn(|_cx, ability| matches!(ability, Ability::ViewAny | Ability::Create))
            .table(Table::new(TextColumn::new(lens!(Note.title))))
    }

    async fn create_record(
        _cx: &Cx,
        _form: NoteForm,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Note> {
        Err(std::io::Error::other("the write refused itself").into())
    }

    async fn before_commit(
        _cx: &Cx,
        change: &Change<Note>,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        audit("before", change, ex).await
    }

    async fn after_commit(cx: &Cx, change: Change<Note>) -> topcoat::Result<()> {
        audit("after", &change, &mut tablo::db::db(cx)).await
    }
}

/// A before-commit hook that writes its audit row, then refuses the write.
struct RefusingHookResource;

impl Resource for RefusingHookResource {
    type Model = Note;
    type Form = NoteForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("refusing-hooks")
            .policy_fn(|_cx, ability| matches!(ability, Ability::ViewAny | Ability::Create))
            .table(Table::new(TextColumn::new(lens!(Note.title))))
    }

    async fn before_commit(
        _cx: &Cx,
        change: &Change<Note>,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        audit("before", change, ex).await?;
        Err(std::io::Error::other("the audit refused the write").into())
    }

    async fn after_commit(cx: &Cx, change: Change<Note>) -> topcoat::Result<()> {
        audit("after", &change, &mut tablo::db::db(cx)).await
    }
}

/// An after-commit hook that records its call, then fails.
struct FailingAfterResource;

impl Resource for FailingAfterResource {
    type Model = Note;
    type Form = NoteForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("failing-afters")
            .policy_fn(|_cx, ability| matches!(ability, Ability::ViewAny | Ability::Create))
            .table(Table::new(TextColumn::new(lens!(Note.title))))
    }

    async fn after_commit(cx: &Cx, change: Change<Note>) -> topcoat::Result<()> {
        audit("after", &change, &mut tablo::db::db(cx)).await?;
        Err(std::io::Error::other("the webhook is down").into())
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Note)]
struct NoteForm {
    title: String,
}

async fn seeded_db() -> Db {
    test_db(toasty::models!(Note, Audit)).await
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
async fn a_create_audits_the_row_it_wrote_exactly_once() {
    let db = seeded_db().await;
    let router = panel_router::<AuditedResource>(db.clone());

    let response = post_fields(&router, "/admin/notes/-/create", &[("title", "Alpha")]).await;
    assert_eq!(response.status(), 303, "a valid create redirects");

    let created = notes(&db).await;
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].title, "Alpha");

    let audits = audits(&db).await;
    assert_eq!(audits.len(), 1, "one write is one hook call");
    assert_eq!(audits[0].change, "create");
    assert_eq!(audits[0].row_key, created[0].id.to_string());
    assert_eq!(audits[0].rows, 1);
}

#[tokio::test]
async fn an_edit_audits_both_sides_and_a_delete_the_removed_row() {
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
    assert_eq!(after_edit[0].change, "update");
    assert_eq!(after_edit[0].row_key, note.id.to_string());
    assert_eq!(after_edit[0].title, "Beta", "the row as the update left it");
    assert_eq!(
        after_edit[0].before.as_deref(),
        Some("Alpha"),
        "and as the handler loaded it"
    );

    let response = post_fields(
        &router,
        &format!("/admin/notes/{}/delete", note.id),
        &[("confirm", "1")],
    )
    .await;
    assert_eq!(response.status(), 303, "a confirmed delete redirects");
    assert!(notes(&db).await.is_empty());

    let audits = audits(&db).await;
    let deleted: Vec<_> = audits.iter().filter(|a| a.change == "delete").collect();
    assert_eq!(audits.len(), 2, "two writes, two calls");
    assert_eq!(deleted.len(), 1);
    assert_eq!(deleted[0].row_key, note.id.to_string());
    assert_eq!(
        deleted[0].title, "Beta",
        "the row as it stood before the delete"
    );
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
        "/admin/notes/-/bulk-delete",
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
    assert_eq!(audits[0].change, "delete");
    assert_eq!(audits[0].rows, 2, "both rows travel in the one call");
}

#[tokio::test]
async fn a_refused_submit_never_reaches_the_hooks() {
    let db = seeded_db().await;
    let router = panel_router::<AuditedResource>(db.clone());

    let response = post_fields(&router, "/admin/notes/-/create", &[("title", "")]).await;
    assert_eq!(response.status(), 200, "the form re-renders");

    assert!(notes(&db).await.is_empty());
    assert!(audits(&db).await.is_empty(), "nothing was written to audit");
}

#[tokio::test]
async fn a_failed_write_never_reaches_the_hooks() {
    let db = seeded_db().await;
    let router = panel_router::<FailingWriteResource>(db.clone());

    let response = post_fields(
        &router,
        "/admin/failing-writes/-/create",
        &[("title", "Alpha")],
    )
    .await;
    assert!(
        response.status().is_server_error(),
        "a record fn that refuses is an error, got {}",
        response.status()
    );

    assert!(notes(&db).await.is_empty());
    assert!(audits(&db).await.is_empty(), "neither hook ran");
}

#[tokio::test]
async fn a_failing_before_commit_rolls_back_the_write_and_its_own_rows() {
    let db = seeded_db().await;
    let router = panel_router::<RefusingHookResource>(db.clone());

    let response = post_fields(
        &router,
        "/admin/refusing-hooks/-/create",
        &[("title", "Alpha")],
    )
    .await;
    assert!(
        response.status().is_server_error(),
        "a refusing hook fails the write as a record fn does, got {}",
        response.status()
    );

    assert!(notes(&db).await.is_empty(), "the write rolled back");
    assert!(
        audits(&db).await.is_empty(),
        "so did the hook's audit row, and after_commit never ran"
    );
}

#[tokio::test]
async fn a_failing_after_commit_does_not_undo_the_write() {
    let db = seeded_db().await;
    let router = panel_router::<FailingAfterResource>(db.clone());

    let response = post_fields(
        &router,
        "/admin/failing-afters/-/create",
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
    let audits = audits(&db).await;
    assert_eq!(
        audits.len(),
        1,
        "the hook ran; only its failure is swallowed"
    );
    assert_eq!(audits[0].hook, "after");
}
