//! A field's declared rules over HTTP: the attributes the browser checks, the server's check of
//! the same rules, and the declarations that refuse the mount.

use jiff::civil::date;
use tablo::{
    Action, Allow, Auth, DeclarationErrorKind, Field, FieldError, FieldRuleFault, FormInput,
    FormValues, Panel, Resource, ResourceDef, Schema, Table, TextColumn, Toggle, lens, parse_list,
    parse_scalar, testing::ErrorKind,
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{
    TestClient, body_string, control, field_error, field_error_message, mount, post_fields,
    refusal, test_db,
};

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
            .action::<Book>()
            .form(Schema::new((
                c.code.pattern("[A-Z]{3}-[0-9]{2}"),
                c.seats.min(1).max(10).max_length(2),
                c.day.min(date(2026, 1, 1)),
                c.note.min_length(2).max_length(5).multiline(3),
            )))
    }
}

/// What booking a ticket asks for: a hand-written input, whose controls declare the rules.
struct Booking {
    places: Vec<String>,
    site: String,
}

impl FormInput for Booking {
    fn schema() -> Schema {
        Schema::new((
            Field::toggle_input("terms").accepted(),
            Field::choice_input("places")
                .options(["aisle", "window", "exit"])
                .multiple()
                .min_items(1)
                .max_items(2),
            Field::text_input::<String>("site").url(),
        ))
    }

    fn parse(_cx: &Cx, values: &FormValues) -> Result<Self, Vec<FieldError>> {
        Ok(Self {
            places: parse_list("places", values).map_err(|e| vec![e])?,
            site: parse_scalar("site", values, Some(String::new())).map_err(|e| vec![e])?,
        })
    }

    fn write(&self, _out: &mut FormValues) {}
}

/// Books a ticket: its note records the places and the site.
struct Book;

impl Action<TicketResource> for Book {
    type Input = Booking;
    const NAME: &'static str = "book";

    fn label(_cx: &Cx) -> String {
        "Book".to_string()
    }

    async fn run(
        _cx: &Cx,
        tickets: &[Ticket],
        booking: Booking,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<tablo::Outcome, tablo::WriteError> {
        for ticket in tickets {
            Ticket::filter(Ticket::fields().id().eq(ticket.id))
                .update()
                .note(format!("{} {}", booking.places.join(","), booking.site))
                .exec(&mut *ex)
                .await?;
        }
        Ok(tablo::Outcome::done())
    }
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Trip {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
    #[document]
    #[default(Vec::<Stop>::new())]
    stops: Vec<Stop>,
}

/// One stop of a trip: an item of a `#[document]` list.
#[derive(Debug, Clone, toasty::Embed, tablo::FormInput)]
struct Stop {
    place: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Trip)]
struct TripForm {
    name: String,
    #[form(repeat)]
    stops: Vec<Stop>,
}

struct TripResource;

impl Resource for TripResource {
    type Model = Trip;
    type Form = TripForm;

    fn declare() -> ResourceDef<Self> {
        let c = TripForm::controls();
        ResourceDef::new()
            .slug("trips")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Trip.name))))
            .form(Schema::new((c.name, c.stops.min_items(1).max_items(2))))
    }
}

