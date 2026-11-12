//! A list header action's button badged with the count of the records a filter matches.

use tablo::{
    Allow, HeaderAction, HeaderActionDef, NoForm, Resource, ResourceDef, Table, TextColumn, lens,
};
use topcoat::context::Cx;

use crate::{
    common::{action_badge, action_label, body_string, get, mount, panel},
    header_action::{Ticket, db, seed},
};

/// Drains the open tickets, its button badged with how many there are.
struct Drain;

impl HeaderAction for Drain {
    type Input = ();
    const NAME: &'static str = "drain";

    fn declare() -> HeaderActionDef {
        HeaderActionDef::new().label("Drain")
    }

    async fn run(
        _: &Cx,
        _: (),
        _: &mut dyn toasty::Executor,
    ) -> topcoat::Result<tablo::Outcome, tablo::WriteError<std::convert::Infallible>> {
        Ok(tablo::Outcome::done())
    }
}

/// The tickets as a queue, badging its drain with the open ones.
struct QueueResource;

impl Resource for QueueResource {
    type Model = Ticket;
    type Form = NoForm<Ticket>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("queue")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Ticket.title))))
            .header_action_with_badge::<Drain>(Ticket::fields().closed().eq(false))
    }
}

#[tokio::test]
async fn a_header_button_badges_the_count_its_filter_matches() {
    let db = db().await;
    seed(&db, "Open one", false).await;
    seed(&db, "Open two", false).await;
    seed(&db, "Done", true).await;
    let router = mount(db.clone(), panel().resource::<QueueResource>()).expect("panel builds");

    let html = body_string(get(&router, "/admin/queue").await).await;
    assert_eq!(
        action_badge(&html, "/admin/queue/-/actions/drain").as_deref(),
        Some("2"),
        "the drain's button shows the two open tickets: {html}"
    );
}

#[tokio::test]
async fn a_header_button_shows_no_badge_for_a_zero_count() {
    let db = db().await;
    seed(&db, "Done", true).await;
    let router = mount(db.clone(), panel().resource::<QueueResource>()).expect("panel builds");

    let html = body_string(get(&router, "/admin/queue").await).await;
    assert_eq!(
        action_badge(&html, "/admin/queue/-/actions/drain"),
        None,
        "no open ticket, no badge: {html}"
    );
    assert!(
        action_label(&html, "/admin/queue/-/actions/drain").is_some(),
        "the drain still shows: {html}"
    );
}
