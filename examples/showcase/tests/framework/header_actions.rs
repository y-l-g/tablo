//! Actions outside the table: a list's and a page's header actions, which act on no record, and a
//! record's actions and Delete in the header of its detail and edit pages.

use http::header::LOCATION;
use tablo::{
    Ability, Action, Allow, Committed, DeclarationErrorKind, Detail, FieldErrors, HeaderAction,
    HeaderActions, Mutation, NotificationStatus, Page, Places, Policy, Resource, ResourceDef, Site,
    Table, TextColumn, header_action_buttons, lens,
};
use toasty::Db;
use topcoat::{
    context::Cx,
    router::{Body, Router},
    view::{View, view},
};
use uuid::Uuid;

use crate::framework::common::{
    body_string, flash, get, memory_db, mount, panel, post_fields, refusal,
};

#[derive(Debug, toasty::Model, Clone)]
struct Ticket {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    closed: bool,
}

/// What `after_commit` saw, one row per committed write.
#[derive(Debug, toasty::Model, Clone)]
struct Log {
    #[key]
    #[auto]
    id: Uuid,
    mutation: String,
    records: i64,
}

/// Closes a ticket, wherever a record shows.
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

/// Reopens a ticket from its row only.
struct Reopen;

impl Action<TicketResource> for Reopen {
    type Input = ();
    const NAME: &'static str = "reopen";
    const PLACES: Places = Places::ROW;

    async fn run(_: &Cx, _: &[Ticket], _: (), _: &mut dyn toasty::Executor) -> topcoat::Result<()> {
        Ok(())
    }
}

/// Retitles the selection, from the bulk bar only.
struct Retitle;

impl Action<TicketResource> for Retitle {
    type Input = ();
    const NAME: &'static str = "retitle";
    const PLACES: Places = Places::BULK;

    async fn run(
        _cx: &Cx,
        tickets: &[Ticket],
        _: (),
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        for ticket in tickets {
            Ticket::filter(Ticket::fields().id().eq(ticket.id))
                .update()
                .title("retitled".to_string())
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }
}

/// Closes every ticket, confirming first.
struct CloseAll;

impl HeaderAction for CloseAll {
    type Input = ();
    const NAME: &'static str = "close-all";
    const CONFIRM: bool = true;

    async fn run(_cx: &Cx, _: (), ex: &mut dyn toasty::Executor) -> topcoat::Result<()> {
        Ticket::filter(Ticket::fields().closed().eq(false))
            .update()
            .closed(true)
            .exec(&mut *ex)
            .await?;
        Ok(())
    }
}

/// What opening a ticket asks for.
#[derive(tablo::ActionInput)]
struct Opening {
    title: String,
}

/// Opens a ticket, asking for its title.
struct OpenTicket;

impl HeaderAction for OpenTicket {
    type Input = Opening;
    const NAME: &'static str = "open-ticket";

    fn validate_input(_cx: &Cx, opening: &Opening) -> FieldErrors {
        let mut errors = FieldErrors::new();
        if opening.title == "bad" {
            errors.add("title", "Not that title");
        }
        errors
    }

    async fn run(_cx: &Cx, opening: Opening, ex: &mut dyn toasty::Executor) -> topcoat::Result<()> {
        toasty::create!(Ticket {
            title: opening.title,
            closed: false,
        })
        .exec(&mut *ex)
        .await?;
        Ok(())
    }
}

/// An action its own `can_run` refuses to every request.
struct Flagged;

impl HeaderAction for Flagged {
    type Input = ();
    const NAME: &'static str = "flagged";

    fn can_run(_cx: &Cx) -> bool {
        false
    }

    async fn run(_: &Cx, _: (), _: &mut dyn toasty::Executor) -> topcoat::Result<()> {
        Ok(())
    }
}

/// Writes, then fails: nothing it wrote may survive.
struct Explode;

impl HeaderAction for Explode {
    type Input = ();
    const NAME: &'static str = "explode";

    async fn run(_cx: &Cx, _: (), ex: &mut dyn toasty::Executor) -> topcoat::Result<()> {
        Ticket::filter(Ticket::fields().closed().eq(false))
            .update()
            .title("exploded".to_string())
            .exec(&mut *ex)
            .await?;
        Err(std::io::Error::other("the action refused itself").into())
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
            .view(Detail::new(TextColumn::new(lens!(Ticket.title))))
            .action::<Close>()
            .action::<Reopen>()
            .action::<Retitle>()
            .header_action::<CloseAll>()
            .header_action::<OpenTicket>()
            .header_action::<Flagged>()
            .header_action::<Explode>()
    }