async fn tickets() -> (Db, Router) {
    let db = test_db(toasty::models!(Ticket, Trip)).await;
    let router = mount(
        db.clone(),
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<TicketResource>()
            .resource::<TripResource>(),
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
    // Four characters in six bytes, under a `max_length(5)`: a length counts characters.
    let response = client
        .submit(
            "/admin/tickets/-/create",
            "code=XYZ-98&seats=9&day=2026-01-01&note=%C3%A9t%C3%A9s",
        )
        .await;
    assert_eq!(response.status(), 303);
    assert_eq!(ticket_count(&db).await, 3);
}

/// A value the field's type does not parse is refused in the type's words, though it also
/// breaks a rule: `lots` is too long for `seats` too.
#[tokio::test]
async fn the_type_refuses_before_the_rules() {
    let (_db, router) = tickets().await;
    let html = body_string(
        TestClient::new(&router)
            .submit(
                "/admin/tickets/-/create",
                "code=ABC-12&seats=lots&day=2026-02-01&note=hey",
            )
            .await,
    )
    .await;
    let message = field_error_message(&html, "seats").expect("seats is refused");
    assert!(message.contains("whole number"), "{message}");
}

/// A rule declared over stored data does not lock the records breaking it: an edit that leaves
/// the value as stored saves, and one that changes it keeps the rule.
#[tokio::test]
async fn an_edit_keeping_a_stored_value_skips_its_rules() {
    let (db, router) = tickets().await;
    let ticket = toasty::create!(Ticket {
        code: "legacy".to_string(),
        seats: 50,
        day: date(2020, 1, 1),
        note: "x".to_string(),
    })
    .exec(&mut db.clone())
    .await
    .unwrap();
    let client = TestClient::new(&router);
    let edit = format!("/admin/tickets/{}/edit", ticket.id);
    let response = client
        .submit(&edit, "code=legacy&seats=50&day=2020-01-01&note=x")
        .await;
    assert_eq!(response.status(), 303);
    let response = client
        .submit(&edit, "code=legacy&seats=51&day=2020-01-01&note=x")
        .await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert_eq!(
        field_error(&html, "seats"),
        Some(ErrorKind::Invalid),
        "{html}"
    );
}

/// A rule no value can keep, one the field cannot check, or a default the type does not parse,
/// refuses the mount.
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
                    c.code.min("a").default("abc").pattern("[a-z]{4}"),
                    c.seats.min("many").max(1).min_length(4).max_length(3),
                    c.day
                        .min(date(2026, 2, 1))
                        .max(date(2026, 1, 1))
                        .default("soon"),
                    c.note.pattern("("),
                )))
        }
    }

    let db = test_db(toasty::models!(Ticket)).await;
    let errors = refusal(mount(
        db,
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<Misdeclared>(),
    ));
    let kinds: Vec<_> = errors.into_iter().map(|error| error.kind).collect();
    let rule = |field: &str, fault: FieldRuleFault| DeclarationErrorKind::FieldRule {
        field: field.to_string(),
        fault,
    };
    for expected in [
        rule("code", FieldRuleFault::UnorderedBound("a".to_string())),
        rule("code", FieldRuleFault::RefusedDefault("abc".to_string())),
        rule("seats", FieldRuleFault::UnorderedBound("many".to_string())),
        rule("seats", FieldRuleFault::EmptyRange),
        rule("day", FieldRuleFault::EmptyRange),
        rule("day", FieldRuleFault::UnparsedDefault("soon".to_string())),
    ] {
        assert!(kinds.contains(&expected), "{expected:?} in {kinds:?}");
    }
    assert!(
        kinds.iter().any(|kind| matches!(
            kind,
            DeclarationErrorKind::FieldRule {
                field,
                fault: FieldRuleFault::InvalidPattern(_),
            } if field == "note"
        )),
        "{kinds:?}"
    );
}

/// The browser checks what it can: an accepted checkbox is required, a URL field is a URL input,
/// and a repeater carries its row bounds for its script.
#[tokio::test]
async fn the_form_renders_the_list_and_checkbox_rules() {
    let (db, router) = tickets().await;
    let ticket = seed_ticket(&db).await;
    let html = body_string(post_fields(&router, &book(&ticket), &[]).await).await;
    assert!(
        html.split('<').any(|tag| tag.contains("name=\"terms\"")
            && tag.contains("value=\"true\"")
            && tag.contains(" required")),
        "{html}"
    );
    assert!(control(&html, "site").contains("type=\"url\""), "{html}");
    let html = body_string(TestClient::new(&router).get("/admin/trips/-/create").await).await;
    assert!(
        html.contains("data-repeater-max=\"2\"") && html.contains("data-repeater-min=\"1\""),
        "{html}"
    );
}

/// The server refuses an unchecked accepted box, too few or too many chosen options, a URL
/// that is not `http` or `https`, and a hand-made request leaving a rule's key out; the action
/// runs once every rule holds.
#[tokio::test]
async fn the_server_refuses_a_list_or_checkbox_breaking_a_rule() {
    let (db, router) = tickets().await;
    let ticket = seed_ticket(&db).await;
    let valid = [
        ("terms", "false"),
        ("terms", "true"),
        ("places", ""),
        ("places", "aisle"),
        ("site", "https://example.com/a"),
    ];
    for (refused, fields) in [
        ("terms", vec![("terms", "false")]),
        ("terms", vec![]),
        ("places", vec![("places", "")]),
        ("places", vec![]),
        (
            "places",
            vec![
                ("places", "aisle"),
                ("places", "window"),
                ("places", "exit"),
            ],
        ),
        ("site", vec![("site", "javascript:alert(1)")]),
        ("site", vec![("site", "http:example.com")]),
    ] {
        let mut body: Vec<_> = valid
            .iter()
            .filter(|(key, _)| !fields.iter().any(|(k, _)| k == key) && *key != refused)
            .copied()
            .collect();
        body.extend(fields.iter().copied());
        let response = post_fields(&router, &book(&ticket), &body).await;
        assert_eq!(response.status(), 200, "{body:?}");
        let html = body_string(response).await;
        assert_eq!(
            field_error(&html, refused),
            Some(ErrorKind::Invalid),
            "{body:?}: {html}"
        );
    }
    assert_eq!(note_of(&db, &ticket).await, ticket.note, "nothing ran");

    let response = post_fields(&router, &book(&ticket), &valid).await;
    assert_eq!(response.status(), 303);
    assert_eq!(note_of(&db, &ticket).await, "aisle https://example.com/a");
}

