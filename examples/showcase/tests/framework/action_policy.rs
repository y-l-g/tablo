//! Who may run a custom action: the policy the panel mounted, through `RunAny` and `Run`.
//!
//! One resource declares the action under `Allow`, and each test remounts it with
//! `Panel::resource_with`, the guide's way to serve a resource read-only in a second panel.

use tablo::{
    Ability, Action, Allow, Policy, ReadOnly, Resource, ResourceDef, Table, TextColumn, lens,
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::framework::common::{
    body_string, flash, get, memory_db, mount, panel, post_fields, rows,
};

#[derive(Debug, toasty::Model, Clone)]
struct Ticket {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    closed: bool,
}

/// Closes tickets, on a row or on the selection. It refuses no ticket itself, so only the
/// policy stands between a POST and the write.
struct Close;

impl Action<TicketResource> for Close {
    const NAME: &'static str = "close";

    fn label(_cx: &Cx) -> String {
        "Close".to_string()
    }

    async fn run(
        _cx: &Cx,
        tickets: &[Ticket],
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        for ticket in tickets {
            Ticket::filter(Ticket::fields().id().eq(ticket.id))
                .update()
                .closed(true)
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }
}

struct TicketResource;

impl Resource for TicketResource {
    type Model = Ticket;
    type Form = TicketForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("tickets")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Ticket.title))))
            .action::<Close>()
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Ticket)]
struct TicketForm {
    title: String,
    #[form(blank = false)]
    closed: bool,
}

async fn db() -> Db {
    memory_db(toasty::models!(Ticket)).await
}

async fn seed(db: &Db, title: &str) -> Ticket {
    let mut db = db.clone();
    toasty::create!(Ticket {
        title: title.to_string(),
        closed: false,
    })
    .exec(&mut db)
    .await
    .expect("seed ticket")
}

async fn closed(db: &Db, id: Uuid) -> bool {
    let mut db = db.clone();
    Ticket::get_by_id(&mut db, &id)
        .await
        .expect("the ticket exists")
        .closed
}

/// The tickets resource, mounted with `policy` in place of the one it declares.
fn router(db: &Db, policy: impl Policy<Ticket>) -> Router {
    mount(
        db.clone(),
        panel().resource_with::<TicketResource>(|def| def.policy(policy)),
    )
    .expect("panel builds")
}

fn row_close(ticket: &Ticket) -> String {
    format!("/admin/tickets/{}/-/actions/close", ticket.id)
}

const BULK_CLOSE: &str = "/admin/tickets/-/actions/close";

#[tokio::test]
async fn a_read_only_policy_refuses_the_action_and_writes_nothing() {
    let db = db().await;
    let ticket = seed(&db, "Alpha").await;
    let router = router(&db, ReadOnly);

    let html = body_string(get(&router, "/admin/tickets").await).await;
    let found = rows(&html);
    assert_eq!(
        found.len(),
        1,
        "the read-only list shows the ticket: {html}"
    );
    assert!(
        !html.contains("/-/actions/close"),
        "neither the row nor the bulk bar offers Close: {html}"
    );
    assert!(
        found[0].select_value.is_none(),
        "no bulk write takes the row, so it renders no checkbox: {html}"
    );

    let row = post_fields(&router, &row_close(&ticket), &[]).await;
    assert_eq!(row.status(), 403, "the row route refuses");
    let ids = ticket.id.to_string();
    let bulk = post_fields(&router, BULK_CLOSE, &[("ids", &ids)]).await;
    assert_eq!(bulk.status(), 403, "the bulk route refuses");
    assert!(!closed(&db, ticket.id).await, "nothing was written");
}

#[tokio::test]
async fn an_allowing_policy_runs_the_action_on_a_row_and_a_selection() {
    let db = db().await;
    let first = seed(&db, "Alpha").await;
    let second = seed(&db, "Bravo").await;
    let third = seed(&db, "Charlie").await;
    let router = router(&db, Allow);

    let html = body_string(get(&router, "/admin/tickets").await).await;
    assert!(
        html.contains(&format!("formaction=\"{}\"", row_close(&first))),
        "the row offers Close: {html}"
    );
    assert!(
        html.contains(&format!("formaction=\"{BULK_CLOSE}\"")),
        "the bulk bar offers Close: {html}"
    );

    let row = post_fields(&router, &row_close(&first), &[]).await;
    assert_eq!(row.status(), 303, "the row action commits");
    assert!(closed(&db, first.id).await);

    let ids = format!("{},{}", second.id, third.id);
    let bulk = post_fields(&router, BULK_CLOSE, &[("ids", &ids)]).await;
    assert_eq!(bulk.status(), 303, "the bulk action commits");
    assert!(closed(&db, second.id).await && closed(&db, third.id).await);
}

/// A policy that allows every ability but closing a ticket titled "Locked". It matches the
/// action's name, so a `Run` asked under another name would let the locked ticket through.
fn all_but_locked(_cx: &Cx, ability: Ability<'_, Ticket>) -> bool {
    match ability {
        Ability::Run {
            action: "close",
            record,
        } => record.title != "Locked",
        _ => true,
    }
}

#[tokio::test]
async fn a_record_the_policy_refuses_to_run_on_has_no_button_and_is_skipped() {
    let db = db().await;
    let open = seed(&db, "Open").await;
    let locked = seed(&db, "Locked").await;
    let router = router(&db, all_but_locked);

    let html = body_string(get(&router, "/admin/tickets").await).await;
    assert!(
        html.contains(&format!("formaction=\"{}\"", row_close(&open))),
        "the open ticket offers Close: {html}"
    );
    assert!(
        !html.contains(&row_close(&locked)),
        "the locked ticket does not: {html}"
    );

    let row = post_fields(&router, &row_close(&locked), &[]).await;
    assert_eq!(row.status(), 403, "the refused row answers 403");
    assert!(!closed(&db, locked.id).await);

    let ids = format!("{},{}", open.id, locked.id);
    let bulk = post_fields(&router, BULK_CLOSE, &[("ids", &ids)]).await;
    assert_eq!(
        bulk.status(),
        303,
        "the selection runs what the policy allows"
    );
    assert!(
        flash(&bulk).contains("(1 of 2 skipped)"),
        "the notification reports the refused record: {}",
        flash(&bulk)
    );
    assert!(closed(&db, open.id).await, "the allowed record is written");
    assert!(!closed(&db, locked.id).await, "the refused one is not");
}
