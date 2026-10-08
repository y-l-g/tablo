//! An action that asks for input: its POST renders the input page until a submission parses, then
//! runs with the typed value.

use std::collections::HashMap;

use tablo::{
    Ability, Action, ActionInput, ActionInputFault, DeclarationErrorKind, Field, FieldError,
    FieldErrors, NotificationStatus, Policy, ReadOnly, Resource, ResourceDef, Schema, Site, Table,
    TextColumn, lens, testing::field_error,
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::framework::common::{
    body_string, confirms_first, flash, get, memory_db, mount, panel, post_fields, refusal,
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

    fn validate_input(_cx: &Cx, closing: &Closing) -> FieldErrors {
        let mut errors = FieldErrors::new();
        if closing.reason.trim().len() < 3 {
            errors.add("reason", "Say why in a few words");
        }
        errors
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

/// Merges a ticket into another, chosen from the tickets themselves: a relationship choice, whose
/// options query the database.
struct Merge;

/// A hand-written input: the ticket to merge into.
struct Merging {
    into: String,
}

impl ActionInput for Merging {
    fn schema() -> Schema {
        Schema::new(Field::choice_input("into").relationship::<TicketResource>())
    }

    fn parse(_cx: &Cx, values: &HashMap<String, String>) -> Result<Self, Vec<FieldError>> {
        let into =
            tablo::form::parse_scalar::<String>("into", values, None).map_err(|e| vec![e])?;
        Ok(Self { into })
    }
}

impl Action<TicketResource> for Merge {
    type Input = Merging;
    const NAME: &'static str = "merge";

    fn label(_cx: &Cx) -> String {
        "Merge".to_string()
    }

    async fn run(
        _cx: &Cx,
        tickets: &[Ticket],
        merging: Merging,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        for ticket in tickets {
            Ticket::filter(Ticket::fields().id().eq(ticket.id))
                .update()
                .reason(format!("merged into {}", merging.into))
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
            .action::<Merge>()
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
    assert_eq!(flash(&response).status, NotificationStatus::Success);
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
    assert!(field_error(&html, "reason").is_some(), "{html}");
    assert!(field_error(&html, "outcome").is_some(), "{html}");
    assert!(field_error(&html, "priority").is_some(), "{html}");
    assert!(
        html.contains("value=\"high\""),
        "the refused value stays in its control: {html}"
    );
    assert!(!ticket(&db, alpha.id).await.closed, "nothing ran");
}

#[tokio::test]
async fn validate_input_refuses_a_parsed_value_on_the_page() {
    let db = db().await;
    let alpha = seed(&db, "Alpha").await;
    let router = router(&db, runs);

    let response = post_fields(
        &router,
        &row_close(&alpha),
        &[("-input", "1"), ("reason", "ok"), ("outcome", "fixed")],
    )
    .await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert!(field_error(&html, "reason").is_some(), "{html}");
    assert!(field_error(&html, "outcome").is_none(), "{html}");
    assert!(!ticket(&db, alpha.id).await.closed, "nothing ran");
}

#[tokio::test]
async fn a_row_page_names_the_record_and_keeps_the_return_target() {
    let db = db().await;
    let alpha = seed(&db, "Alpha").await;
    let router = router(&db, runs);
    let url = format!("{}?return=%2Fadmin%2Ftickets%3Fq%3Dal", row_close(&alpha));

    let html = body_string(post_fields(&router, &url, &[]).await).await;
    assert!(
        html.contains(&format!("Close: Ticket {}", alpha.id)),
        "the title names the record: {html}"
    );
    assert!(
        html.contains(&format!("action=\"{}?return=", row_close(&alpha))),
        "the submit keeps the return target: {html}"
    );
}

#[tokio::test]
async fn a_bulk_page_counts_the_records_it_skips() {
    let db = db().await;
    let alpha = seed(&db, "Alpha").await;
    let bravo = seed(&db, "Bravo").await;
    let mut conn = db.clone();
    Ticket::filter(Ticket::fields().id().eq(bravo.id))
        .update()
        .closed(true)
        .exec(&mut conn)
        .await
        .unwrap();
    let router = router(&db, runs);
    let ids = format!("{},{}", alpha.id, bravo.id);

    let html = body_string(post_fields(&router, BULK_CLOSE, &[("ids", &ids)]).await).await;
    assert!(html.contains("Close: 1 record (1 of 2 skipped)"), "{html}");
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
    assert_eq!(
        confirms_first(&html, &url),
        Some(false),
        "the row offers Purge without the dialog: Purge confirms on its page: {html}"
    );

    let response = post_fields(&router, &url, &[]).await;
    assert_eq!(
        response.status(),
        200,
        "opening the page writes nothing, so needs no marker"
    );
    let page = body_string(response).await;
    assert!(
        page.contains("name=\"confirm\" value=\"1\""),
        "the page's submit is the confirmation: {page}"
    );
    assert!(page.contains("cannot be undone"), "{page}");
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

/// A hand-written input holding a file field and a choice offering nothing.
struct Attaching;

impl ActionInput for Attaching {
    fn schema() -> Schema {
        Schema::new((
            Field::file(lens!(Ticket.reason)),
            Field::choice(lens!(Ticket.title)),
        ))
    }

    fn parse(_cx: &Cx, _values: &HashMap<String, String>) -> Result<Self, Vec<FieldError>> {
        Ok(Self)
    }
}

/// A hand-written input with no field whose parse still requires one.
struct Strict;

impl ActionInput for Strict {
    fn schema() -> Schema {
        Schema::empty()
    }

    fn parse(_cx: &Cx, _values: &HashMap<String, String>) -> Result<Self, Vec<FieldError>> {
        Err(vec![FieldError::required("note")])
    }
}

struct Attach;

impl Action<SelectResource> for Attach {
    type Input = Attaching;
    const NAME: &'static str = "attach";

    fn label(_cx: &Cx) -> String {
        "Attach".to_string()
    }

    async fn run(
        _: &Cx,
        _: &[Ticket],
        _: Attaching,
        _: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        Ok(())
    }
}

struct Refuse;

impl Action<SelectResource> for Refuse {
    type Input = Strict;
    const NAME: &'static str = "refuse";

    fn label(_cx: &Cx) -> String {
        "Refuse".to_string()
    }

    async fn run(
        _: &Cx,
        _: &[Ticket],
        _: Strict,
        _: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        Ok(())
    }
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
            .action::<Attach>()
            .action::<Refuse>()
    }
}

#[tokio::test]
async fn an_input_that_cannot_be_served_fails_the_build() {
    let errors = refusal(mount(db().await, panel().resource::<SelectResource>()));
    assert!(
        errors.iter().all(|error| error.site == Site::Registration),
        "{errors:?}"
    );
    let kinds: Vec<_> = errors.into_iter().map(|error| error.kind).collect();
    assert_eq!(
        kinds,
        [
            DeclarationErrorKind::ActionInput {
                action: "select",
                fault: ActionInputFault::ReservedField("ids".to_string()),
            },
            DeclarationErrorKind::EmptyChoice {
                field: "title".to_string(),
            },
            DeclarationErrorKind::ActionInput {
                action: "attach",
                fault: ActionInputFault::FileField("reason".to_string()),
            },
            DeclarationErrorKind::ActionInput {
                action: "refuse",
                fault: ActionInputFault::RefusesEmpty,
            },
        ]
    );
}

/// The in-memory database holds one connection: a choice whose options query it while the action's
/// transaction holds that connection would wait forever, so the submit must check first.
#[tokio::test]
async fn a_relationship_choice_is_checked_before_the_transaction_and_again_inside_it() {
    let db = db().await;
    let alpha = seed(&db, "Alpha").await;
    let bravo = seed(&db, "Bravo").await;
    let router = router(&db, runs);
    let url = format!("/admin/tickets/{}/-/actions/merge", alpha.id);
    let within = std::time::Duration::from_secs(5);

    let page = tokio::time::timeout(within, post_fields(&router, &url, &[]))
        .await
        .expect("the page renders without waiting on a connection");
    let html = body_string(page).await;
    assert!(html.contains(&format!("value=\"{}\"", bravo.id)), "{html}");

    let unknown = Uuid::new_v4().to_string();
    let refused = tokio::time::timeout(
        within,
        post_fields(&router, &url, &[("-input", "1"), ("into", &unknown)]),
    )
    .await
    .expect("a refused choice answers without waiting on a connection");
    assert_eq!(refused.status(), 200);
    let html = body_string(refused).await;
    assert!(field_error(&html, "into").is_some(), "{html}");

    let into = bravo.id.to_string();
    let response = tokio::time::timeout(
        within,
        post_fields(&router, &url, &[("-input", "1"), ("into", &into)]),
    )
    .await
    .expect("the submit runs without waiting on a connection");
    assert_eq!(response.status(), 303);
    assert_eq!(
        ticket(&db, alpha.id).await.reason,
        format!("merged into {into}")
    );
}
