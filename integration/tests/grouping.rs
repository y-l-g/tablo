//! A grouped list over pages: a grouping orders the query by its field, so each group runs on
//! across pages under numbered and cursor pagination alike.

use tablo::{
    NoForm, Pagination, Resource, ResourceDef, Table, TextColumn, lens,
    testing::{find_href_with, groups},
};
use toasty::Db;
use uuid::Uuid;

use crate::common::{body_string, get, mount, panel, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Ticket {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    team: String,
}

fn tickets_table() -> Table<Ticket> {
    Table::new(TextColumn::new(lens!(Ticket.title)).sortable())
        .group_by(lens!(Ticket.team))
        .group_by(lens!(Ticket.title))
        .page_size(2)
        .per_page_options([])
}

/// The tickets, two to a numbered page.
struct PagedResource;

impl Resource for PagedResource {
    type Model = Ticket;
    type Form = NoForm<Ticket>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("tickets")
            .policy(tablo::Allow)
            .table(tickets_table())
    }
}

/// The tickets, two to a cursor page.
struct CursoredResource;

impl Resource for CursoredResource {
    type Model = Ticket;
    type Form = NoForm<Ticket>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("tickets")
            .policy(tablo::Allow)
            .table(tickets_table().pagination(Pagination::Cursor))
    }
}

/// Five tickets whose title order interleaves their teams.
async fn tickets() -> Db {
    let mut db = test_db(toasty::models!(Ticket)).await;
    for (title, team) in [
        ("Alpha", "ops"),
        ("Bravo", "web"),
        ("Charlie", "ops"),
        ("Delta", "web"),
        ("Echo", "ops"),
    ] {
        toasty::create!(Ticket {
            title: title.to_string(),
            team: team.to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    db
}

/// The groups of the page at `uri`, each label with its rows' titles.
async fn grouped(
    router: &topcoat::router::Router,
    uri: &str,
) -> (Vec<(String, Vec<String>)>, String) {
    let html = body_string(get(router, uri).await).await;
    let groups = groups(&html)
        .into_iter()
        .map(|group| {
            let titles = group
                .rows
                .into_iter()
                .filter_map(|row| row.cells.into_iter().find(|cell| !cell.is_empty()))
                .collect();
            (group.label, titles)
        })
        .collect();
    (groups, html)
}

fn group(label: &str, titles: &[&str]) -> (String, Vec<String>) {
    (
        label.to_string(),
        titles.iter().map(|title| title.to_string()).collect(),
    )
}

/// Each team's tickets run together across numbered pages, sorted within the team, and a page
/// a group runs on to heads its rows with the group's label again.
#[tokio::test]
async fn a_group_runs_on_across_numbered_pages() {
    let router = mount(tickets().await, panel().resource::<PagedResource>()).expect("panel builds");
    let mut pages = Vec::new();
    for page in 1..=3 {
        let uri = format!("/admin/tickets?group_by=team&page={page}");
        pages.push(grouped(&router, &uri).await.0);
    }
    assert_eq!(
        pages,
        [
            vec![group("ops", &["Alpha", "Charlie"])],
            vec![group("ops", &["Echo"]), group("web", &["Bravo"])],
            vec![group("web", &["Delta"])],
        ]
    );

    let (ungrouped, html) = grouped(&router, "/admin/tickets?page=1").await;
    assert!(ungrouped.is_empty(), "{html}");
}

/// A cursor list keeps each group together as it pages on.
#[tokio::test]
async fn a_group_runs_on_across_cursor_pages() {
    let router =
        mount(tickets().await, panel().resource::<CursoredResource>()).expect("panel builds");
    let mut uri = "/admin/tickets?group_by=team".to_string();
    let mut pages = Vec::new();
    loop {
        let (page, html) = grouped(&router, &uri).await;
        pages.push(page);
        match find_href_with(&html, "after=") {
            Some(next) if pages.len() < 5 => uri = next,
            _ => break,
        }
    }
    assert_eq!(
        pages,
        [
            vec![group("ops", &["Alpha", "Charlie"])],
            vec![group("ops", &["Echo"]), group("web", &["Bravo"])],
            vec![group("web", &["Delta"])],
        ]
    );
}
