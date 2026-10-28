//! The table and detail page a model's `ModelColumns` give a resource that declares neither, and
//! the detail page a declared table gives.

use http::StatusCode;
use tablo::{
    BooleanColumn, Detail, EmbeddedColumn, FileColumn, ModelColumns, NoForm, Resource, ResourceDef,
    SelectFilter, Table, TextColumn, lens,
    testing::{columns, detail_entries, filter_options, rows},
};
use toasty::Db;
use topcoat::router::Router;
use uuid::Uuid;

use crate::common::{body_string, get, input_value, panel_router, test_db};

#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo::Options)]
enum TicketStatus {
    Open,
    #[option(label = "Waiting on customer")]
    Waiting,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Ticket {
    #[key]
    #[auto]
    id: Uuid,
    subject: String,
    status: TicketStatus,
    urgent: bool,
    estimate: i64,
    /// Toasty writes it; no form does.
    #[default(0)]
    reopened: i64,
    /// Toasty writes it as the record is created.
    #[auto]
    created_at: jiff::Timestamp,
    #[columns(skip)]
    token: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Ticket)]
struct TicketForm {
    subject: String,
    #[form(options)]
    status: TicketStatus,
    urgent: bool,
    estimate: i64,
}

/// A resource naming only its model, its form and its policy.
struct TicketResource;

impl Resource for TicketResource {
    type Model = Ticket;
    type Form = TicketForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().policy(tablo::ReadOnly).edit_only()
    }
}

/// The same resource with its detail page turned off.
struct UnviewedTicketResource;

impl Resource for UnviewedTicketResource {
    type Model = Ticket;
    type Form = TicketForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(tablo::ReadOnly)
            .edit_only()
            .detail(Detail::empty())
    }
}

const TOKEN: &str = "d1ce-secret";

async fn ticket_db() -> (Db, Ticket) {
    let mut db = test_db(toasty::models!(Ticket)).await;
    let ticket = toasty::create!(Ticket {
        subject: "Printer jam".to_string(),
        status: TicketStatus::Waiting,
        urgent: true,
        estimate: 3,
        reopened: 2,
        token: TOKEN.to_string(),
    })
    .exec(&mut db)
    .await
    .expect("seed ticket");
    toasty::create!(Ticket {
        subject: "New laptop".to_string(),
        status: TicketStatus::Open,
        urgent: false,
        estimate: 8,
        token: TOKEN.to_string(),
    })
    .exec(&mut db)
    .await
    .expect("seed ticket");
    (db, ticket)
}

/// The model's columns give the table: a sortable column per field, a field no form writes
/// included, searchable over a `String`, an options enum by its label, and a toggle as yes or
/// no, an `#[auto]` timestamp included. A skipped field shows nowhere.
#[tokio::test]
async fn the_model_columns_give_the_table() {
    let (db, _) = ticket_db().await;
    let router = panel_router::<TicketResource>(db);

    let html = body_string(get(&router, "/admin/tickets?sort=reopened&dir=desc").await).await;
    for header in [
        "Subject",
        "Status",
        "Urgent",
        "Estimate",
        "Reopened",
        "Created at",
    ] {
        assert!(html.contains(header), "a {header} column: {html}");
    }
    assert!(
        !html.contains(TOKEN),
        "a skipped field shows nowhere: {html}"
    );
    assert!(
        html.contains("Waiting on customer"),
        "an options enum shows its label: {html}"
    );
    assert!(html.contains("Yes") && html.contains("No"), "{html}");
    assert!(
        html.find("Printer jam").unwrap() < html.find("New laptop").unwrap(),
        "sorted by the field no form writes, descending: {html}"
    );

    let searched = body_string(get(&router, "/admin/tickets?q=printer").await).await;
    assert!(
        searched.contains("Printer jam") && !searched.contains("New laptop"),
        "searched by the subject: {searched}"
    );
}