    async fn after_commit(cx: &Cx, committed: Committed<Ticket>) -> topcoat::Result<()> {
        let Mutation::Action(name) = committed.mutation() else {
            return Ok(());
        };
        toasty::create!(Log {
            mutation: name.to_string(),
            records: committed.records().len() as i64,
        })
        .exec(&mut tablo::db::db(cx))
        .await?;
        Ok(())
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Ticket)]
struct TicketForm {
    title: String,
    #[form(blank = false)]
    closed: bool,
}

/// Deletes the closed tickets, from a page's header.
struct Sweep;

impl HeaderAction for Sweep {
    type Input = ();
    const NAME: &'static str = "sweep";

    async fn run(_cx: &Cx, _: (), ex: &mut dyn toasty::Executor) -> topcoat::Result<()> {
        Ticket::filter(Ticket::fields().closed().eq(true))
            .delete()
            .exec(&mut *ex)
            .await?;
        Ok(())
    }
}

/// A page whose header carries `Sweep`.
struct OpsPage;

impl Page for OpsPage {
    fn header_actions() -> HeaderActions {
        HeaderActions::new().add::<Sweep>().add::<Flagged>()
    }

    async fn render(cx: &Cx) -> topcoat::Result<impl View> {
        Ok(view! {
            cx =>
            tablo::ui::page(
                tablo::ui::page_header(
                    tablo::ui::page_title("Ops")
                    tablo::ui::page_actions((header_action_buttons::<Self>(cx)))
                )
            )
        })
    }
}

/// The same action behind a page no request may open.
struct LockedPage;

impl Page for LockedPage {
    fn can_access(_cx: &Cx) -> bool {
        false
    }

    fn header_actions() -> HeaderActions {
        HeaderActions::new().add::<Sweep>()
    }

