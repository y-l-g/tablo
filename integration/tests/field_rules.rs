//! A text field's declared rules over HTTP: the attributes the browser checks, the server's
//! check of the same rules, and the declarations that refuse the mount.

use jiff::civil::date;
use tablo::{
    Allow, Auth, DeclarationErrorKind, Panel, Resource, ResourceDef, Schema, Table, TextColumn,
    lens, testing::ErrorKind,
};
use toasty::Db;
use topcoat::router::Router;
use uuid::Uuid;

use crate::common::{TestClient, body_string, control, field_error, memory_db, mount, refusal};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Ticket {
    #[key]
    #[auto]
    id: Uuid,
    code: String,
    seats: i64,
    day: jiff::civil::Date,
    note: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Ticket)]
struct TicketForm {
    code: String,
    seats: i64,
    day: jiff::civil::Date,
    note: String,
}

struct TicketResource;

impl Resource for TicketResource {
    type Model = Ticket;
    type Form = TicketForm;

    fn declare() -> ResourceDef<Self> {
        let c = TicketForm::controls();
        ResourceDef::new()
            .slug("tickets")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Ticket.code))))
            .form(Schema::new((
                c.code.pattern("[A-Z]{3}-[0-9]{2}"),
                c.seats.min(1).max(10),
                c.day.min(date(2026, 1, 1)),
                c.note.min_length(2).max_length(5).multiline(3),
            )))
    }
}

async fn tickets() -> (Db, Router) {
    let db = memory_db(toasty::models!(Ticket)).await;
    let router = mount(
        db.clone(),
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<TicketResource>(),
    )
    .expect("panel builds");
    (db, router)
}

async fn ticket_count(db: &Db) -> usize {
    Ticket::all().exec(&mut db.clone()).await.unwrap().len()
}

/// The browser checks each rule before it submits: the controls carry them as attributes.
#[tokio::test]
async fn the_form_renders_each_rule_as_its_attribute() {
    let (_db, router) = tickets().await;
    let html = body_string(
        TestClient::new(&router)
            .get("/admin/tickets/-/create")
            .await,
    )
    .await;
    assert!(
        control(&html, "code").contains("pattern=\"[A-Z]{3}-[0-9]{2}\""),
        "{html}"
    );
    let seats = control(&html, "seats");
    assert!(
        seats.contains("min=\"1\"") && seats.contains("max=\"10\""),
        "{seats}"
    );
    assert!(
        control(&html, "day").contains("min=\"2026-01-01\""),
        "{html}"
    );
    let note = control(&html, "note");
    assert!(
        note.contains("minlength=\"2\"") && note.contains("maxlength=\"5\""),
        "{note}"
    );
}

/// The server refuses what each rule refuses, whatever the browser let through, and writes
/// nothing; a submission keeping every rule writes.
#[tokio::test]
async fn the_server_refuses_a_value_breaking_a_rule() {
    let (db, router) = tickets().await;
    let client = TestClient::new(&router);
    for (body, refused) in [
        ("code=abc-12&seats=2&day=2026-02-01&note=hey", "code"),
        ("code=ABC-123&seats=2&day=2026-02-01&note=hey", "code"),
        ("code=ABC-12&seats=0&day=2026-02-01&note=hey", "seats"),
        ("code=ABC-12&seats=11&day=2026-02-01&note=hey", "seats"),
        ("code=ABC-12&seats=2&day=2025-12-31&note=hey", "day"),
        ("code=ABC-12&seats=2&day=2026-02-01&note=h", "note"),
        (
            "code=ABC-12&seats=2&day=2026-02-01&note=hello+there",
            "note",
        ),
    ] {
        let response = client.submit("/admin/tickets/-/create", body).await;
        assert_eq!(response.status(), 200, "{body}");
        let html = body_string(response).await;
        assert_eq!(
            field_error(&html, refused),
            Some(ErrorKind::Invalid),
            "{body}: {html}"
        );
    }
    assert_eq!(ticket_count(&db).await, 0);

    // Each bound is inclusive, and a number compares as a number: 9 is below 10.
    let response = client
        .submit(
            "/admin/tickets/-/create",
            "code=ABC-12&seats=10&day=2026-01-01&note=hi",
        )
        .await;
    assert_eq!(response.status(), 303);
    let response = client
        .submit(
            "/admin/tickets/-/create",
            "code=XYZ-99&seats=9&day=2026-01-01&note=hello",
        )
        .await;
    assert_eq!(response.status(), 303);
    assert_eq!(ticket_count(&db).await, 2);
}

/// A rule no value can keep, or one the field cannot check, refuses the mount.
#[tokio::test]
async fn a_misdeclared_rule_refuses_the_mount() {
    struct Misdeclared;

    impl Resource for Misdeclared {
        type Model = Ticket;
        type Form = TicketForm;

        fn declare() -> ResourceDef<Self> {
            let c = TicketForm::controls();
            ResourceDef::new()
                .slug("tickets")
                .policy(Allow)
                .form(Schema::new((
                    c.code.min("a"),
                    c.seats.min("many").max(1).min_length(4).max_length(3),
                    c.day.min(date(2026, 2, 1)).max(date(2026, 1, 1)),
                    c.note.pattern("("),
                )))
        }
    }

    let db = memory_db(toasty::models!(Ticket)).await;
    let errors = refusal(mount(
        db,
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<Misdeclared>(),
    ));
    let kinds: Vec<_> = errors.into_iter().map(|error| error.kind).collect();
    let unordered = |field: &str, bound: &str| DeclarationErrorKind::UnorderedBound {
        field: field.to_string(),
        bound: bound.to_string(),
    };
    let empty = |field: &str| DeclarationErrorKind::EmptyRange {
        field: field.to_string(),
    };
    for expected in [
        unordered("code", "a"),
        unordered("seats", "many"),
        empty("seats"),
        empty("day"),
    ] {
        assert!(kinds.contains(&expected), "{expected:?} in {kinds:?}");
    }
    assert!(
        kinds.iter().any(|kind| matches!(
            kind,
            DeclarationErrorKind::InvalidPattern { field, .. } if field == "note"
        )),
        "{kinds:?}"
    );
}