/// The model's columns give the detail page: each field the table lists, read-only; an empty
/// detail turns it off.
#[tokio::test]
async fn the_model_columns_give_the_detail_page() {
    let (db, ticket) = ticket_db().await;
    let viewed = panel_router::<TicketResource>(db.clone());
    let detail = get(&viewed, &format!("/admin/tickets/{}", ticket.id)).await;
    assert_eq!(detail.status(), StatusCode::OK);
    let detail = body_string(detail).await;
    for (label, value) in [
        ("Subject", "Printer jam"),
        ("Status", "Waiting on customer"),
        ("Urgent", "Yes"),
        ("Estimate", ">3<"),
        ("Reopened", ">2<"),
    ] {
        assert!(
            entry(&detail, label).contains(value),
            "{label} shows {value}: {detail}"
        );
    }
    assert!(
        !detail.contains(TOKEN),
        "a skipped field shows nowhere: {detail}"
    );
    assert_eq!(
        input_value(&detail, "subject"),
        None,
        "the detail page renders no control for the field: {detail}"
    );

    let unviewed = panel_router::<UnviewedTicketResource>(db);
    let detail = get(&unviewed, &format!("/admin/unviewed-tickets/{}", ticket.id)).await;
    assert_eq!(detail.status(), StatusCode::NOT_FOUND);
}

/// The detail page's entry labelled `label`: its markup up to the next entry.
fn entry<'h>(html: &'h str, label: &str) -> &'h str {
    let at = html
        .find(&format!(">{label}<"))
        .unwrap_or_else(|| panic!("no {label} entry: {html}"));
    let rest = &html[at..];
    &rest[..rest.find("data-slot=\"field\"").unwrap_or(rest.len())]
}

/// A delivery's stage, stored as an embedded enum.
#[derive(Debug, Clone, toasty::Embed, tablo::EmbeddedForm)]
enum Stage {
    #[column(variant = 1)]
    Draft,
    #[column(variant = 2)]
    Out { note: String },
}

/// One signature on a delivery: an item of a `#[document]` list.
#[derive(Debug, Clone, PartialEq, toasty::Embed, tablo::FormInput)]
struct Signature {
    signer: String,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Delivery {
    #[key]
    #[auto]
    id: Uuid,
    address: String,
    receipt: String,
    stage: Stage,
    #[document]
    signatures: Vec<Signature>,
}

/// A list-only resource: no form writes the model, and its columns still list and show it.
struct DeliveryResource;

impl Resource for DeliveryResource {
    type Model = Delivery;
    type Form = NoForm<Delivery>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().policy(tablo::ReadOnly)
    }
}

/// The detail page shows an embedded value leaf by leaf and a list's items, which the table
/// leaves out.
#[tokio::test]
async fn embedded_values_and_lists_show_on_the_detail_page_only() {
    let (db, delivery) = delivery_db().await;
    let router = panel_router::<DeliveryResource>(db);

    let list = body_string(get(&router, "/admin/deliveries").await).await;
    assert!(list.contains("12 Rue Haute"), "{list}");
    assert!(
        !list.contains("shipped") && !list.contains("Grace"),
        "the table leaves out embedded values and lists: {list}"
    );

    let detail =
        body_string(get(&router, &format!("/admin/deliveries/{}", delivery.id)).await).await;
    assert!(
        entry(&detail, "Address").contains("12 Rue Haute"),
        "{detail}"
    );
    assert!(entry(&detail, "Stage").contains(">Out<"), "{detail}");
    assert!(entry(&detail, "Note").contains("shipped"), "{detail}");
    let signatures = detail.find(">Signatures<").expect("a Signatures entry");
    assert!(detail[signatures..].contains(">Grace<"), "{detail}");
    assert!(
        entry(&detail, "Receipt").contains("/uploads/receipt.pdf")
            && !entry(&detail, "Receipt").contains("href="),
        "the model knows no file: a path shows as text: {detail}"
    );
}

