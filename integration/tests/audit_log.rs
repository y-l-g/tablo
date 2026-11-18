//! A panel's audit log: each write stores an entry with the fields it changed, inside the write's
//! transaction, and the log lists to its reader only what the reader may read.

use tablo::{
    Ability, Action, Allow, DeclarationErrorKind, FieldAccess, Outcome, Resource, ResourceDef,
    Table, TenantId, TextColumn, WriteError,
    audit::{AuditChange, AuditEntry, AuditLog, PanelAuditExt},
    filter_keys, lens,
    testing::mount_with_handle,
};
use toasty::Db;
use topcoat::context::Cx;
use uuid::Uuid;

use crate::common::{TestClient, mount, panel, post_fields, refusal, test_db};

#[derive(Debug, toasty::Model, tablo::PanelModel, Clone)]
struct Memo {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    note: String,
    #[columns(skip)]
    #[default("hunter2".to_string())]
    secret: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Memo)]
struct MemoForm {
    title: String,
    note: String,
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

#[derive(Debug, toasty::Model, tablo::PanelModel, Clone)]
struct Vault {
    #[key]
    #[auto]
    id: Uuid,
    code: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Vault)]
struct VaultForm {
    code: String,
}

/// A resource its readers may write to and never list.
struct VaultResource;

impl Resource for VaultResource {
    type Model = Vault;
    type Form = VaultForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("vaults")
            .policy_fn(|_cx, ability: Ability<'_, Vault>| !matches!(ability, Ability::ViewAny))
            .table(Table::new(TextColumn::new(lens!(Vault.code))))
    }
}

#[derive(Debug, toasty::Model, tablo::PanelModel, Clone)]
struct Ledger {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    state: String,
}

/// A resource whose query hides the archived ledgers, and whose actions write through the model.
struct LedgerResource;

impl Resource for LedgerResource {
    type Model = Ledger;
    type Form = tablo::NoForm<Ledger>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("ledgers")
            .policy(Allow)
            .query(Ledger::fields().state().ne("archived"))
            .table(Table::new((
                TextColumn::new(lens!(Ledger.title)),
                TextColumn::new(lens!(Ledger.state)),
            )))
            .action::<Retitle>()
            .action::<Archive>()
            .action::<Touch>()
            .action::<Purge>()
    }
}

/// Sets the title.
struct Retitle;

impl Action<LedgerResource> for Retitle {
    type Input = ();
    const NAME: &'static str = "retitle";

    async fn run(
        _: &Cx,
        ledgers: &[Ledger],
        _: (),
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Outcome, WriteError<()>> {
        Ledger::filter(filter_keys(ledgers))
            .update()
            .title("Settled".to_owned())
            .exec(ex)
            .await?;
        Ok(Outcome::done())
    }
}

/// Moves the ledger out of the resource's query.
struct Archive;

impl Action<LedgerResource> for Archive {
    type Input = ();
    const NAME: &'static str = "archive";

    async fn run(
        _: &Cx,
        ledgers: &[Ledger],
        _: (),
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Outcome, WriteError<()>> {
        Ledger::filter(filter_keys(ledgers))
            .update()
            .state("archived".to_owned())
            .exec(ex)
            .await?;
        Ok(Outcome::done())
    }
}

/// Writes nothing.
struct Touch;

impl Action<LedgerResource> for Touch {
    type Input = ();
    const NAME: &'static str = "touch";

    async fn run(
        _: &Cx,
        _: &[Ledger],
        _: (),
        _: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Outcome, WriteError<()>> {
        Ok(Outcome::done())
    }
}

/// Removes the row.
struct Purge;

impl Action<LedgerResource> for Purge {
    type Input = ();
    const NAME: &'static str = "purge";

