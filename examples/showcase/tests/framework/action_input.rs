//! An action that asks for input: its POST renders the input page until a submission parses, then
//! runs with the typed value.

use tablo::{
    Ability, Action, ActionInput, DeclarationErrorKind, Policy, ReadOnly, Resource, ResourceDef,
    Site, Table, TextColumn, lens,
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
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
    reason: String,
    priority: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, tablo::Options)]
enum Outcome {
    Fixed,
    #[option(value = "wontfix", label = "Won't fix")]
    WontFix,
}

/// What closing asks for.
#[derive(ActionInput)]
struct Closing {
    #[form(multiline = 3, label = "Why")]
    reason: String,
    #[form(options)]
    outcome: Outcome,
    #[form(blank = 3)]
    priority: i64,
    reopenable: bool,
}

/// Closes tickets with the reason, outcome and priority it asks for.
struct Close;

impl Action<TicketResource> for Close {
    type Input = Closing;
    const NAME: &'static str = "close";

    fn label(_cx: &Cx) -> String {
        "Close".to_string()
    }

    fn can_run(_cx: &Cx, ticket: &Ticket) -> bool {
        !ticket.closed
    }

    async fn run(
        _cx: &Cx,
        tickets: &[Ticket],
        closing: Closing,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        let suffix = if closing.reopenable { "" } else { " (final)" };
        for ticket in tickets {
            Ticket::filter(Ticket::fields().id().eq(ticket.id))
                .update()
                .closed(true)
                .reason(format!(
                    "{}: {}{suffix}",
                    closing.outcome.value(),
                    closing.reason
                ))
                .priority(closing.priority)
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }
}

/// Closing asks first, on the input page rather than in the dialog.
struct Purge;

#[derive(ActionInput)]
struct Purging {
    #[form(optional)]
    note: String,
}

impl Action<TicketResource> for Purge {
    type Input = Purging;
    const NAME: &'static str = "purge";
    const CONFIRM: bool = true;

    fn label(_cx: &Cx) -> String {
        "Purge".to_string()
    }

