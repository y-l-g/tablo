use super::*;
use crate::{TextColumn, lens, table::tests::Task};

fn tabbed() -> Table<Task> {
    Table::<Task>::new(TextColumn::new(lens!(Task.title)))
        .tab(ListTab::new("open", "Open").query(Task::fields().featured().eq(false)))
        .tab(ListTab::new("all", "All"))
}

fn at(tab: Option<&str>) -> TableState {
    TableState {
        tab: tab.map(str::to_string),
        ..TableState::default()
    }
}

/// A list naming no tab shows the first, which narrows its rows like any other.
#[test]
fn no_tab_shows_the_first() {
    let table = tabbed();
    assert_eq!(table.active_tab(&at(None)).map(ListTab::key), Some("open"));
    assert_eq!(
        table.active_tab(&at(Some("all"))).map(ListTab::key),
        Some("all")
    );
    assert_eq!(table.unknown_tab(&at(None)), None);
}

/// A `?tab=` naming no declared tab shows none, and is reported rather than dropped.
#[test]
fn an_unknown_tab_shows_none_and_is_reported() {
    let table = tabbed();
    assert!(table.active_tab(&at(Some("archived"))).is_none());
    assert_eq!(table.unknown_tab(&at(Some("archived"))), Some("archived"));
    let untabbed = Table::<Task>::new(TextColumn::new(lens!(Task.title)));
    assert_eq!(untabbed.unknown_tab(&at(Some("open"))), Some("open"));
}

#[test]
fn two_tabs_sharing_a_key_are_misdeclared() {
    let errors = tabbed()
        .tab(ListTab::new("all", "Everything"))
        .declaration_errors();
    assert_eq!(
        errors,
        [DeclarationErrorKind::DuplicateTab {
            key: "all".to_string()
        }]
    );
}

/// A tab with no query reads nothing; one with a query reads the fields the query names.
#[test]
fn a_tab_reads_what_its_query_names() {
    let all: ListTab<Task> = ListTab::new("all", "All");
    assert!(all.reads().fields().is_empty());
    let open = ListTab::<Task>::new("open", "Open").query(Task::fields().featured().eq(false));
    assert_eq!(open.reads().fields(), ["featured"]);
}

/// A stage whose options list a value its form spelling refuses, as a hand-written `Options` may.
#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed)]
enum Stage {
    Open,
}

impl Options for Stage {
    fn options() -> Vec<(String, crate::Text)> {
        vec![
            ("open".to_string(), crate::Text::from("Open")),
            ("Closed".to_string(), crate::Text::from("Closed")),
        ]
    }
}

impl FormScalar for Stage {
    fn parse_form(value: &str) -> std::result::Result<Self, crate::Text> {
        match value {
            "open" => Ok(Self::Open),
            _ => Err(crate::Text::from("not a stage")),
        }
    }

    fn to_form(&self) -> String {
        "open".to_string()
    }
}

#[derive(Debug, Clone, toasty::Model)]
struct Ticket {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    stage: Stage,
}

/// The option tabs offer every row first, then a counted tab per option reading its field; an
/// option the field's type does not parse is misdeclared rather than offered.
#[test]
fn option_tabs_offer_all_then_a_counted_tab_per_option() {
    let table = Table::<Ticket>::new(TextColumn::new(lens!(Ticket.title)))
        .option_tabs(Ticket::fields().stage());
    let tabs = table.declared_tabs();
    assert_eq!(
        tabs.iter().map(ListTab::key).collect::<Vec<_>>(),
        ["all", "open"]
    );
    assert!(!tabs[0].has_badge() && tabs[0].reads().fields().is_empty());
    assert!(tabs[1].has_badge());
    assert_eq!(tabs[1].reads().fields(), ["stage"]);
    assert_eq!(
        table.declaration_errors(),
        [DeclarationErrorKind::UnparsedTabOption {
            value: "Closed".to_string()
        }]
    );
}