async fn delivery_db() -> (Db, Delivery) {
    let mut db = test_db(toasty::models!(Delivery)).await;
    let delivery = toasty::create!(Delivery {
        address: "12 Rue Haute".to_string(),
        receipt: "/uploads/receipt.pdf".to_string(),
        stage: Stage::Out {
            note: "shipped".to_string(),
        },
        signatures: vec![Signature {
            signer: "Grace".to_string(),
        }],
    })
    .exec(&mut db)
    .await
    .expect("seed delivery");
    (db, delivery)
}

/// The resource declares the columns the model cannot pick: a file and an embedded value in the
/// table.
struct DeclaredDeliveryResource;

impl Resource for DeclaredDeliveryResource {
    type Model = Delivery;
    type Form = NoForm<Delivery>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(tablo::ReadOnly)
            .table(Table::new((
                TextColumn::new(lens!(Delivery.address)),
                EmbeddedColumn::new(lens!(Delivery.stage)),
            )))
            .detail(Detail::new(FileColumn::new(lens!(Delivery.receipt))))
    }
}

/// A declared file column links its path; a declared embedded column exports its leaves.
#[tokio::test]
async fn declared_columns_link_a_file_and_export_an_embedded_value() {
    let (db, delivery) = delivery_db().await;
    let router = panel_router::<DeclaredDeliveryResource>(db);

    let detail = body_string(
        get(
            &router,
            &format!("/admin/declared-deliveries/{}", delivery.id),
        )
        .await,
    )
    .await;
    assert!(
        entry(&detail, "Receipt").contains("href=\"/uploads/receipt.pdf\""),
        "a file links its path: {detail}"
    );

    let csv = body_string(get(&router, "/admin/declared-deliveries/-/export").await).await;
    assert!(
        csv.contains("12 Rue Haute,\"Stage: Out, Note: shipped\"\n"),
        "the embedded column exports its leaves: {csv}"
    );
}

/// An invoice whose fields shape their columns and declare their filters.
#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Invoice {
    #[key]
    #[auto]
    id: Uuid,
    number: String,
    #[columns(badge, filter)]
    status: TicketStatus,
    #[columns(filter)]
    paid: bool,
    #[columns(label = "Amount due", money = EUR)]
    total: i64,
    #[columns(hidden_by_default, filter)]
    issued_at: jiff::Timestamp,
}

/// A resource taking the model's table and detail page.
struct InvoiceResource;

impl Resource for InvoiceResource {
    type Model = Invoice;
    type Form = NoForm<Invoice>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().policy(tablo::ReadOnly)
    }
}

/// A resource extending the model's table with a filter of its own.
struct ExtendedInvoiceResource;

impl Resource for ExtendedInvoiceResource {
    type Model = Invoice;
    type Form = NoForm<Invoice>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(tablo::ReadOnly)
            .table(Invoice::table().filters(SelectFilter::new(lens!(Invoice.number), ["A-1"])))
    }
}

async fn invoice_db() -> (Db, Invoice) {
    let mut db = test_db(toasty::models!(Invoice)).await;
    let paid = toasty::create!(Invoice {
        number: "A-1".to_string(),
        status: TicketStatus::Waiting,
        paid: true,
        total: 1234,
        issued_at: "2020-03-01T12:00:00Z".parse::<jiff::Timestamp>().unwrap(),
    })
    .exec(&mut db)
    .await
    .expect("seed invoice");
    toasty::create!(Invoice {
        number: "A-2".to_string(),
        status: TicketStatus::Open,
        paid: false,
        total: 50,
        issued_at: "2030-03-01T12:00:00Z".parse::<jiff::Timestamp>().unwrap(),
    })
    .exec(&mut db)
    .await
    .expect("seed invoice");
    (db, paid)
}

/// The invoice numbers the list at `uri` shows, in order.
async fn listed(router: &Router, uri: &str) -> Vec<String> {
    let html = body_string(get(router, uri).await).await;
    rows(&html)
        .into_iter()
        .filter_map(|row| row.cells.into_iter().find(|cell| cell.starts_with("A-")))
        .collect()
}

