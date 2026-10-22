//! Who may run a custom action: the policy the panel mounted, through `RunAny` and `Run`.
//!
//! One resource declares the action under `Allow`, and each test remounts it with
//! `Panel::resource_with`, the guide's way to serve a resource read-only in a second panel.

use tablo::{
    Ability, Action, Allow, Detail, NoForm, NotificationStatus, Places, Policy, ReadOnly, Resource,
    ResourceDef, Table, TextColumn, lens, url,
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{
    body_string, confirms_first, flash, get, mount, panel, panel_cx, post_fields, rows, test_db,
};

#[derive(Debug, toasty::Model, tablo::ModelColumns, Clone)]
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
    type Input = ();
    const NAME: &'static str = "close";

    fn label(_cx: &Cx) -> String {
        "Close".to_string()
    }

    async fn run(
        _cx: &Cx,
        tickets: &[Ticket],
        _: (),
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<tablo::Outcome, tablo::WriteError> {
        for ticket in tickets {
            Ticket::filter(Ticket::fields().id().eq(ticket.id))
                .update()
                .closed(true)
                .exec(&mut *ex)
                .await?;
        }
        Ok(tablo::Outcome::done())
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
    test_db(toasty::models!(Ticket)).await
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

fn row_close(db: &Db, ticket: &Ticket) -> String {
    url::action::<TicketResource, Close>(&panel_cx::<TicketResource>(db), ticket)
        .expect("the tickets declare Close")
}

fn bulk_close(db: &Db) -> String {
    url::bulk_action::<TicketResource, Close>(&panel_cx::<TicketResource>(db))
        .expect("the tickets declare Close")
}

/// Reopens a ticket; no resource declares it.
struct Reopen;

impl Action<TicketResource> for Reopen {
    type Input = ();
    const NAME: &'static str = "reopen";

    fn label(_cx: &Cx) -> String {
        "Reopen".to_string()
    }

    async fn run(
        _cx: &Cx,
        _: &[Ticket],
        _: (),
        _: &mut dyn toasty::Executor,
    ) -> topcoat::Result<tablo::Outcome, tablo::WriteError> {
        Ok(tablo::Outcome::done())
    }
}

/// Archives a selection; declared in the bulk bar only.
struct Archive;

impl Action<TicketResource> for Archive {
    type Input = ();
    const NAME: &'static str = "archive";
    const PLACES: Places = Places::BULK;

    fn label(_cx: &Cx) -> String {
        "Archive".to_string()
    }

    async fn run(
        _cx: &Cx,
        _: &[Ticket],
        _: (),
        _: &mut dyn toasty::Executor,
    ) -> topcoat::Result<tablo::Outcome, tablo::WriteError> {
        Ok(tablo::Outcome::done())
    }
}

/// The tickets, listed with no record form: no create or edit page.
struct TicketLog;

impl Resource for TicketLog {
    type Model = Ticket;
    type Form = NoForm<Ticket>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("ticket-log")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Ticket.title))))
    }
}

#[tokio::test]
async fn the_url_helpers_spell_a_records_routes() {
    let db = db().await;
    let ticket = seed(&db, "Alpha").await;
    let cx = panel()
        .resource_with::<TicketResource>(|def| def.action::<Archive>())
        .resource::<TicketLog>()
        .handle(&db)
        .expect("panel builds")
        .context();
    let id = ticket.id;

    assert_eq!(
        url::record::<TicketResource>(&cx, &ticket),
        Some(format!("/admin/tickets/{id}"))
    );
    assert_eq!(
        url::edit::<TicketResource>(&cx, &ticket),
        Some(format!("/admin/tickets/{id}/edit"))
    );
    assert_eq!(
        url::create::<TicketResource>(&cx).as_deref(),
        Some("/admin/tickets/-/create")
    );
    assert_eq!(
        url::action::<TicketResource, Close>(&cx, &ticket),
        Some(format!("/admin/tickets/{id}/-/actions/close"))
    );
    assert_eq!(
        url::bulk_action::<TicketResource, Close>(&cx).as_deref(),
        Some("/admin/tickets/-/actions/close")
    );
    assert_eq!(
        url::action::<TicketResource, Reopen>(&cx, &ticket),
        None,
        "an action the resource does not declare has no route"
    );
    assert_eq!(
        url::bulk_action::<TicketResource, Archive>(&cx).as_deref(),
        Some("/admin/tickets/-/actions/archive")
    );
    assert_eq!(
        url::action::<TicketResource, Archive>(&cx, &ticket),
        None,
        "the record route refuses an action declared in the bulk bar only"
    );

    // Without a detail or a record form, the panel serves no such page.
    let undetailed = panel()
        .resource_with::<TicketResource>(|def| def.detail(Detail::empty()))
        .handle(&db)
        .expect("panel builds")
        .context();
    assert_eq!(
        url::record::<TicketResource>(&undetailed, &ticket),
        None,
        "a resource with an empty detail has no detail page"
    );
    assert_eq!(url::edit::<TicketLog>(&cx, &ticket), None);
    assert_eq!(url::create::<TicketLog>(&cx), None);
}

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

    let row = post_fields(&router, &row_close(&db, &ticket), &[]).await;
    assert_eq!(row.status(), 403, "the row route refuses");
    let ids = ticket.id.to_string();
    let bulk = post_fields(&router, &bulk_close(&db), &[("ids", &ids)]).await;
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
        confirms_first(&html, &row_close(&db, &first)).is_some(),
        "the row offers Close: {html}"
    );
    assert!(
        confirms_first(&html, &bulk_close(&db)).is_some(),
        "the bulk bar offers Close: {html}"
    );

    let row = post_fields(&router, &row_close(&db, &first), &[]).await;
    assert_eq!(row.status(), 303, "the row action commits");
    assert!(closed(&db, first.id).await);

    let ids = format!("{},{}", second.id, third.id);
    let bulk = post_fields(&router, &bulk_close(&db), &[("ids", &ids)]).await;
    assert_eq!(bulk.status(), 303, "the bulk action commits");
    assert!(closed(&db, second.id).await && closed(&db, third.id).await);
}

