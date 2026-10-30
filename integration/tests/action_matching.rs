//! A bulk action on every record the list's query selects: the bulk bar's Select all matching,
//! which posts the list's query under `matching` instead of the selected `ids`.

use tablo::{
    Ability, Action, Allow, Limits, NotificationStatus, Policy, Resource, ResourceDef, Table,
    TextColumn, filter_keys, lens, url,
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{body_string, flash, get, mount, panel, panel_cx, post_fields, test_db};

#[derive(Debug, toasty::Model, tablo::ModelColumns, Clone)]
struct Ticket {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    closed: bool,
}

/// Closes the selected tickets.
struct Close;

impl Action<TicketResource> for Close {
    type Input = ();
    const NAME: &'static str = "close";

    async fn run(
        _cx: &Cx,
        tickets: &[Ticket],
        _: (),
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<tablo::Outcome, tablo::WriteError<std::convert::Infallible>> {
        Ticket::filter(filter_keys(tickets))
            .update()
            .closed(true)
            .exec(ex)
            .await?;
        Ok(tablo::Outcome::done())
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
            .table(Table::new(TextColumn::new(lens!(Ticket.title)).searchable()).page_size(2))
            .action::<Close>()
    }
}

/// Seeds a ticket per title, open.
async fn db(titles: &[&str]) -> Db {
    let db = test_db(toasty::models!(Ticket)).await;
    let mut conn = db.clone();
    for title in titles {
        toasty::create!(Ticket {
            title: title.to_string(),
            closed: false,
        })
        .exec(&mut conn)
        .await
        .expect("seed ticket");
    }
    db
}

/// The titles of the closed tickets, sorted.
async fn closed(db: &Db) -> Vec<String> {
    let mut conn = db.clone();
    let mut titles: Vec<String> = Ticket::all()
        .exec(&mut conn)
        .await
        .expect("query tickets")
        .into_iter()
        .filter(|ticket| ticket.closed)
        .map(|ticket| ticket.title)
        .collect();
    titles.sort();
    titles
}

fn router(db: &Db, policy: impl Policy<Ticket>, limits: Limits) -> Router {
    mount(
        db.clone(),
        panel()
            .limits(limits)
            .resource_with::<TicketResource>(|def| def.policy(policy)),
    )
    .expect("panel builds")
}

fn bulk_close(db: &Db) -> String {
    url::bulk_action::<TicketResource, Close>(&panel_cx::<TicketResource>(db))
        .expect("the tickets declare Close")
}

#[tokio::test]
async fn every_matching_record_takes_the_action_past_the_page() {
    let db = db(&["bug 1", "bug 2", "bug 3", "feature"]).await;
    let router = router(&db, Allow, Limits::default());

    let html = body_string(get(&router, "/admin/tickets?q=bug").await).await;
    assert!(
        tablo::testing::selects_matching(&html),
        "three bugs over pages of two offer every match: {html}"
    );
    let html = body_string(get(&router, "/admin/tickets?q=feature").await).await;
    assert!(
        !tablo::testing::selects_matching(&html),
        "one page holds every match: {html}"
    );

    let reply = post_fields(&router, &bulk_close(&db), &[("matching", "q=bug")]).await;
    assert_eq!(reply.status(), 303, "the selection commits");
    assert_eq!(closed(&db).await, ["bug 1", "bug 2", "bug 3"]);
}

#[tokio::test]
async fn the_matching_query_overrides_the_posted_ids() {
    let db = db(&["bug 1", "feature"]).await;
    let router = router(&db, Allow, Limits::default());
    let mut conn = db.clone();
    let feature = Ticket::filter(Ticket::fields().title().eq("feature".to_string()))
        .first()
        .exec(&mut conn)
        .await
        .expect("query the feature")
        .expect("the feature exists");

    let reply = post_fields(
        &router,
        &bulk_close(&db),
        &[("ids", &feature.id.to_string()), ("matching", "q=bug")],
    )
    .await;
    assert_eq!(reply.status(), 303);
    assert_eq!(closed(&db).await, ["bug 1"]);
}

/// Every ability, but viewing the ticket titled "secret bug".
fn hides_secret(_cx: &Cx, ability: Ability<'_, Ticket>) -> bool {
    match ability {
        Ability::View(ticket) => ticket.title != "secret bug",
        _ => true,
    }
}

#[tokio::test]
async fn a_matching_record_its_user_may_not_view_is_skipped() {
    let db = db(&["bug 1", "secret bug"]).await;
    let router = router(&db, hides_secret, Limits::default());

    let reply = post_fields(&router, &bulk_close(&db), &[("matching", "q=bug")]).await;
    assert_eq!(
        reply.status(),
        303,
        "a hidden match is skipped, not a refusal of the whole selection"
    );
    assert_eq!(flash(&reply).status, NotificationStatus::Success);
    assert_eq!(closed(&db).await, ["bug 1"]);
}

#[tokio::test]
async fn more_matching_records_than_the_bulk_limit_write_nothing() {
    let db = db(&["bug 1", "bug 2", "bug 3"]).await;
    let router = router(&db, Allow, Limits::default().bulk_records(2));

    let reply = post_fields(&router, &bulk_close(&db), &[("matching", "q=bug")]).await;
    assert_eq!(reply.status(), 303, "the list explains the refusal");
    assert_eq!(flash(&reply).status, NotificationStatus::Error);
    assert!(closed(&db).await.is_empty(), "nothing was written");

    let reply = post_fields(&router, &bulk_close(&db), &[("matching", "q=bug 1")]).await;
    assert_eq!(reply.status(), 303, "a narrower list fits the limit");
    assert_eq!(closed(&db).await, ["bug 1"]);
}

/// Every ability but opening the list: viewing and running on each ticket stay allowed.
fn no_list(_cx: &Cx, ability: Ability<'_, Ticket>) -> bool {
    !matches!(ability, Ability::ViewAny)
}

#[tokio::test]
async fn a_user_who_may_not_open_the_list_may_not_select_its_matches() {
    let db = db(&["bug 1", "bug 2"]).await;
    let router = router(&db, no_list, Limits::default());

    let reply = post_fields(&router, &bulk_close(&db), &[("matching", "q=bug")]).await;
    assert_eq!(
        reply.status(),
        403,
        "re-running the list would count records its user may not list"
    );
    assert!(closed(&db).await.is_empty(), "nothing was written");
}

#[tokio::test]
async fn a_filter_the_list_ignores_refuses_the_selection() {
    let db = db(&["bug 1", "bug 2", "bug 3"]).await;
    let router = router(&db, Allow, Limits::default());

    let html = body_string(get(&router, "/admin/tickets?f.nope=1").await).await;
    assert!(
        !tablo::testing::selects_matching(&html),
        "a list ignoring a filter offers no selection its route refuses: {html}"
    );

    let reply = post_fields(&router, &bulk_close(&db), &[("matching", "f.nope=1")]).await;
    assert_eq!(
        reply.status(),
        400,
        "an ignored filter would widen the selection past what the list showed"
    );
    assert!(closed(&db).await.is_empty(), "nothing was written");
}

#[tokio::test]
async fn a_query_matching_nothing_writes_nothing() {
    let db = db(&["bug 1"]).await;
    let router = router(&db, Allow, Limits::default());

    let reply = post_fields(&router, &bulk_close(&db), &[("matching", "q=feature")]).await;
    assert_eq!(reply.status(), 303);
    assert_eq!(flash(&reply).status, NotificationStatus::Error);
    assert!(closed(&db).await.is_empty());
}
