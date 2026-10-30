//! A write declined for a reason that names no field: a record fn, a delete or an action answers
//! `WriteError::Declined`, the transaction rolls back what it wrote first, and the page shows the
//! message.

use tablo::{
    Ability, Action, NotificationStatus, Resource, ResourceDef, Table, TextColumn, WriteError,
    lens,
    testing::{ImportError, TestClient, body_string, import_errors, mount_with_handle},
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{import_csv, panel, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Invoice {
    #[key]
    #[auto]
    id: Uuid,
    number: String,
    status: String,
}

/// A row every write takes before it declines, so the decline has something to roll back.
#[derive(Debug, toasty::Model)]
struct Entry {
    #[key]
    #[auto]
    id: Uuid,
    number: String,
}

#[derive(Debug, Clone, tablo::RecordForm)]
#[form(model = Invoice)]
struct InvoiceForm {
    number: String,
    status: String,
}

const RESERVED: &str = "Numbers starting with X are reserved";
const FINAL: &str = "A paid invoice is final";

/// Takes an entry for `number`, then declines with `message` when `declines`.
async fn enter<K>(
    number: &str,
    declines: bool,
    message: &str,
    ex: &mut dyn toasty::Executor,
) -> Result<(), WriteError<K>> {
    toasty::create!(Entry {
        number: number.to_string()
    })
    .exec(&mut *ex)
    .await?;
    if declines {
        return Err(WriteError::declined(message.to_string()));
    }
    Ok(())
}

struct InvoiceResource;

impl Resource for InvoiceResource {
    type Model = Invoice;
    type Form = InvoiceForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("invoices")
            .policy_fn(|_cx, _ability: Ability<'_, Invoice>| true)
            .table(Table::new(TextColumn::new(lens!(Invoice.number))))
            .importable()
            .action::<Void>()
    }

    async fn create_record(
        cx: &Cx,
        form: InvoiceForm,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Invoice, WriteError<InvoiceFormField>> {
        let reserved = form.number.starts_with('X');
        enter(&form.number, reserved, RESERVED, &mut *ex).await?;
        tablo::write_create::<Self>(cx, form, ex).await
    }

    async fn update_record(
        cx: &Cx,
        record: Invoice,
        posted: tablo::Posted<InvoiceForm>,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Invoice, WriteError<InvoiceFormField>> {
        enter(&record.number, record.status == "paid", FINAL, &mut *ex).await?;
        tablo::write_update::<Self>(cx, record, posted, ex).await
    }

    async fn delete_record(
        _cx: &Cx,
        record: &Invoice,
        ex: &mut dyn toasty::Executor,
    ) -> Result<(), WriteError> {
        enter(&record.number, record.status == "paid", FINAL, &mut *ex).await?;
        Invoice::filter(Invoice::fields().id().eq(record.id))
            .delete()
            .exec(&mut *ex)
            .await?;
        Ok(())
    }
}

/// Voids the selected invoices, declining a paid one.
struct Void;

impl Action<InvoiceResource> for Void {
    type Input = ();
    const NAME: &'static str = "void";

    async fn run(
        _cx: &Cx,
        invoices: &[Invoice],
        _: (),
        ex: &mut dyn toasty::Executor,
    ) -> Result<tablo::Outcome, WriteError> {
        for invoice in invoices {
            enter(&invoice.number, invoice.status == "paid", FINAL, &mut *ex).await?;
        }
        Ok(tablo::Outcome::done())
    }
}

async fn router() -> (Db, Router, tablo::PanelHandle) {
    let db = test_db(toasty::models!(Invoice, Entry)).await;
    let (router, handle) =
        mount_with_handle(db.clone(), panel().resource::<InvoiceResource>()).expect("panel builds");
    (db, router, handle)
}

async fn invoices(db: &Db) -> Vec<Invoice> {
    Invoice::all().exec(&mut db.clone()).await.unwrap()
}

async fn entries(db: &Db) -> usize {
    Entry::all().exec(&mut db.clone()).await.unwrap().len()
}

async fn issue(db: &Db, number: &str, status: &str) -> Invoice {
    toasty::create!(Invoice {
        number: number.to_string(),
        status: status.to_string(),
    })
    .exec(&mut db.clone())
    .await
    .unwrap()
}

#[tokio::test]
async fn a_declined_create_shows_the_message_above_the_form_and_writes_nothing() {
    let (db, router, handle) = router().await;
    let client = TestClient::new(&router).panel(&handle);

    let reply = client
        .resource::<InvoiceResource>()
        .create()
        .set(InvoiceFormField::Number, "X1")
        .set(InvoiceFormField::Status, "open")
        .submit()
        .await;
    assert_eq!(reply.status(), 200);
    assert_eq!(reply.declined().as_deref(), Some(RESERVED));
    assert_eq!(
        reply.input_value(InvoiceFormField::Number).as_deref(),
        Some("X1"),
        "the form keeps what the user typed"
    );
    assert!(reply.dirty());
    assert!(invoices(&db).await.is_empty());
    assert_eq!(
        entries(&db).await,
        0,
        "the declined write's entry rolls back"
    );
}

#[tokio::test]
async fn a_declined_update_shows_the_message_and_keeps_the_record() {
    let (db, router, handle) = router().await;
    let paid = issue(&db, "A1", "paid").await;
    let client = TestClient::new(&router).panel(&handle);

    let reply = client
        .resource::<InvoiceResource>()
        .edit(&paid)
        .set(InvoiceFormField::Number, "A2")
        .submit()
        .await;
    assert_eq!(reply.status(), 200);
    assert_eq!(reply.declined().as_deref(), Some(FINAL));
    assert_eq!(invoices(&db).await[0].number, "A1");
    assert_eq!(entries(&db).await, 0);
}

#[tokio::test]
async fn a_declined_delete_flashes_the_message_and_keeps_the_record() {
    let (db, router, handle) = router().await;
    let paid = issue(&db, "A1", "paid").await;
    let open = issue(&db, "A2", "open").await;
    let client = TestClient::new(&router).panel(&handle);
    let resource = client.resource::<InvoiceResource>();

    let reply = resource.delete(&paid).submit().await;
    assert_eq!(reply.status(), 303);
    let note = reply.notification().expect("the decline flashes");
    assert_eq!(
        (note.status, note.title.as_str()),
        (NotificationStatus::Error, FINAL)
    );
    assert_eq!(invoices(&db).await.len(), 2);
    assert_eq!(entries(&db).await, 0);

    let reply = resource.delete(&open).submit().await;
    assert_eq!(reply.status(), 303, "a delete the rule passes lands");
    assert_eq!(invoices(&db).await.len(), 1);
}

#[tokio::test]
async fn a_declined_action_flashes_the_message_and_writes_nothing() {
    let (db, router, handle) = router().await;
    let paid = issue(&db, "A1", "paid").await;
    let client = TestClient::new(&router).panel(&handle);

    let reply = client
        .resource::<InvoiceResource>()
        .action::<Void>(&paid)
        .submit()
        .await;
    assert_eq!(reply.status(), 303);
    let note = reply.notification().expect("the decline flashes");
    assert_eq!(note.title, FINAL);
    assert_eq!(entries(&db).await, 0);
}

/// A declined row rolls back to its savepoint and lists against the row as a whole; the file
/// writes nothing.
#[tokio::test]
async fn an_import_lists_the_rows_a_record_fn_declines() {
    let (db, router, _) = router().await;

    let csv = "number,status\nA1,open\nX2,open\n";
    let response = import_csv(&router, "/admin/invoices/-/import", csv).await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert_eq!(import_errors(&html), [ImportError { row: 3, key: None }]);
    assert!(invoices(&db).await.is_empty());
    assert_eq!(entries(&db).await, 0);
}