/// A policy that allows every ability but closing a ticket titled "Locked". It matches the
/// action `Close`, so a `Run` asked under another name would let the locked ticket through.
fn all_but_locked(_cx: &Cx, ability: Ability<'_, Ticket>) -> bool {
    match ability {
        Ability::Run { record, .. } if ability.is_action::<Close, _>() => record.title != "Locked",
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
        confirms_first(&html, &row_close(&db, &open)).is_some(),
        "the open ticket offers Close: {html}"
    );
    assert!(
        !html.contains(&row_close(&db, &locked)),
        "the locked ticket does not: {html}"
    );

    let row = post_fields(&router, &row_close(&db, &locked), &[]).await;
    assert_eq!(row.status(), 403, "the refused row answers 403");
    assert!(!closed(&db, locked.id).await);

    let ids = format!("{},{}", open.id, locked.id);
    let bulk = post_fields(&router, &bulk_close(&db), &[("ids", &ids)]).await;
    assert_eq!(
        bulk.status(),
        303,
        "the selection runs what the policy allows"
    );
    let bulk = flash(&bulk);
    assert!(
        bulk.status == NotificationStatus::Success && bulk.title.contains("(1 of 2 skipped)"),
        "the notification reports the refused record: {}",
        bulk.title
    );
    assert!(closed(&db, open.id).await, "the allowed record is written");
    assert!(!closed(&db, locked.id).await, "the refused one is not");
}

/// Allows every ability, but none on the ticket titled "Locked", which may only be listed.
fn locked_away(_cx: &Cx, ability: Ability<'_, Ticket>) -> bool {
    match ability {
        Ability::View(ticket) | Ability::Update(ticket) | Ability::Delete(ticket) => {
            ticket.title != "Locked"
        }
        Ability::Run { record, .. } => record.title != "Locked",
        _ => true,
    }
}

/// A row's menu, like its buttons, holds no write its record's policy refuses.
#[tokio::test]
async fn a_refused_rows_menu_holds_no_write() {
    let db = db().await;
    let open = seed(&db, "Open").await;
    let locked = seed(&db, "Locked").await;
    let router = mount(
        db.clone(),
        panel().resource_with::<TicketResource>(|def| {
            def.policy(locked_away).table(
                Table::new(TextColumn::new(lens!(Ticket.title)))
                    .action_layout(tablo::ActionLayout::Menu),
            )
        }),
    )
    .expect("panel builds");

    let html = body_string(get(&router, "/admin/tickets").await).await;
    assert!(
        html.contains("Locked"),
        "the refused row still lists: {html}"
    );
    for write in [
        url::edit::<TicketResource>(&panel_cx::<TicketResource>(&db), &open).expect("an edit page"),
        format!("/admin/tickets/{}/delete", open.id),
        row_close(&db, &open),
    ] {
        assert!(html.contains(&write), "the open row offers {write}: {html}");
    }
    let locked_id = locked.id.to_string();
    assert!(
        !html.contains(&format!("{locked_id}/edit"))
            && !html.contains(&format!("{locked_id}/delete"))
            && !html.contains(&row_close(&db, &locked)),
        "the locked row's menu holds no write: {html}"
    );
}
