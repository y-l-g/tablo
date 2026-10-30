//! A record fn's write under tenancy: the handler re-reads the row it returns through the
//! request's tenant scope before committing, so an override that writes outside the tenant
//! fails closed, whether it skips `write_create` or moves the row on update.

use tablo::{Allow, Resource, ResourceDef, Schema, Table, Tenancy, TenantId, TextColumn, lens};
use toasty::Db;
use topcoat::context::Cx;
use uuid::Uuid;

use crate::common::{TestClient, mount, panel, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Memo {
    #[key]
    #[auto]
    id: Uuid,
    #[index]
    tenant_id: TenantId,
    title: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Memo)]
struct MemoForm {
    title: String,
}

/// The tenant every request runs in.
const MINE: TenantId = TenantId::new(Uuid::from_u128(1));
/// The tenant the stray record fns write to.
const THEIRS: TenantId = TenantId::new(Uuid::from_u128(2));

fn declare<R: Resource<Model = Memo, Form = MemoForm>>() -> ResourceDef<R> {
    ResourceDef::new()
        .slug("memos")
        .policy(Allow)
        .tenancy(Tenancy::column(Memo::fields().tenant_id()))
        .form(Schema::new(MemoForm::controls().title))
        .table(Table::new(TextColumn::new(lens!(Memo.title))))
        .importable()
}

/// The derived record fns, which stamp and keep the request's tenant.
struct KeptResource;

impl Resource for KeptResource {
    type Model = Memo;
    type Form = MemoForm;

    fn declare() -> ResourceDef<Self> {
        declare()
    }
}

/// Record fns that write to [`THEIRS`]: a create that skips `write_create`, an update that moves
/// the row.
struct StrayResource;

impl Resource for StrayResource {
    type Model = Memo;
    type Form = MemoForm;

    fn declare() -> ResourceDef<Self> {
        declare()
    }

    async fn create_record(
        _cx: &Cx,
        form: MemoForm,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Memo, tablo::WriteError<MemoFormField>> {
        Ok(toasty::create!(Memo {
            tenant_id: THEIRS,
            title: form.title,
        })
        .exec(&mut *ex)
        .await?)
    }

    async fn update_record(
        cx: &Cx,
        record: Memo,
        posted: tablo::Posted<MemoForm>,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Memo, tablo::WriteError<MemoFormField>> {
        let mut written = tablo::write_update::<Self>(cx, record, posted, &mut *ex).await?;
        written.update().tenant_id(THEIRS).exec(&mut *ex).await?;
        Ok(written)
    }
}

async fn memos(db: &Db) -> Vec<Memo> {
    Memo::all().exec(&mut db.clone()).await.unwrap()
}

async fn seeded() -> (Db, Memo) {
    let mut db = test_db(toasty::models!(Memo)).await;
    let memo = toasty::create!(Memo {
        tenant_id: MINE,
        title: "Kept".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    (db, memo)
}

#[tokio::test]
async fn the_derived_writes_keep_the_request_tenant() {
    let (db, memo) = seeded().await;
    let router = mount(db.clone(), panel().resource::<KeptResource>()).expect("panel builds");
    let client = TestClient::new(&router).tenant(MINE);

    let created = client.submit("/admin/memos/-/create", "title=New").await;
    assert!(created.status().is_redirection(), "{}", created.status());
    let edited = client
        .submit(&format!("/admin/memos/{}/edit", memo.id), "title=Edited")
        .await;
    assert!(edited.status().is_redirection(), "{}", edited.status());

    let mut titles: Vec<_> = memos(&db)
        .await
        .into_iter()
        .inspect(|memo| assert_eq!(memo.tenant_id, MINE))
        .map(|memo| memo.title)
        .collect();
    titles.sort();
    assert_eq!(titles, ["Edited", "New"]);
}

/// A create that writes another tenant's row answers 500 and leaves nothing behind.
#[tokio::test]
async fn a_create_outside_the_tenant_rolls_back() {
    let (db, _) = seeded().await;
    let router = mount(db.clone(), panel().resource::<StrayResource>()).expect("panel builds");
    let response = TestClient::new(&router)
        .tenant(MINE)
        .submit("/admin/memos/-/create", "title=Stray")
        .await;
    assert_eq!(response.status(), 500);
    assert_eq!(memos(&db).await.len(), 1, "the stray row rolled back");
}

/// An update that moves the row to another tenant answers 500 and leaves the row as it was.
#[tokio::test]
async fn an_update_moving_the_row_out_of_the_tenant_rolls_back() {
    let (db, memo) = seeded().await;
    let router = mount(db.clone(), panel().resource::<StrayResource>()).expect("panel builds");
    let response = TestClient::new(&router)
        .tenant(MINE)
        .submit(&format!("/admin/memos/{}/edit", memo.id), "title=Moved")
        .await;
    assert_eq!(response.status(), 500);
    let [stored] = memos(&db).await.try_into().expect("one memo");
    assert_eq!((stored.tenant_id, stored.title.as_str()), (MINE, "Kept"));
}

/// An import row that writes another tenant's row fails the import, and no row of it lands.
#[tokio::test]
async fn an_import_outside_the_tenant_rolls_back() {
    let (db, _) = seeded().await;
    let router = mount(db.clone(), panel().resource::<StrayResource>()).expect("panel builds");
    let response = TestClient::new(&router)
        .tenant(MINE)
        .submit_multipart(
            "/admin/memos/-/import",
            &[("file", Some("m.csv"), "title\nOne\nTwo\n")],
        )
        .await;
    assert_eq!(response.status(), 500);
    assert_eq!(memos(&db).await.len(), 1, "no imported row landed");
}