    async fn run(
        _cx: &Cx,
        tickets: &[Ticket],
        purging: Purging,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        for ticket in tickets {
            Ticket::filter(Ticket::fields().id().eq(ticket.id))
                .update()
                .reason(format!("purged{}", purging.note))
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }
}

struct TicketResource;

impl Resource for TicketResource {
    type Model = Ticket;
    type Form = tablo::NoForm<Ticket>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("tickets")
            .policy(runs)
            .table(Table::new(TextColumn::new(lens!(Ticket.title))))
            .action::<Close>()
            .action::<Purge>()
    }
}

/// Lists tickets and runs their actions; no form, so no create or update.
fn runs(_cx: &Cx, ability: Ability<'_, Ticket>) -> bool {
    matches!(
        ability,
        Ability::ViewAny | Ability::View(_) | Ability::RunAny { .. } | Ability::Run { .. }
    )
}

async fn db() -> Db {
    memory_db(toasty::models!(Ticket)).await
}

async fn seed(db: &Db, title: &str) -> Ticket {
    let mut db = db.clone();
    toasty::create!(Ticket {
        title: title.to_string(),
        closed: false,
        reason: String::new(),
        priority: 0,
    })
    .exec(&mut db)
    .await
    .expect("seed ticket")
}

async fn ticket(db: &Db, id: Uuid) -> Ticket {
    let mut db = db.clone();
    Ticket::get_by_id(&mut db, &id)
        .await
        .expect("the ticket exists")
}

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
async fn the_button_post_renders_the_input_page_and_writes_nothing() {
    let db = db().await;
    let alpha = seed(&db, "Alpha").await;
    let router = router(&db, runs);

    let response = post_fields(&router, &row_close(&alpha), &[]).await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    for expected in [
        "<textarea",
        "name=\"reason\"",
        ">Why<",
        "name=\"outcome\"",
        "value=\"wontfix\"",
        "Won't fix",
        "name=\"priority\"",
        "name=\"reopenable\"",
        "name=\"-input\"",
    ] {
        assert!(html.contains(expected), "the page holds {expected}: {html}");
    }
    assert!(
        !html.contains("name=\"ids\""),
        "a row's page carries no selection: {html}"
    );
    assert!(!ticket(&db, alpha.id).await.closed, "nothing ran");
}

#[tokio::test]
async fn a_parsed_submission_runs_the_action_with_the_typed_input() {
    let db = db().await;
    let alpha = seed(&db, "Alpha").await;
    let router = router(&db, runs);

    let response = post_fields(
        &router,
        &row_close(&alpha),
        &[
            ("-input", "1"),
            ("reason", " duplicate "),
            ("outcome", "wontfix"),
            ("priority", ""),
            ("reopenable", "false"),
        ],
    )
    .await;
    assert_eq!(response.status(), 303);
    assert!(flash(&response).contains("Close: 1 record"));
    let closed = ticket(&db, alpha.id).await;
    assert!(closed.closed);
    assert_eq!(closed.reason, "wontfix: duplicate (final)");
    assert_eq!(closed.priority, 3, "an empty priority reads as its blank");
}

#[tokio::test]
async fn a_refused_submission_renders_the_page_again_and_writes_nothing() {
    let db = db().await;
    let alpha = seed(&db, "Alpha").await;
    let router = router(&db, runs);

    let response = post_fields(
        &router,
        &row_close(&alpha),
        &[
            ("-input", "1"),
            ("reason", ""),
            ("outcome", "lost"),
            ("priority", "high"),
        ],
    )
    .await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert!(html.contains("Why is required"), "{html}");
    assert!(
        html.contains("value=\"high\""),
        "the refused value stays in its control: {html}"
    );
    assert_eq!(
        html.matches("aria-invalid=\"true\"").count(),
        3,
        "the reason, the outcome and the priority are refused: {html}"
    );
    assert!(!ticket(&db, alpha.id).await.closed, "nothing ran");
}

#[tokio::test]
async fn a_key_the_input_does_not_declare_answers_400() {
    let db = db().await;
    let alpha = seed(&db, "Alpha").await;
    let router = router(&db, runs);

    let response = post_fields(
        &router,
        &row_close(&alpha),
        &[
            ("-input", "1"),
            ("reason", "x"),
            ("outcome", "fixed"),
            ("closed", "true"),
        ],
    )
    .await;
    assert_eq!(response.status(), 400);
    assert!(!ticket(&db, alpha.id).await.closed);
}

#[tokio::test]
async fn a_bulk_page_carries_the_selection_and_runs_on_it() {
    let db = db().await;
    let alpha = seed(&db, "Alpha").await;
    let bravo = seed(&db, "Bravo").await;
    let router = router(&db, runs);
    let ids = format!("{},{}", alpha.id, bravo.id);

    let response = post_fields(&router, BULK_CLOSE, &[("ids", &ids)]).await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert!(html.contains("Close: 2 records"), "{html}");
    assert!(
        html.contains(&format!("name=\"ids\" value=\"{ids}\"")),
        "the page carries the selection back: {html}"
    );

    let response = post_fields(
        &router,
        BULK_CLOSE,
        &[
            ("ids", &ids),
            ("-input", "1"),
            ("reason", "done"),
            ("outcome", "fixed"),
        ],
    )
    .await;
    assert_eq!(response.status(), 303);
    for id in [alpha.id, bravo.id] {
        assert_eq!(ticket(&db, id).await.reason, "fixed: done (final)");
    }
}

#[tokio::test]
async fn the_policy_refuses_before_the_page_renders() {
    let db = db().await;
    let alpha = seed(&db, "Alpha").await;
    let router = router(&db, ReadOnly);

    let response = post_fields(&router, &row_close(&alpha), &[]).await;
    assert_eq!(response.status(), 403);
    let response = post_fields(
        &router,
        &row_close(&alpha),
        &[("-input", "1"), ("reason", "x"), ("outcome", "fixed")],
    )
    .await;
    assert_eq!(response.status(), 403);
    assert!(!ticket(&db, alpha.id).await.closed);
}

#[tokio::test]
async fn a_record_can_run_refuses_gets_no_page() {
    let db = db().await;
    let alpha = seed(&db, "Alpha").await;
    let mut conn = db.clone();
    Ticket::filter(Ticket::fields().id().eq(alpha.id))
        .update()
        .closed(true)
        .exec(&mut conn)
        .await
        .unwrap();
    let router = router(&db, runs);

    let response = post_fields(&router, &row_close(&alpha), &[]).await;
    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn a_confirming_action_with_input_confirms_on_its_page() {
    let db = db().await;
    let alpha = seed(&db, "Alpha").await;
    let router = router(&db, runs);
    let url = format!("/admin/tickets/{}/-/actions/purge", alpha.id);

    let html = body_string(get(&router, "/admin/tickets").await).await;
    assert!(
        html.contains(&format!("formaction=\"{url}\"")),
        "the row offers Purge: {html}"
    );
    assert!(
        !html.contains("Run this action?"),
        "no action opens the dialog: Purge confirms on its page: {html}"
    );

    let page = body_string(post_fields(&router, &url, &[("confirm", "1")]).await).await;
    assert!(
        page.contains("name=\"confirm\" value=\"1\""),
        "the page's submit is the confirmation: {page}"
    );
    assert!(
        page.contains("bg-destructive"),
        "and renders destructive: {page}"
    );

    let response = post_fields(&router, &url, &[("-input", "1"), ("note", "")]).await;
    assert_eq!(response.status(), 400, "an unconfirmed submit is refused");
    let response = post_fields(
        &router,
        &url,
        &[("-input", "1"), ("confirm", "1"), ("note", "")],
    )
    .await;
    assert_eq!(response.status(), 303);
    assert_eq!(ticket(&db, alpha.id).await.reason, "purged");
}

/// An input field named like a key the action's POST carries.
#[derive(ActionInput)]
struct Selecting {
    #[allow(dead_code, reason = "only its name matters")]
    ids: String,
}

struct Select;

impl Action<SelectResource> for Select {
    type Input = Selecting;
    const NAME: &'static str = "select";

    fn label(_cx: &Cx) -> String {
        "Select".to_string()
    }

    async fn run(
        _: &Cx,
        _: &[Ticket],
        _: Selecting,
        _: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        Ok(())
    }
}

struct SelectResource;

impl Resource for SelectResource {
    type Model = Ticket;
    type Form = tablo::NoForm<Ticket>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(runs)
            .table(Table::new(TextColumn::new(lens!(Ticket.title))))
            .action::<Select>()
    }
}

#[tokio::test]
async fn an_input_field_named_like_a_transport_key_fails_the_build() {
    let errors = refusal(mount(db().await, panel().resource::<SelectResource>()));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].site, Site::Registration);
    assert_eq!(
        errors[0].kind,
        DeclarationErrorKind::ReservedActionInput {
            action: "select",
            field: "ids".to_string(),
        }
    );
}