    async fn run(
        _: &Cx,
        ledgers: &[Ledger],
        _: (),
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Outcome, WriteError<()>> {
        Ledger::filter(filter_keys(ledgers))
            .delete()
            .exec(ex)
            .await?;
        Ok(Outcome::done())
    }
}

async fn db() -> Db {
    test_db(toasty::models!(Memo, Vault, Ledger, AuditEntry)).await
}

async fn entries(db: &Db) -> Vec<AuditEntry> {
    let mut db = db.clone();
    let mut entries = AuditEntry::all()
        .exec(&mut db)
        .await
        .expect("query entries");
    entries.sort_by_key(|entry| entry.created_at);
    entries
}

fn change(field: &str, before: Option<&str>, after: Option<&str>) -> AuditChange {
    AuditChange {
        field: field.to_owned(),
        label: field.to_owned(),
        before: before.map(str::to_owned),
        after: after.map(str::to_owned),
    }
}

/// Stores an entry as the hook would, on `panel` for `tenant`.
async fn seed(db: &Db, panel: &str, tenant: Option<TenantId>, changes: Vec<AuditChange>) {
    let mut db = db.clone();
    toasty::create!(AuditEntry {
        panel: panel.to_owned(),
        tenant,
        actor_id: None,
        actor: "Seeder".to_owned(),
        event: "update".to_owned(),
        resource: Some("memos".to_owned()),
        record: Some("seeded".to_owned()),
        url: None,
        changes: toasty::Json(changes),
        created_at: jiff::Timestamp::now(),
    })
    .exec(&mut db)
    .await
    .expect("seed an entry");
}

#[tokio::test]
async fn each_write_stores_an_entry_with_the_fields_it_changed() {
    let db = db().await;
    let router = mount(
        db.clone(),
        panel()
            .resource::<MemoResource>()
            .audit_log(|log| log.policy(Allow)),
    )
    .expect("panel builds");

    let response = post_fields(
        &router,
        "/admin/memos/-/create",
        &[("title", "Alpha"), ("note", "first")],
    )
    .await;
    assert_eq!(response.status(), 303, "a valid create redirects");
    let memo = {
        let mut db = db.clone();
        Memo::all()
            .exec(&mut db)
            .await
            .expect("query memos")
            .remove(0)
    };
    let response = post_fields(
        &router,
        &format!("/admin/memos/{}/edit", memo.id),
        &[("title", "Beta")],
    )
    .await;
    assert_eq!(response.status(), 303, "a valid edit redirects");
    let response = post_fields(
        &router,
        &format!("/admin/memos/{}/edit", memo.id),
        &[("title", "Beta")],
    )
    .await;
    assert_eq!(response.status(), 303, "an edit changing nothing saves");
    let response = post_fields(
        &router,
        &format!("/admin/memos/{}/delete", memo.id),
        &[("confirm", "1")],
    )
    .await;
    assert_eq!(response.status(), 303, "a confirmed delete redirects");

    let entries = entries(&db).await;
    let events: Vec<&str> = entries.iter().map(|entry| entry.event.as_str()).collect();
    assert_eq!(
        events,
        ["create", "update", "delete"],
        "an edit changing nothing stores no entry"
    );
    for entry in &entries {
        assert_eq!(entry.panel, "/admin");
        assert_eq!(entry.resource.as_deref(), Some("memos"));
        assert_eq!(entry.record, Some(memo.id.to_string()));
        assert!(
            entry
                .changes
                .0
                .iter()
                .all(|change| change.field != "secret"),
            "a skipped field is never stored: {entry:?}"
        );
    }
    let fields = |entry: &AuditEntry| -> Vec<(String, Option<String>, Option<String>)> {
        entry
            .changes
            .0
            .iter()
            .map(|change| {
                (
                    change.field.clone(),
                    change.before.clone(),
                    change.after.clone(),
                )
            })
            .collect()
    };
    assert_eq!(
        fields(&entries[1]),
        [(
            "title".to_owned(),
            Some("Alpha".to_owned()),
            Some("Beta".to_owned())
        )],
        "an update names only what it changed"
    );
    assert!(
        fields(&entries[2])
            .iter()
            .any(|(field, before, after)| field == "title"
                && before.as_deref() == Some("Beta")
                && after.is_none()),
        "a delete keeps what the record held"
    );
}

/// Each change of an entry, as `(field, before, after)`.
fn diff(entry: &AuditEntry) -> Vec<(&str, Option<&str>, Option<&str>)> {
    entry
        .changes
        .0
        .iter()
        .map(|change| {
            (
                change.field.as_str(),
                change.before.as_deref(),
                change.after.as_deref(),
            )
        })
        .collect()
}

#[tokio::test]
async fn an_action_stores_the_fields_its_run_changed() {
    let db = db().await;
    let router = mount(
        db.clone(),
        panel()
            .resource::<LedgerResource>()
            .audit_log(|log| log.policy(Allow)),
    )
    .expect("panel builds");
    let ledger = {
        let mut db = db.clone();
        toasty::create!(Ledger {
            title: "Open".to_owned(),
            state: "live".to_owned(),
        })
        .exec(&mut db)
        .await
        .expect("seed a ledger")
    };
    for action in ["retitle", "touch", "archive"] {
        let response = post_fields(
            &router,
            &format!("/admin/ledgers/{}/-/actions/{action}", ledger.id),
            &[("confirm", "1")],
        )
        .await;
        assert_eq!(response.status(), 303, "{action} runs");
    }

    let entries = entries(&db).await;
    let events: Vec<&str> = entries.iter().map(|entry| entry.event.as_str()).collect();
    assert_eq!(
        events,
        ["action:retitle", "action:touch", "action:archive"],
        "a run that changed no field still records who ran it"
    );
    assert!(
        entries
            .iter()
            .all(|entry| entry.record == Some(ledger.id.to_string())),
        "each entry names the ledger: {entries:?}"
    );
    assert_eq!(
        diff(&entries[0]),
        [("title", Some("Open"), Some("Settled"))]
    );
    assert_eq!(diff(&entries[1]), []);
    assert_eq!(
        diff(&entries[2]),
        [("state", Some("live"), Some("archived"))],
        "a run that takes the row out of the resource's query reads as an update, not a removal"
    );
}

#[tokio::test]
async fn an_action_that_removes_its_rows_stores_what_they_held() {
    let db = db().await;
    let router = mount(
        db.clone(),
        panel()
            .resource::<LedgerResource>()
            .audit_log(|log| log.policy(Allow)),
    )
    .expect("panel builds");
    let mut ids = Vec::new();
    for title in ["First", "Second"] {
        let mut db = db.clone();
        let ledger = toasty::create!(Ledger {
            title: title.to_owned(),
            state: "live".to_owned(),
        })
        .exec(&mut db)
        .await
        .expect("seed a ledger");
        ids.push(ledger.id.to_string());
    }

    let response = post_fields(
        &router,
        "/admin/ledgers/-/actions/purge",
        &[("ids", &ids.join(",")), ("confirm", "1")],
    )
    .await;
    assert_eq!(response.status(), 303, "the bulk purge runs");

    let mut entries = entries(&db).await;
    entries.sort_by(|a, b| a.record.cmp(&b.record));
    let mut expected = ids.clone();
    expected.sort();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.record.clone().unwrap_or_default())
            .collect::<Vec<_>>(),
        expected,
        "one entry per purged ledger"
    );
    for entry in &entries {
        assert_eq!(entry.event, "action:purge");
        let title = if Some(&ids[0]) == entry.record.as_ref() {
            "First"
        } else {
            "Second"
        };
        assert!(
            diff(entry).contains(&("title", Some(title), None)),
            "a removed row keeps what it held: {entry:?}"
        );
    }
}

#[tokio::test]
async fn the_log_is_denied_until_the_app_gives_it_a_policy() {
    let db = db().await;
    let (router, handle) = mount_with_handle(
        db.clone(),
        panel().resource::<MemoResource>().audit_log(|log| log),
    )
    .expect("panel builds");
    let client = TestClient::new(&router).panel(&handle);
    assert_eq!(client.resource::<AuditLog>().list().await.status(), 403);
}

#[tokio::test]
async fn the_log_lists_only_what_its_reader_may_read() {
    let db = db().await;
    let (router, handle) = mount_with_handle(
        db.clone(),
        panel()
            .resource::<MemoResource>()
            .resource::<VaultResource>()
            .audit_log(|log| log.policy(Allow)),
    )
    .expect("panel builds");
    let response = post_fields(
        &router,
        "/admin/memos/-/create",
        &[("title", "Alpha"), ("note", "first")],
    )
    .await;
    assert_eq!(response.status(), 303, "a valid create redirects");
    let response = post_fields(&router, "/admin/vaults/-/create", &[("code", "1234")]).await;
    assert_eq!(response.status(), 303, "a valid create redirects");
    seed(&db, "/elsewhere", None, Vec::new()).await;
    seed(
        &db,
        "/admin",
        Some(TenantId::new(Uuid::from_u128(7))),
        Vec::new(),
    )
    .await;
    assert_eq!(
        entries(&db).await.len(),
        4,
        "every write and seed stored one"
    );

    let client = TestClient::new(&router).panel(&handle);
    let list = client.resource::<AuditLog>().list().await;
    assert_eq!(list.status(), 200);
    let rows = list.rows();
    assert_eq!(
        rows.len(),
        1,
        "the vault's entry, another panel's and another tenant's stay out: {rows:?}"
    );
    assert!(rows[0].cells.iter().any(|cell| cell.contains("Alpha")));
    assert!(
        !list.html().contains("1234"),
        "nothing of the vault's entry reaches the page"
    );
    for entry in entries(&db).await {
        let held = entry.panel == "/admin"
            && entry.tenant.is_none()
            && entry.resource.as_deref() == Some("memos");
        let status = client.resource::<AuditLog>().view(&entry).await.status();
        assert_eq!(status == 200, held, "opened by its URL: {entry:?}");
    }
}

#[tokio::test]
async fn the_log_is_never_written_through_its_pages() {
    let db = db().await;
    let (router, handle) = mount_with_handle(
        db.clone(),
        panel()
            .resource::<MemoResource>()
            .audit_log(|log| log.policy(Allow)),
    )
    .expect("panel builds");
    seed(&db, "/admin", None, Vec::new()).await;
    let entry = entries(&db).await.remove(0);
    let client = TestClient::new(&router).panel(&handle);
    let log = client.resource::<AuditLog>();
    let deleted = log.delete(&entry).submit().await;
    assert_eq!(
        deleted.status(),
        403,
        "an allowing policy still refuses a delete"
    );
    assert_eq!(entries(&db).await.len(), 1, "the entry stays");
}

#[tokio::test]
async fn a_field_its_reader_may_not_view_reads_hidden() {
    let db = db().await;
    let (router, handle) = mount_with_handle(
        db.clone(),
        panel()
            .resource_with::<MemoResource>(|def| {
                def.field_access(lens!(Memo.note), FieldAccess::new().view(|_| false))
            })
            .audit_log(|log| log.policy(Allow)),
    )
    .expect("panel builds");
    seed(
        &db,
        "/admin",
        None,
        vec![
            change("title", Some("Alpha"), Some("Beta")),
            change("note", Some("classified"), Some("redacted")),
        ],
    )
    .await;
    let client = TestClient::new(&router).panel(&handle);
    let list = client.resource::<AuditLog>().list().await;
    let rows = list.rows();
    assert_eq!(rows.len(), 1);
    let changes = rows[0].cells.join(" ");
    assert!(
        changes.contains("Alpha") && changes.contains("Beta"),
        "{changes}"
    );
    assert!(changes.contains("Hidden"), "{changes}");
    assert!(
        !list.html().contains("classified") && !list.html().contains("redacted"),
        "a hidden field's values never reach the page"
    );
}

#[tokio::test]
async fn the_mount_refuses_a_log_whose_table_the_db_lacks() {
    let db = test_db(toasty::models!(Memo)).await;
    let errors = refusal(mount(
        db,
        panel()
            .resource::<MemoResource>()
            .audit_log(|log| log.policy(Allow)),
    ));
    assert!(
        errors.iter().any(|error| error.kind
            == DeclarationErrorKind::UnregisteredModel {
                model: "AuditEntry".to_owned()
            }),
        "{errors:?}"
    );
}

#[tokio::test]
async fn a_trail_registered_twice_records_each_write_once() {
    let db = db().await;
    let router = mount(
        db.clone(),
        panel()
            .resource::<MemoResource>()
            .audit_trail()
            .audit_log(|log| log.policy(Allow)),
    )
    .expect("panel builds");
    let response = post_fields(
        &router,
        "/admin/memos/-/create",
        &[("title", "Alpha"), ("note", "first")],
    )
    .await;
    assert_eq!(response.status(), 303, "a valid create redirects");
    assert_eq!(entries(&db).await.len(), 1, "one write, one entry");
}

#[tokio::test]
async fn the_mount_refuses_a_log_on_a_panel_without_its_trail() {
    let errors = refusal(mount(
        db().await,
        panel()
            .resource::<MemoResource>()
            .resource_with::<AuditLog>(|log| log.policy(Allow)),
    ));
    assert!(
        errors
            .iter()
            .any(|error| matches!(error.kind, DeclarationErrorKind::MissingCommitHook { .. })),
        "{errors:?}"
    );
}
