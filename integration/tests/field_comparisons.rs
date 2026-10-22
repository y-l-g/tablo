//! A field compared with another over HTTP: the server's check, its round with the other
//! controls' errors, an edit's stored pair, and the declarations that refuse the mount.

use jiff::civil::date;
use tablo::{
    Allow, Auth, DeclarationErrorKind, FieldAccess, FieldAccessFault, FieldRuleFault, Panel,
    Resource, ResourceDef, Schema, Table, TextColumn, lens, testing::ErrorKind,
};
use toasty::Db;
use topcoat::router::Router;
use uuid::Uuid;

use crate::common::{TestClient, body_string, field_error, mount, refusal, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Stay {
    #[key]
    #[auto]
    id: Uuid,
    guest: String,
    nickname: String,
    adults: i64,
    rooms: i64,
    arrives: jiff::civil::Date,
    leaves: Option<jiff::civil::Date>,
}

#[derive(tablo::RecordForm)]
#[form(model = Stay)]
struct StayForm {
    guest: String,
    nickname: String,
    adults: i64,
    rooms: i64,
    arrives: jiff::civil::Date,
    leaves: Option<jiff::civil::Date>,
}

struct StayResource;

impl Resource for StayResource {
    type Model = Stay;
    type Form = StayForm;

    fn declare() -> ResourceDef<Self> {
        let c = StayForm::controls();
        let nickname = c.nickname.different(&c.guest);
        let rooms = c.rooms.lte(&c.adults);
        let leaves = c.leaves.gt(&c.arrives);
        ResourceDef::new()
            .slug("stays")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Stay.guest))))
            .form(Schema::new((
                c.guest, nickname, c.adults, rooms, c.arrives, leaves,
            )))
    }
}

async fn stays() -> (Db, Router) {
    let db = test_db(toasty::models!(Stay)).await;
    let router = mount(
        db.clone(),
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<StayResource>(),
    )
    .expect("panel builds");
    (db, router)
}

async fn stay_count(db: &Db) -> usize {
    Stay::all().exec(&mut db.clone()).await.unwrap().len()
}

/// The server refuses a value that does not relate to the other field's as declared, on the
/// declaring field, and writes nothing.
#[tokio::test]
async fn the_server_refuses_a_pair_breaking_its_comparison() {
    let (db, router) = stays().await;
    let client = TestClient::new(&router);
    for (body, refused) in [
        (
            "guest=Ada&nickname=Al&adults=2&rooms=1&arrives=2026-05-02&leaves=2026-05-01",
            "leaves",
        ),
        (
            "guest=Ada&nickname=Al&adults=2&rooms=1&arrives=2026-05-02&leaves=2026-05-02",
            "leaves",
        ),
        (
            "guest=Ada&nickname=Al&adults=2&rooms=3&arrives=2026-05-01&leaves=",
            "rooms",
        ),
        (
            "guest=Ada&nickname=Ada&adults=2&rooms=1&arrives=2026-05-01&leaves=",
            "nickname",
        ),
    ] {
        let response = client.submit("/admin/stays/-/create", body).await;
        assert_eq!(response.status(), 200, "{body}");
        let html = body_string(response).await;
        assert_eq!(
            field_error(&html, refused),
            Some(ErrorKind::Invalid),
            "{body}"
        );
    }
    assert_eq!(stay_count(&db).await, 0);

    // An inclusive comparison takes an equal value, and a blank one skips its comparison.
    let response = client
        .submit(
            "/admin/stays/-/create",
            "guest=Ada&nickname=Al&adults=2&rooms=2&arrives=2026-05-01&leaves=",
        )
        .await;
    assert_eq!(response.status(), 303);
    assert_eq!(stay_count(&db).await, 1);
}