/// A repeater refuses fewer or more rows than it takes, and writes a list within its bounds.
#[tokio::test]
async fn the_server_refuses_a_repeater_outside_its_bounds() {
    let (db, router) = tickets().await;
    let client = TestClient::new(&router);
    for body in [
        "name=none&stops=",
        "name=three&stops=0,1,2&stops.0.place=a&stops.1.place=b&stops.2.place=c",
    ] {
        let response = client.submit("/admin/trips/-/create", body).await;
        assert_eq!(response.status(), 200, "{body}");
        let html = body_string(response).await;
        assert_eq!(
            field_error(&html, "stops"),
            Some(ErrorKind::Invalid),
            "{body}: {html}"
        );
    }
    let response = client
        .submit(
            "/admin/trips/-/create",
            "name=two&stops=0,1&stops.0.place=a&stops.1.place=b",
        )
        .await;
    assert_eq!(response.status(), 303);
    let trips = Trip::all().exec(&mut db.clone()).await.unwrap();
    assert_eq!(trips.len(), 1);
    assert_eq!(trips[0].stops.len(), 2);
}

/// A list or checkbox rule no submission can keep, or one its control cannot take, refuses the
/// mount.
#[tokio::test]
async fn a_misdeclared_list_or_checkbox_rule_refuses_the_mount() {
    struct Misdeclared;

    impl FormInput for Misdeclared {
        fn schema() -> Schema {
            Schema::new((
                Field::text_input::<String>("agree")
                    .custom(Toggle)
                    .accepted(),
                Field::toggle_input("sealed").accepted().disabled(),
                Field::toggle_input("given").accepted().default(false),
                Field::choice_input("one").options(["a", "b"]).min_items(1),
                Field::choice_input("many")
                    .options(["a", "b"])
                    .multiple()
                    .min_items(3),
                Field::choice_input("crossed")
                    .options(["a", "b", "c"])
                    .multiple()
                    .min_items(2)
                    .max_items(1),
            ))
        }

        fn parse(_cx: &Cx, _values: &FormValues) -> Result<Self, Vec<FieldError>> {
            Ok(Self)
        }

        fn write(&self, _out: &mut FormValues) {}
    }

    struct Run;

    impl Action<Trips> for Run {
        type Input = Misdeclared;
        const NAME: &'static str = "run";

        fn label(_cx: &Cx) -> String {
            "Run".to_string()
        }

        async fn run(
            _cx: &Cx,
            _trips: &[Trip],
            _input: Misdeclared,
            _ex: &mut dyn toasty::Executor,
        ) -> topcoat::Result<tablo::Outcome, tablo::WriteError> {
            Ok(tablo::Outcome::done())
        }
    }

    struct Trips;

    impl Resource for Trips {
        type Model = Trip;
        type Form = TripForm;

        fn declare() -> ResourceDef<Self> {
            let c = TripForm::controls();
            ResourceDef::new()
                .slug("trips")
                .policy(Allow)
                .action::<Run>()
                .form(Schema::new((c.name, c.stops.max_items(101))))
        }
    }

    let db = test_db(toasty::models!(Trip)).await;
    let errors = refusal(mount(
        db,
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<Trips>(),
    ));
    let kinds: Vec<_> = errors.into_iter().map(|error| error.kind).collect();
    let rule = |field: &str, fault: FieldRuleFault| DeclarationErrorKind::FieldRule {
        field: field.to_string(),
        fault,
    };
    for expected in [
        rule("stops", FieldRuleFault::AboveRowCap(101)),
        rule("agree", FieldRuleFault::NotCheckbox),
        rule("sealed", FieldRuleFault::RefusedBlank),
        rule("given", FieldRuleFault::RefusedDefault("false".to_string())),
        rule("one", FieldRuleFault::SingleValue),
        rule("many", FieldRuleFault::FewerOptions(2)),
        rule("crossed", FieldRuleFault::EmptyRange),
    ] {
        assert!(kinds.contains(&expected), "{expected:?} in {kinds:?}");
    }
}

async fn seed_ticket(db: &Db) -> Ticket {
    toasty::create!(Ticket {
        code: "ABC-12".to_string(),
        seats: 2,
        day: date(2026, 2, 1),
        note: "hey".to_string(),
    })
    .exec(&mut db.clone())
    .await
    .unwrap()
}

fn book(ticket: &Ticket) -> String {
    format!("/admin/tickets/{}/-/actions/book", ticket.id)
}

async fn note_of(db: &Db, ticket: &Ticket) -> String {
    Ticket::get_by_id(&mut db.clone(), &ticket.id)
        .await
        .unwrap()
        .note
}
