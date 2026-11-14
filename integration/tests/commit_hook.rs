use tablo::{
    Allow, Change, CommitHook, PanelChange, Resource, ResourceDef, Table, TextColumn, lens,
};
use toasty::Db;
use topcoat::context::Cx;
use uuid::Uuid;

use crate::common::{get, mount, panel, post_fields, test_db};

#[derive(Debug, toasty::Model, tablo::PanelModel, Clone)]
struct Memo {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    #[columns(skip)]
    #[default("hidden".to_string())]
    secret: String,
}

#[derive(Debug, toasty::Model, tablo::PanelModel, Clone)]
struct Label {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
}

/// One call of the panel's hook: the resource, the kind of write and the first record it names.
#[derive(Debug, toasty::Model, Clone)]
struct Trail {
    #[key]
    #[auto]
    id: Uuid,
    slug: String,
    label: String,
    kind: String,
    record_key: String,
    record_url: Option<String>,
    /// The fields the last record names, `name:before>after` joined by `;`.
    fields: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Memo)]
struct MemoForm {
    title: String,
}

struct MemoResource;

impl Resource for MemoResource {
    type Model = Memo;
    type Form = MemoForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("memos")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Memo.title))))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Label)]
struct LabelForm {
    name: String,
}

struct LabelResource;

impl Resource for LabelResource {
    type Model = Label;
    type Form = LabelForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("labels")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Label.name))))
    }
}

/// Writes a trail row for every write, whichever resource made it.
struct Audit;