/// A refused comparison renders with a missing field's error, in one round: `validate_record`
/// would wait until every field parses.
#[tokio::test]
async fn a_comparison_refuses_in_the_same_round_as_a_missing_field() {
    let (_db, router) = stays().await;
    let html = body_string(
        TestClient::new(&router)
            .submit(
                "/admin/stays/-/create",
                "guest=Ada&nickname=Al&adults=&rooms=1&arrives=2026-05-02&leaves=2026-05-01",
            )
            .await,
    )
    .await;
    assert_eq!(field_error(&html, "adults"), Some(ErrorKind::Required));
    assert_eq!(field_error(&html, "leaves"), Some(ErrorKind::Invalid));
    assert_eq!(
        field_error(&html, "rooms"),
        None,
        "a blank side skips the comparison: {html}"
    );
}

/// An edit keeping a stored pair saves though the pair breaks the comparison, and an edit
/// changing either side checks it: moving the arrival past an untouched departure is refused.
#[tokio::test]
async fn an_edit_checks_a_pair_once_either_side_changes() {
    let (db, router) = stays().await;
    let stay = toasty::create!(Stay {
        guest: "Ada".to_string(),
        nickname: "Al".to_string(),
        adults: 2,
        rooms: 1,
        arrives: date(2026, 5, 3),
        leaves: Some(date(2026, 5, 1)),
    })
    .exec(&mut db.clone())
    .await
    .unwrap();
    let client = TestClient::new(&router);
    let edit = format!("/admin/stays/{}/edit", stay.id);
    let response = client
        .submit(
            &edit,
            "guest=Ada&nickname=Al&adults=2&rooms=1&arrives=2026-05-03&leaves=2026-05-01",
        )
        .await;
    assert_eq!(response.status(), 303, "an untouched pair saves");
    let response = client
        .submit(
            &edit,
            "guest=Ada&nickname=Al&adults=2&rooms=1&arrives=2026-05-04&leaves=2026-05-01",
        )
        .await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert_eq!(
        field_error(&html, "leaves"),
        Some(ErrorKind::Invalid),
        "{html}"
    );
}

/// A comparison with the field itself, with a field of another type, or one ordering a type
/// without an order, refuses the mount. A resource's form places every record-form field, so only
/// a hand-built schema can leave a compared field out.
#[tokio::test]
async fn a_misdeclared_comparison_refuses_the_mount() {
    struct Misdeclared;

    impl Resource for Misdeclared {
        type Model = Stay;
        type Form = StayForm;

        fn declare() -> ResourceDef<Self> {
            let c = StayForm::controls();
            let guest = c.guest.gt(&c.nickname);
            let adults = c.adults.lt(&c.arrives);
            let rooms = c.rooms.gt(&StayForm::controls().rooms);
            ResourceDef::new()
                .slug("stays")
                .policy(Allow)
                .form(Schema::new((guest, c.nickname, adults, rooms, c.arrives)))
        }
    }

    let db = test_db(toasty::models!(Stay)).await;
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
        rule(
            "guest",
            FieldRuleFault::UnorderedComparison("nickname".to_string()),
        ),
        rule(
            "adults",
            FieldRuleFault::IncomparableField("arrives".to_string()),
        ),
        rule("rooms", FieldRuleFault::SelfComparison),
    ] {
        assert!(kinds.contains(&expected), "{expected:?} in {kinds:?}");
    }
}

/// A refusal tells what the compared field holds, so comparing with a field a `view` rule hides
/// from some user refuses the mount: posting guesses would read the hidden value.
#[tokio::test]
async fn a_comparison_with_a_hidden_field_refuses_the_mount() {
    struct Hiding;

    impl Resource for Hiding {
        type Model = Stay;
        type Form = StayForm;

        fn declare() -> ResourceDef<Self> {
            let c = StayForm::controls();
            let rooms = c.rooms.lte(&c.adults);
            ResourceDef::new()
                .slug("stays")
                .policy(Allow)
                .field_access(lens!(Stay.adults), FieldAccess::new().view(|_| false))
                .form(Schema::new((
                    c.guest, c.nickname, c.adults, rooms, c.arrives, c.leaves,
                )))
        }
    }

    let db = test_db(toasty::models!(Stay)).await;
    let errors = refusal(mount(
        db,
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<Hiding>(),
    ));
    let watched = DeclarationErrorKind::FieldAccess {
        fault: FieldAccessFault::Watched,
    };
    assert!(
        errors.iter().any(|error| error.kind == watched),
        "{errors:?}"
    );
}