    async fn render(cx: &Cx) -> topcoat::Result<impl View> {
        Ok(view! { cx => tablo::ui::page(tablo::ui::page_title("Locked")) })
    }
}

/// A page with no action.
struct PlainPage;

impl Page for PlainPage {
    async fn render(cx: &Cx) -> topcoat::Result<impl View> {
        Ok(view! { cx => tablo::ui::page(tablo::ui::page_title("Plain")) })
    }
}

async fn db() -> Db {
    memory_db(toasty::models!(Ticket, Log)).await
}

async fn seed(db: &Db, title: &str, closed: bool) -> Ticket {
    toasty::create!(Ticket {
        title: title.to_string(),
        closed,
    })
    .exec(&mut db.clone())
    .await
    .expect("seed ticket")
}

async fn tickets(db: &Db) -> Vec<Ticket> {
    Ticket::all()
        .exec(&mut db.clone())
        .await
        .expect("load tickets")
}

async fn logs(db: &Db) -> Vec<(String, i64)> {
    Log::all()
        .exec(&mut db.clone())
        .await
        .expect("load logs")
        .into_iter()
        .map(|log| (log.mutation, log.records))
        .collect()
}

fn router(db: &Db, policy: impl Policy<Ticket>) -> Router {
    mount(
        db.clone(),
        panel()
            .resource_with::<TicketResource>(|def| def.policy(policy))
            .page::<OpsPage>()
            .page::<LockedPage>()
            .page::<PlainPage>(),
    )
    .expect("panel builds")
}

fn location(response: &http::Response<Body>) -> &str {
    response
        .headers()
        .get(LOCATION)
        .expect("the write redirects")
        .to_str()
        .expect("the location is ASCII")
}

fn form_action(url: &str) -> String {
    format!("<form method=\"post\" action=\"{url}\"")
}

#[tokio::test]
async fn the_list_header_offers_each_header_action_the_request_may_run() {
    let db = db().await;
    let html = body_string(get(&router(&db, Allow), "/admin/tickets").await).await;
    for name in ["close-all", "explode"] {
        assert!(
            html.contains(&form_action(&format!("/admin/tickets/-/actions/{name}"))),
            "the header offers {name}: {html}"
        );
    }
    assert!(
        html.contains("action=\"/admin/tickets/-/actions/open-ticket\""),
        "the header offers open-ticket, whose input dialog posts to its route: {html}"
    );
    assert!(
        !html.contains("/admin/tickets/-/actions/flagged"),
        "an action its `can_run` refuses has no button: {html}"
    );
}

#[tokio::test]
async fn a_confirming_header_action_runs_only_confirmed_and_commits_with_no_record() {
    let db = db().await;
    let open = seed(&db, "Open", false).await;
    let router = router(&db, Allow);

    let unconfirmed = post_fields(&router, "/admin/tickets/-/actions/close-all", &[]).await;
    assert_eq!(unconfirmed.status(), 400, "an unconfirmed POST is refused");
    assert!(!tickets(&db).await[0].closed, "nothing was written");

    let response = post_fields(
        &router,
        "/admin/tickets/-/actions/close-all",
        &[("confirm", "1")],
    )
    .await;
    assert_eq!(response.status(), 303);
    assert_eq!(
        location(&response),
        "/admin/tickets",
        "it lands on the list"
    );
    assert_eq!(flash(&response).status, NotificationStatus::Success);
    let ticket = &tickets(&db).await[0];
    assert!(ticket.id == open.id && ticket.closed, "the action wrote");
    assert_eq!(
        logs(&db).await,
        vec![("close-all".to_string(), 0)],
        "after_commit sees the action's name and no record"
    );
}

#[tokio::test]
async fn a_header_action_with_input_asks_for_it_then_runs_with_it() {
    let db = db().await;
    let router = router(&db, Allow);
    let url = "/admin/tickets/-/actions/open-ticket";

    let page = post_fields(&router, url, &[]).await;
    assert_eq!(
        page.status(),
        200,
        "an empty submission renders the input page"
    );
    let html = body_string(page).await;
    assert!(
        html.contains("name=\"title\"") && html.contains(&format!("action=\"{url}\"")),
        "the page asks for the title and posts back here: {html}"
    );
    assert!(tickets(&db).await.is_empty(), "nothing ran");

    let refused = post_fields(&router, url, &[("title", "bad")]).await;
    assert_eq!(
        refused.status(),
        200,
        "a refused value renders the page again"
    );
    let html = body_string(refused).await;
    assert!(
        tablo::testing::field_error(&html, "title").is_some(),
        "with its error under the field: {html}"
    );
    assert!(tickets(&db).await.is_empty(), "and writes nothing");

    let ran = post_fields(&router, url, &[("title", "Fresh")]).await;
    assert_eq!(ran.status(), 303);
    let all = tickets(&db).await;
    assert_eq!(all.len(), 1);
    assert_eq!(
        all[0].title, "Fresh",
        "the action ran with the parsed input"
    );
}

/// Allows everything but running `CloseAll`.
fn no_close_all(_cx: &Cx, ability: Ability<'_, Ticket>) -> bool {
    !ability.is_header_action::<CloseAll>()
}

#[tokio::test]
async fn a_header_action_the_policy_or_its_can_run_refuses_has_no_button_and_answers_403() {
    let db = db().await;
    seed(&db, "Open", false).await;
    let router = router(&db, no_close_all);

    let html = body_string(get(&router, "/admin/tickets").await).await;
    assert!(
        !html.contains("/admin/tickets/-/actions/close-all"),
        "the refused action has no button: {html}"
    );
    assert!(
        html.contains("/admin/tickets/-/actions/open-ticket"),
        "another one keeps its own: {html}"
    );
    for name in ["close-all", "flagged"] {
        let response = post_fields(
            &router,
            &format!("/admin/tickets/-/actions/{name}"),
            &[("confirm", "1")],
        )
        .await;
        assert_eq!(response.status(), 403, "{name} is refused");
    }
    assert!(!tickets(&db).await[0].closed, "nothing was written");
}

/// Allows everything a record action asks, and the list, but no header action: `RunAny` does not
/// stand for `RunHeader`.
fn record_actions_only(_cx: &Cx, ability: Ability<'_, Ticket>) -> bool {
    !matches!(ability, Ability::RunHeader { .. })
}

/// Allows every action but not the list.
fn no_list(_cx: &Cx, ability: Ability<'_, Ticket>) -> bool {
    !matches!(ability, Ability::ViewAny)
}

#[tokio::test]
async fn a_header_action_needs_run_header_and_the_list() {
    let db = db().await;
    let open = seed(&db, "Open", false).await;
    for (router, why) in [
        (
            router(&db, record_actions_only),
            "`RunAny` grants no header action",
        ),
        (
            router(&db, no_list),
            "a user who may not open the list runs none of its actions",
        ),
    ] {
        let response = post_fields(
            &router,
            "/admin/tickets/-/actions/close-all",
            &[("confirm", "1")],
        )
        .await;
        assert_eq!(response.status(), 403, "{why}");
    }
    let row = post_fields(
        &router(&db, record_actions_only),
        &format!("/admin/tickets/{}/-/actions/close", open.id),
        &[],
    )
    .await;
    assert_eq!(row.status(), 303, "the record action still runs");
    let html = body_string(get(&router(&db, record_actions_only), "/admin/tickets").await).await;
    assert!(
        !html.contains("/admin/tickets/-/actions/close-all"),
        "and the list offers no header button: {html}"
    );
}

#[tokio::test]
async fn a_failing_header_action_rolls_its_writes_back() {
    let db = db().await;
    seed(&db, "Open", false).await;
    let response = post_fields(&router(&db, Allow), "/admin/tickets/-/actions/explode", &[]).await;
    assert!(response.status().is_server_error(), "{}", response.status());
    assert_eq!(tickets(&db).await[0].title, "Open", "the write rolled back");
    assert!(logs(&db).await.is_empty(), "after_commit never ran");
}

#[tokio::test]
async fn a_page_renders_and_runs_its_header_actions() {
    let db = db().await;
    seed(&db, "Done", true).await;
    seed(&db, "Open", false).await;
    let router = router(&db, Allow);

    let html = body_string(get(&router, "/admin/ops").await).await;
    assert!(
        html.contains(&form_action("/admin/ops/-/actions/sweep")),
        "the page renders its action's button: {html}"
    );
    assert!(
        !html.contains("/actions/flagged"),
        "and none for an action its `can_run` refuses: {html}"
    );

    let response = post_fields(&router, "/admin/ops/-/actions/sweep", &[]).await;
    assert_eq!(response.status(), 303);
    assert_eq!(location(&response), "/admin/ops", "it lands on the page");
    let left: Vec<String> = tickets(&db).await.into_iter().map(|t| t.title).collect();
    assert_eq!(left, ["Open"], "the action ran");

    let flagged = post_fields(&router, "/admin/ops/-/actions/flagged", &[]).await;
    assert_eq!(flagged.status(), 403, "`can_run` refuses the POST too");
    let unknown = post_fields(&router, "/admin/ops/-/actions/nope", &[]).await;
    assert_eq!(unknown.status(), 404);
}

#[tokio::test]
async fn a_page_header_action_answers_403_where_the_page_does() {
    let db = db().await;
    seed(&db, "Done", true).await;
    let router = router(&db, Allow);
    let response = post_fields(&router, "/admin/locked/-/actions/sweep", &[]).await;
    assert_eq!(response.status(), 403);
    assert_eq!(tickets(&db).await.len(), 1, "nothing was deleted");

    let plain = post_fields(&router, "/admin/plain/-/actions/sweep", &[]).await;
    assert!(
        plain.status().is_client_error(),
        "a page with no action serves no action route: {}",
        plain.status()
    );
}

/// A header action's input searches its own options route, behind the checks its POST makes; past
/// them, the route answers only for the input's own choices.
#[tokio::test]
async fn a_header_actions_options_route_asks_what_its_post_asks() {
    let db = db().await;
    let router = router(&db, Allow);
    for (url, status) in [
        (
            "/admin/tickets/-/actions/open-ticket/options?field=title",
            400,
        ),
        ("/admin/tickets/-/actions/flagged/options?field=title", 403),
        ("/admin/ops/-/actions/sweep/options?field=title", 400),
        ("/admin/ops/-/actions/flagged/options?field=title", 403),
        ("/admin/locked/-/actions/sweep/options?field=title", 403),
        ("/admin/ops/-/actions/unknown/options?field=title", 404),
    ] {
        assert_eq!(get(&router, url).await.status(), status, "{url}");
    }
}

#[tokio::test]
async fn the_detail_page_offers_the_records_actions_placed_there_and_its_delete() {
    let db = db().await;
    let open = seed(&db, "Open", false).await;
    let closed = seed(&db, "Closed", true).await;
    let router = router(&db, Allow);
    let page = format!("/admin/tickets/{}", open.id);

    let html = body_string(get(&router, &page).await).await;
    let back = format!("?return=%2Fadmin%2Ftickets%2F{}", open.id);
    assert!(
        html.contains(&form_action(&format!(
            "/admin/tickets/{}/-/actions/close{back}",
            open.id
        ))),
        "Close lands back on the page: {html}"
    );
    assert!(
        !html.contains("/-/actions/reopen") && !html.contains("/-/actions/retitle"),
        "an action placed elsewhere has no button: {html}"
    );
    assert!(
        html.contains(&format!("action=\"/admin/tickets/{}/delete\"", open.id)),
        "Delete posts to the record's route: {html}"
    );

    let html = body_string(get(&router, &format!("/admin/tickets/{}", closed.id)).await).await;
    assert!(
        !html.contains("/-/actions/close"),
        "a record `can_run` refuses has no button: {html}"
    );

    let ran = post_fields(
        &router,
        &format!("/admin/tickets/{}/-/actions/close{back}", open.id),
        &[],
    )
    .await;
    assert_eq!(ran.status(), 303);
    assert_eq!(
        location(&ran),
        page,
        "the action lands back on the detail page"
    );

    let deleted = post_fields(
        &router,
        &format!("/admin/tickets/{}/delete", open.id),
        &[("confirm", "1")],
    )
    .await;
    assert_eq!(deleted.status(), 303);
    assert_eq!(
        location(&deleted),
        "/admin/tickets",
        "Delete lands on the list"
    );
    assert_eq!(tickets(&db).await.len(), 1);
}

#[tokio::test]
async fn the_edit_page_offers_the_records_actions_and_its_delete() {
    let db = db().await;
    let open = seed(&db, "Open", false).await;
    let page = format!("/admin/tickets/{}/edit", open.id);
    let html = body_string(get(&router(&db, Allow), &page).await).await;
    assert!(
        html.contains(&form_action(&format!(
            "/admin/tickets/{}/-/actions/close?return=%2Fadmin%2Ftickets%2F{}%2Fedit",
            open.id, open.id
        ))),
        "Close lands back on the edit page: {html}"
    );
    assert!(
        html.contains(&format!("action=\"/admin/tickets/{}/delete\"", open.id)),
        "the edit page offers Delete: {html}"
    );
}

/// Allows everything but deleting.
fn no_delete(_cx: &Cx, ability: Ability<'_, Ticket>) -> bool {
    !matches!(ability, Ability::DeleteAny | Ability::Delete(_))
}

#[tokio::test]
async fn a_record_the_policy_may_not_delete_has_no_delete_button() {
    let db = db().await;
    let open = seed(&db, "Open", false).await;
    let html = body_string(
        get(
            &router(&db, no_delete),
            &format!("/admin/tickets/{}", open.id),
        )
        .await,
    )
    .await;
    assert!(!html.contains("/delete\""), "no Delete: {html}");
}

#[tokio::test]
async fn each_route_serves_only_the_actions_placed_on_it() {
    let db = db().await;
    let open = seed(&db, "Open", false).await;
    let router = router(&db, Allow);

    let record = post_fields(
        &router,
        &format!("/admin/tickets/{}/-/actions/retitle", open.id),
        &[],
    )
    .await;
    assert_eq!(
        record.status(),
        404,
        "a bulk-only action has no record route"
    );

    let ids = open.id.to_string();
    let bulk = post_fields(&router, "/admin/tickets/-/actions/reopen", &[("ids", &ids)]).await;
    assert_eq!(bulk.status(), 404, "a row-only action has no bulk route");
    assert_eq!(tickets(&db).await[0].title, "Open");
}

/// A header action named like one of the resource's record actions.
struct CloseHeader;

impl HeaderAction for CloseHeader {
    type Input = ();
    const NAME: &'static str = "close";

    async fn run(_: &Cx, _: (), _: &mut dyn toasty::Executor) -> topcoat::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn a_header_action_sharing_a_record_actions_name_fails_the_mount() {
    let errors = refusal(mount(
        db().await,
        panel().resource_with::<TicketResource>(|def| def.header_action::<CloseHeader>()),
    ));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].site, Site::Registration);
    assert_eq!(
        errors[0].kind,
        DeclarationErrorKind::DuplicateAction { name: "close" }
    );
}

/// A page declaring one action twice.
struct TwicePage;

impl Page for TwicePage {
    fn header_actions() -> HeaderActions {
        HeaderActions::new().add::<Sweep>().add::<Sweep>()
    }

    async fn render(cx: &Cx) -> topcoat::Result<impl View> {
        Ok(view! { cx => tablo::ui::page(tablo::ui::page_title("Twice")) })
    }
}

#[tokio::test]
async fn a_page_declaring_one_action_twice_fails_the_mount() {
    let errors = refusal(mount(db().await, panel().page::<TwicePage>()));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].resource, Some(std::any::type_name::<TwicePage>()));
    assert_eq!(
        errors[0].kind,
        DeclarationErrorKind::DuplicateAction { name: "sweep" }
    );
}
