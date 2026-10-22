//! A write that refuses its submission: a record fn, an action or a header action answers
//! `WriteError::Refused`, the transaction rolls back what it wrote first, and the form renders
//! again with the errors under their fields.

use tablo::{
    Ability, Action, FieldErrors, FormInput, HeaderAction, Resource, ResourceDef, Table,
    TextColumn, WriteError, lens,
    testing::{ImportError, field_error, import_errors},
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{body_string, import_csv, mount, panel, post_fields, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Booking {
    #[key]
    #[auto]
    id: Uuid,
    room: String,
    nights: i64,
}

/// A row every write takes before its rule reads the database, so a refusal has something to
/// roll back.
#[derive(Debug, toasty::Model)]
struct Hold {
    #[key]
    #[auto]
    id: Uuid,
    room: String,
}

#[derive(Debug, Clone, tablo::RecordForm)]
#[form(model = Booking)]
struct BookingForm {
    room: String,
    nights: i64,
}

/// The longest stay a booking or an extension takes.
const MAX_NIGHTS: i64 = 10;

/// Holds `room`, then refuses `nights` past [`MAX_NIGHTS`] and a room another write holds.
async fn hold(
    room: &str,
    nights: i64,
    ex: &mut dyn toasty::Executor,
) -> Result<(), WriteError<BookingFormField>> {
    toasty::create!(Hold {
        room: room.to_string()
    })
    .exec(&mut *ex)
    .await?;
    if nights > MAX_NIGHTS {
        return Err(WriteError::field(
            BookingFormField::Nights,
            "Too long a stay",
        ));
    }
    let holds = Hold::filter(Hold::fields().room().eq(room.to_string()))
        .exec(&mut *ex)
        .await?;
    if holds.len() > 1 {
        return Err(WriteError::field(BookingFormField::Room, "Room is booked"));
    }
    Ok(())
}

struct BookingResource;

impl Resource for BookingResource {
    type Model = Booking;
    type Form = BookingForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("bookings")
            .policy_fn(|_cx, _ability: Ability<'_, Booking>| true)
            .table(Table::new(TextColumn::new(lens!(Booking.room))))
            .importable()
            .action::<Extend>()
            .action::<Lock>()
            .header_action::<Block>()
    }

    async fn create_record(
        cx: &Cx,
        form: BookingForm,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Booking, WriteError<BookingFormField>> {
        hold(&form.room, form.nights, &mut *ex).await?;
        Ok(tablo::write_create::<Self>(cx, form, ex).await?)
    }

    async fn update_record(
        cx: &Cx,
        record: Booking,
        posted: tablo::Posted<BookingForm>,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Booking, WriteError<BookingFormField>> {
        if posted.room == "empty" {
            return Err(FieldErrors::new().into());
        }
        hold(&posted.room, posted.nights, &mut *ex).await?;
        Ok(tablo::write_update::<Self>(cx, record, posted, ex).await?)
    }
}

#[derive(FormInput)]
struct Extension {
    nights: i64,
}

/// Adds nights to a booking, up to [`MAX_NIGHTS`] in all.
struct Extend;

impl Action<BookingResource> for Extend {
    type Input = Extension;
    const NAME: &'static str = "extend";

    async fn run(
        _cx: &Cx,
        bookings: &[Booking],
        extension: Extension,
        ex: &mut dyn toasty::Executor,
    ) -> Result<tablo::Outcome, WriteError> {
        for booking in bookings {
            let nights = booking.nights + extension.nights;
            booking
                .clone()
                .update()
                .nights(nights)
                .exec(&mut *ex)
                .await?;
            if nights > MAX_NIGHTS {
                return Err(WriteError::field("nights", "Too long a stay"));
            }
        }
        Ok(tablo::Outcome::done())
    }
}

/// Asks for nothing, so its refusal has no control to render under.
struct Lock;

impl Action<BookingResource> for Lock {
    type Input = ();
    const NAME: &'static str = "lock";

    async fn run(
        _cx: &Cx,
        _: &[Booking],
        _: (),
        _: &mut dyn toasty::Executor,
    ) -> Result<tablo::Outcome, WriteError> {
        Err(WriteError::field("nights", "Locked"))
    }
}

#[derive(FormInput)]
struct Blocking {
    room: String,
}

/// Holds a room for every booking at once, refusing one held already.
struct Block;

impl HeaderAction for Block {
    type Input = Blocking;
    const NAME: &'static str = "block";

    async fn run(
        _cx: &Cx,
        blocking: Blocking,
        ex: &mut dyn toasty::Executor,
    ) -> Result<tablo::Outcome, WriteError> {
        match hold(&blocking.room, 0, ex).await {
            Ok(()) => Ok(tablo::Outcome::done()),
            Err(WriteError::Refused(_)) => Err(WriteError::field("room", "Room is booked")),
            Err(WriteError::Failed(error)) => Err(WriteError::Failed(error)),
        }
    }
}

async fn router() -> (Db, Router) {
    let db = test_db(toasty::models!(Booking, Hold)).await;
    let router = mount(db.clone(), panel().resource::<BookingResource>()).expect("panel builds");
    (db, router)
}

async fn bookings(db: &Db) -> Vec<Booking> {
    Booking::all().exec(&mut db.clone()).await.unwrap()
}

async fn holds(db: &Db) -> usize {
    Hold::all().exec(&mut db.clone()).await.unwrap().len()
}

async fn book(db: &Db, room: &str, nights: i64) -> Booking {
    let mut db = db.clone();
    toasty::create!(Hold {
        room: room.to_string()
    })
    .exec(&mut db)
    .await
    .unwrap();
    toasty::create!(Booking {
        room: room.to_string(),
        nights,
    })
    .exec(&mut db)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_refused_create_renders_the_form_and_writes_nothing() {
    let (db, router) = router().await;
    book(&db, "101", 2).await;

    let response = post_fields(
        &router,
        "/admin/bookings/-/create",
        &[("room", "101"), ("nights", "3")],
    )
    .await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert!(field_error(&html, "room").is_some(), "{html}");
    assert!(field_error(&html, "nights").is_none(), "{html}");
    assert_eq!(bookings(&db).await.len(), 1);
    assert_eq!(holds(&db).await, 1, "the refused write's hold rolls back");

    let response = post_fields(
        &router,
        "/admin/bookings/-/create",
        &[("room", "102"), ("nights", "3")],
    )
    .await;
    assert_eq!(response.status(), 303, "a write the rule passes lands");
    assert_eq!(bookings(&db).await.len(), 2);
}

#[tokio::test]
async fn a_refused_update_renders_the_form_and_keeps_the_record() {
    let (db, router) = router().await;
    let booking = book(&db, "101", 2).await;
    let edit = format!("/admin/bookings/{}/edit", booking.id);

    let response = post_fields(&router, &edit, &[("room", "102"), ("nights", "11")]).await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert!(field_error(&html, "nights").is_some(), "{html}");
    let [kept] = bookings(&db).await.try_into().unwrap();
    assert_eq!((kept.room.as_str(), kept.nights), ("101", 2));
    assert_eq!(holds(&db).await, 1, "the refused write's hold rolls back");
}

#[tokio::test]
async fn a_refusal_naming_no_field_is_a_declaration_error() {
    let (db, router) = router().await;
    let booking = book(&db, "101", 2).await;
    let edit = format!("/admin/bookings/{}/edit", booking.id);

    let response = post_fields(&router, &edit, &[("room", "empty"), ("nights", "2")]).await;
    assert_eq!(response.status(), 500);
    assert_eq!(bookings(&db).await[0].room, "101");
}

#[tokio::test]
async fn a_refused_action_renders_its_input_and_writes_nothing() {
    let (db, router) = router().await;
    let booking = book(&db, "101", 8).await;
    let extend = format!("/admin/bookings/{}/-/actions/extend", booking.id);

    let response = post_fields(&router, &extend, &[("nights", "3")]).await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert!(field_error(&html, "nights").is_some(), "{html}");
    assert_eq!(
        bookings(&db).await[0].nights,
        8,
        "the run's update rolls back"
    );

    let response = post_fields(&router, &extend, &[("nights", "2")]).await;
    assert_eq!(response.status(), 303);
    assert_eq!(bookings(&db).await[0].nights, 10);
}

#[tokio::test]
async fn a_refused_action_without_input_is_a_declaration_error() {
    let (db, router) = router().await;
    let booking = book(&db, "101", 2).await;

    let lock = format!("/admin/bookings/{}/-/actions/lock", booking.id);
    let response = post_fields(&router, &lock, &[]).await;
    assert_eq!(response.status(), 500);
}

#[tokio::test]
async fn a_refused_header_action_renders_its_input_and_writes_nothing() {
    let (db, router) = router().await;
    book(&db, "101", 2).await;

    let block = "/admin/bookings/-/actions/block";
    let response = post_fields(&router, block, &[("room", "101")]).await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert!(field_error(&html, "room").is_some(), "{html}");
    assert_eq!(holds(&db).await, 1, "the run's hold rolls back");

    let response = post_fields(&router, block, &[("room", "102")]).await;
    assert_eq!(response.status(), 303);
    assert_eq!(holds(&db).await, 2);
}

/// A refused row rolls back to its savepoint, so the hold it took refuses no later row; the file
/// writes nothing.
#[tokio::test]
async fn an_import_lists_the_rows_a_record_fn_refuses() {
    let (db, router) = router().await;

    let csv = "room,nights\n101,11\n101,2\n102,2\n102,3\n";
    let response = import_csv(&router, "/admin/bookings/-/import", csv).await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    let refused = |row, key: &str| ImportError {
        row,
        key: Some(key.to_string()),
    };
    assert_eq!(
        import_errors(&html),
        [refused(2, "nights"), refused(5, "room")]
    );
    assert!(bookings(&db).await.is_empty());
    assert_eq!(holds(&db).await, 0);
}
