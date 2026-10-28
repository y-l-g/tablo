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

/// A tab with no query reads nothing; one with a query reads what it declares, else is unknown.
#[test]
fn a_tab_reads_what_its_query_declares() {
    let all: ListTab<Task> = ListTab::new("all", "All");
    assert!(all.reads().is_some_and(|reads| reads.fields().is_empty()));
    let open = ListTab::<Task>::new("open", "Open").query(Task::fields().featured().eq(false));
    assert!(open.reads().is_none());
    let declared = open.reading(lens!(Task.featured).into());
    assert_eq!(
        declared.reads().map(|reads| reads.fields().to_vec()),
        Some(vec!["featured".to_string()])
    );
}