impl CommitHook for Audit {
    async fn before_commit(
        cx: &Cx,
        change: &PanelChange,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        let kind = match change.change() {
            Change::Create(_) => "create",
            Change::Update { .. } => "update",
            Change::Delete(_) => "delete",
            Change::Export { rows } => &format!("export {rows}"),
            _ => "other",
        };
        let first = change.change().records().first();
        let fields = change
            .change()
            .records()
            .last()
            .map(|record| {
                record
                    .fields()
                    .iter()
                    .map(|field| {
                        format!(
                            "{}:{}>{}",
                            field.name(),
                            field.before().unwrap_or_default(),
                            field.after().unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(";")
            })
            .unwrap_or_default();
        toasty::create!(Trail {
            slug: change.slug().to_string(),
            label: change.label().localize(cx),
            kind: kind.to_string(),
            record_key: first
                .map(|record| record.key().to_string())
                .unwrap_or_default(),
            record_url: first.and_then(|record| record.url().map(str::to_string)),
            fields,
        })
        .exec(&mut *ex)
        .await?;
        Ok(())
    }
}

/// Refuses every write after writing its trail row.
struct Refuse;

impl CommitHook for Refuse {
    async fn before_commit(
        cx: &Cx,
        change: &PanelChange,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        Audit::before_commit(cx, change, ex).await?;
        Err(std::io::Error::other("the trail refused the write").into())
    }
}

async fn db() -> Db {
    test_db(toasty::models!(Memo, Label, Trail)).await
}

async fn trails(db: &Db) -> Vec<Trail> {
    let mut db = db.clone();
    Trail::all().exec(&mut db).await.expect("query trails")
}

/// Every resource of the panel writes through its hook, which names the resource and the record.
#[tokio::test]
async fn every_resource_writes_through_the_panels_hook() {
    let db = db().await;
    let router = mount(
        db.clone(),
        panel()
            .resource::<MemoResource>()
            .resource::<LabelResource>()
            .before_commit::<Audit>(),
    )
    .expect("panel builds");
    let label = {
        let mut db = db.clone();
        toasty::create!(Label {
            name: "Urgent".to_string(),
        })
        .exec(&mut db)
        .await
        .expect("seed label")
    };

    let response = post_fields(&router, "/admin/memos/-/create", &[("title", "Alpha")]).await;
    assert_eq!(response.status(), 303, "a valid create redirects");
    let response = post_fields(
        &router,
        &format!("/admin/labels/{}/delete", label.id),
        &[("confirm", "1")],
    )
    .await;
    assert_eq!(response.status(), 303, "a confirmed delete redirects");

    let memo = {
        let mut db = db.clone();
        Memo::all().exec(&mut db).await.expect("query memos")
    };
    let trails = trails(&db).await;
    assert_eq!(trails.len(), 2, "two writes, two calls: {trails:?}");
    let created = trails
        .iter()
        .find(|t| t.kind == "create")
        .expect("the create");
    assert_eq!(created.slug, "memos");
    assert_eq!(created.label, "Memo");
    assert_eq!(created.record_key, memo[0].id.to_string());
    assert_eq!(
        created.record_url.as_deref(),
        Some(format!("/admin/memos/{}", memo[0].id).as_str())
    );
    let deleted = trails
        .iter()
        .find(|t| t.kind == "delete")
        .expect("the delete");
    assert_eq!(deleted.slug, "labels");
    assert_eq!(deleted.record_key, label.id.to_string());
}

/// An error from the panel's hook rolls the write back with every row the hook wrote.
#[tokio::test]
async fn a_hook_error_rolls_the_write_back() {
    let db = db().await;
    let router = mount(
        db.clone(),
        panel().resource::<MemoResource>().before_commit::<Refuse>(),
    )
    .expect("panel builds");

    let response = post_fields(&router, "/admin/memos/-/create", &[("title", "Alpha")]).await;
    assert!(
        response.status().is_server_error(),
        "a refusing hook fails the write, got {}",
        response.status()
    );
    let memos = {
        let mut db = db.clone();
        Memo::all().exec(&mut db).await.expect("query memos")
    };
    assert!(memos.is_empty(), "the write rolled back");
    assert!(trails(&db).await.is_empty(), "so did the hook's row");
}

/// A CSV export reaches the hook with the rows it hands over, and a hook that refuses stops the
/// export before it delivers a row.
#[tokio::test]
async fn an_export_reaches_the_hook_which_may_refuse_it() {
    let db = db().await;
    let recording = mount(
        db.clone(),
        panel().resource::<MemoResource>().before_commit::<Audit>(),
    )
    .expect("panel builds");
    post_fields(&recording, "/admin/memos/-/create", &[("title", "Alpha")]).await;

    let exported = get(&recording, "/admin/memos/-/export").await;
    assert_eq!(exported.status(), 200);
    let kinds: Vec<String> = trails(&db).await.into_iter().map(|t| t.kind).collect();
    assert!(kinds.contains(&"export 1".to_string()), "{kinds:?}");

    let refusing = mount(
        db.clone(),
        panel().resource::<MemoResource>().before_commit::<Refuse>(),
    )
    .expect("panel builds");
    let before = trails(&db).await.len();
    let refused = get(&refusing, "/admin/memos/-/export").await;
    assert!(refused.status().is_server_error(), "{}", refused.status());
    assert_eq!(
        trails(&db).await.len(),
        before,
        "the hook's row rolled back"
    );
}

/// The hook reads the fields each write changed, by the model's columns: what a create filled, what
/// an update changed and what a delete removed, never a field the columns skip.
#[tokio::test]
async fn the_hook_reads_the_fields_each_write_changed() {
    let db = db().await;
    let router = mount(
        db.clone(),
        panel().resource::<MemoResource>().before_commit::<Audit>(),
    )
    .expect("panel builds");
    let response = post_fields(&router, "/admin/memos/-/create", &[("title", "Alpha")]).await;
    assert_eq!(response.status(), 303, "a valid create redirects");
    let id = {
        let mut db = db.clone();
        Memo::all().exec(&mut db).await.expect("query memos")[0].id
    };
    let response = post_fields(
        &router,
        &format!("/admin/memos/{id}/edit"),
        &[("title", "Beta")],
    )
    .await;
    assert_eq!(response.status(), 303, "a valid edit redirects");
    let response = post_fields(
        &router,
        &format!("/admin/memos/{id}/edit"),
        &[("title", "Beta")],
    )
    .await;
    assert_eq!(response.status(), 303, "an edit changing nothing saves");
    let response = post_fields(
        &router,
        &format!("/admin/memos/{id}/delete"),
        &[("confirm", "1")],
    )
    .await;
    assert_eq!(response.status(), 303, "a confirmed delete redirects");

    let fields = |kind: &str, trails: &[Trail]| -> Vec<String> {
        trails
            .iter()
            .filter(|trail| trail.kind == kind)
            .map(|trail| trail.fields.clone())
            .collect()
    };
    let trails = trails(&db).await;
    assert_eq!(fields("create", &trails), ["title:>Alpha"]);
    let mut updates = fields("update", &trails);
    updates.sort();
    assert_eq!(
        updates,
        ["", "title:Alpha>Beta"],
        "an edit changing nothing names no field"
    );
    assert_eq!(fields("delete", &trails), ["title:Beta>"]);
}