/// `#[columns(filter)]` gives the field the filter its type picks: a select over an options
/// enum, a ternary over a `bool`, a range of days over a timestamp.
#[tokio::test]
async fn a_field_filter_filters_the_list_by_the_field() {
    let (db, _) = invoice_db().await;
    let router = panel_router::<InvoiceResource>(db);

    let html = body_string(get(&router, "/admin/invoices").await).await;
    let waiting = filter_options(&html, "status")
        .expect("a status filter")
        .into_iter()
        .find(|option| option.label == "Waiting on customer")
        .expect("the filter offers the enum's options")
        .value;
    let uri = format!("/admin/invoices?f.status={waiting}");
    assert_eq!(listed(&router, &uri).await, ["A-1"]);
    assert_eq!(
        listed(&router, "/admin/invoices?f.paid=false").await,
        ["A-2"]
    );
    assert_eq!(
        listed(&router, "/admin/invoices?f.issued_at.from=2025-01-01").await,
        ["A-2"]
    );
}

/// A table extended from the model's adds its filters after the derived ones.
#[tokio::test]
async fn an_extended_table_keeps_the_field_filters() {
    let (db, _) = invoice_db().await;
    let router = panel_router::<ExtendedInvoiceResource>(db);

    let html = body_string(get(&router, "/admin/extended-invoices").await).await;
    assert!(filter_options(&html, "status").is_some(), "{html}");
    assert!(filter_options(&html, "number").is_some(), "{html}");
}

/// The presentation keys shape the column in the table and on the detail page:
/// `hidden_by_default` starts the column hidden in the list, `money` spells the number as an
/// amount, and `label` names the entry.
#[tokio::test]
async fn the_presentation_keys_shape_the_table_and_the_detail_page() {
    let (db, invoice) = invoice_db().await;
    let router = panel_router::<InvoiceResource>(db);

    let html = body_string(get(&router, "/admin/invoices").await).await;
    assert_eq!(
        columns(&html),
        ["number", "status", "paid", "total"],
        "the hidden column starts out of the list"
    );
    assert!(
        rows(&html)
            .iter()
            .any(|row| row.cells.iter().any(|cell| cell == "€1,234.00")),
        "{html}"
    );

    let detail = body_string(get(&router, &format!("/admin/invoices/{}", invoice.id)).await).await;
    let entries = detail_entries(&detail);
    assert_eq!(
        entries
            .iter()
            .map(|e| e.column.as_str())
            .collect::<Vec<_>>(),
        ["number", "status", "paid", "total", "issued_at"],
        "the detail page shows every column, a hidden one included"
    );
    let total = entries
        .iter()
        .find(|e| e.column == "total")
        .expect("a total entry");
    assert!(
        total.text.contains("Amount due") && total.text.contains("€1,234.00"),
        "{total:?}"
    );
}

/// A resource declaring a table and no detail page.
struct TabledTicketResource;

impl Resource for TabledTicketResource {
    type Model = Ticket;
    type Form = TicketForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(tablo::ReadOnly)
            .edit_only()
            .table(Table::new((
                TextColumn::new(lens!(Ticket.subject)),
                BooleanColumn::new(lens!(Ticket.urgent)),
            )))
    }
}

/// A declared table is the detail page's default: the page shows its columns, in its order.
#[tokio::test]
async fn the_detail_page_defaults_to_the_declared_table() {
    let (db, ticket) = ticket_db().await;
    let router = panel_router::<TabledTicketResource>(db);

    let detail = get(&router, &format!("/admin/tabled-tickets/{}", ticket.id)).await;
    assert_eq!(detail.status(), StatusCode::OK);
    let entries = detail_entries(&body_string(detail).await);
    assert_eq!(
        entries
            .iter()
            .map(|e| e.column.as_str())
            .collect::<Vec<_>>(),
        ["subject", "urgent"]
    );
}
