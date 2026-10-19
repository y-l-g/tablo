//! What an action's `Outcome` does to the response once its write commits: the notification it
//! flashes and where it lands.

use http::header::LOCATION;
use tablo::{
    Action, Allow, HeaderAction, Notification, NotificationStatus, Outcome, Resource, ResourceDef,
    Table, TextColumn, lens,
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{flash, mount, panel, post_fields, test_db};

#[derive(Debug, toasty::Model, tablo::ModelColumns, Clone)]
struct Ticket {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    closed: bool,
}

/// Closes open tickets, reports how many with its own notification, and opens the first.
struct Close;

impl Action<TicketResource> for Close {
    type Input = ();
    const NAME: &'static str = "close";

    fn can_run(_cx: &Cx, ticket: &Ticket) -> bool {
        !ticket.closed
    }

    async fn run(
        _cx: &Cx,
        tickets: &[Ticket],
        _: (),
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Outcome> {
        for ticket in tickets {
            Ticket::filter(Ticket::fields().id().eq(ticket.id))
                .update()
                .closed(true)
                .exec(&mut *ex)
                .await?;
        }
        Ok(Outcome::done()
            .notify(Notification::warning(format!("Closed {}", tickets.len())))
            .redirect(format!("/admin/tickets/{}", tickets[0].id)))
    }
}

/// Closes tickets and asks to land off the site.
struct Leave;

impl Action<TicketResource> for Leave {
    type Input = ();
    const NAME: &'static str = "leave";

    async fn run(
        _cx: &Cx,
        tickets: &[Ticket],
        _: (),
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Outcome> {
        for ticket in tickets {
            Ticket::filter(Ticket::fields().id().eq(ticket.id))
                .update()
                .closed(true)
                .exec(&mut *ex)
                .await?;
        }
        Ok(Outcome::done().redirect("//elsewhere.example/landing"))
    }
}

/// Lands on the panel's root from the list's header.
struct Home;

impl HeaderAction for Home {
    type Input = ();
    const NAME: &'static str = "home";

    async fn run(_: &Cx, _: (), _: &mut dyn toasty::Executor) -> topcoat::Result<Outcome> {
        Ok(Outcome::done().redirect("/admin"))
    }
}

struct TicketResource;

impl Resource for TicketResource {
    type Model = Ticket;
    type Form = tablo::NoForm<Ticket>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("tickets")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Ticket.title))))
            .detail(tablo::Detail::new(TextColumn::new(lens!(Ticket.title))))
            .action::<Close>()
            .action::<Leave>()
            .header_action::<Home>()
    }
}

async fn db() -> Db {
    test_db(toasty::models!(Ticket)).await
}

async fn seed(db: &Db, title: &str, closed: bool) -> Ticket {
    let mut db = db.clone();
    toasty::create!(Ticket {
        title: title.to_string(),
        closed,
    })
    .exec(&mut db)
    .await
    .expect("seed ticket")
}

async fn is_closed(db: &Db, id: Uuid) -> bool {
    let mut db = db.clone();
    Ticket::get_by_id(&mut db, &id)
        .await
        .expect("the ticket exists")
        .closed
}

fn router(db: &Db) -> Router {
    mount(db.clone(), panel().resource::<TicketResource>()).expect("panel builds")
}

fn location(response: &http::Response<topcoat::router::Body>) -> &str {
    response
        .headers()
        .get(LOCATION)
        .and_then(|value| value.to_str().ok())
        .expect("the response redirects")
}

#[tokio::test]
async fn a_row_run_flashes_its_own_notification_and_lands_where_it_asks() {
    let db = db().await;
    let ticket = seed(&db, "Alpha", false).await;
    let router = router(&db);

    let response = post_fields(
        &router,
        &format!("/admin/tickets/{}/-/actions/close", ticket.id),
        &[],
    )
    .await;
    assert_eq!(response.status(), 303);
    assert_eq!(location(&response), format!("/admin/tickets/{}", ticket.id));
    let note = flash(&response);
    assert_eq!(note.status, NotificationStatus::Warning);
    assert_eq!(note.title, "Closed 1");
    assert!(is_closed(&db, ticket.id).await);
}

#[tokio::test]
async fn a_bulk_run_appends_the_skipped_count_to_its_own_notification() {
    let db = db().await;
    let open = seed(&db, "Open", false).await;
    let done = seed(&db, "Done", true).await;
    let router = router(&db);
    let ids = format!("{},{}", open.id, done.id);

    let response = post_fields(&router, "/admin/tickets/-/actions/close", &[("ids", &ids)]).await;
    assert_eq!(location(&response), format!("/admin/tickets/{}", open.id));
    assert_eq!(flash(&response).title, "Closed 1 (1 of 2 skipped)");
}

#[tokio::test]
async fn a_redirect_off_the_site_lands_where_the_run_started_and_keeps_the_write() {
    let db = db().await;
    let ticket = seed(&db, "Alpha", false).await;
    let router = router(&db);

    let response = post_fields(
        &router,
        &format!("/admin/tickets/{}/-/actions/leave", ticket.id),
        &[],
    )
    .await;
    assert_eq!(location(&response), "/admin/tickets");
    assert_eq!(flash(&response).status, NotificationStatus::Success);
    assert!(
        is_closed(&db, ticket.id).await,
        "the committed write stands"
    );
}

#[tokio::test]
async fn a_header_action_lands_where_it_asks() {
    let db = db().await;
    let router = router(&db);

    let response = post_fields(&router, "/admin/tickets/-/actions/home", &[]).await;
    assert_eq!(location(&response), "/admin");
}
