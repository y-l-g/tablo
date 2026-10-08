use std::collections::BTreeMap;

use topcoat::context::CxTestBuilder;

use super::super::core::tests::{
    Driver, Task, filters_state, last_link_named, status_table, vehicule_filter,
};
use crate::{
    Table, TablePage, TableState, TextColumn, lens,
    table::{SelectFilter, Sort},
    test_support::Html as _,
};

#[tokio::test]
async fn query_filter_renders_select_control() {
    let cx = CxTestBuilder::new().build();
    let table_driver1 =
        Table::<Driver>::new(TextColumn::new(lens!(Driver.name))).filters(vehicule_filter());
    let page: TablePage<Driver> = Vec::new().into();
    let html = table_driver1
        .render_with_state(&cx, page, &TableState::default(), "/admin/drivers")
        .await
        .html(&cx)
        .await;
    assert!(
        html.contains("data-filter-name=\"vehicule\""),
        "missing variant control in {html}"
    );
    assert!(
        html.contains("Auto") && html.contains("Moto"),
        "missing variant options in {html}"
    );
}

#[tokio::test]
async fn empty_with_filters_shows_filtered_message() {
    let cx = CxTestBuilder::new().build();
    let table_task2 = Table::<Task>::new(TextColumn::new(lens!(Task.title))).filters(
        SelectFilter::new(Task::fields().status(), vec!["draft".to_string()]),
    );
    let mut filters = BTreeMap::new();
    filters.insert("status".to_string(), "draft".to_string());
    let state = TableState {
        filters,
        ..TableState::default()
    };
    let html = table_task2
        .render_with_state(&cx, Vec::new().into(), &state, "/admin/tasks")
        .await
        .html(&cx)
        .await;
    assert!(
        html.contains("No results for these filters"),
        "filter-only empty must be distinct in {html}"
    );
    assert!(
        html.contains("Clear filters"),
        "filter-only empty needs a clear link in {html}"
    );
}

#[tokio::test]
async fn unknown_filter_warns_on_an_empty_page_too() {
    // The zero-rows branch renders the warning banner too, so a typo'd
    // filter never looks like an honest "no results" on an empty table.
    let cx = CxTestBuilder::new().build();
    let html = status_table()
        .render_with_state(
            &cx,
            Vec::new().into(),
            &filters_state(&[("stauts", "published")]),
            "/admin/tasks",
        )
        .await
        .html(&cx)
        .await;
    assert!(
        html.contains("role=\"alert\"") && html.contains("stauts:published"),
        "empty page must still warn about ignored filters, got {html}"
    );
}

#[tokio::test]
async fn empty_clear_links_preserve_the_untouched_state() {
    // The empty-state link rebuilds the URL from the full state, clearing
    // only the dimension it names: `group_by` survives, and with a search
    // and filters active the "Clear search" link leaves the filters alone.
    let cx = CxTestBuilder::new().build();
    let tbl = Table::<Task>::new(TextColumn::new(lens!(Task.title)).sortable())
        .filters(SelectFilter::new(
            Task::fields().status(),
            vec!["published".to_string()],
        ))
        .group_by(lens!(Task.status));
    let state = TableState {
        search: Some("Hello".to_string()),
        filters: BTreeMap::from([("status".to_string(), "published".to_string())]),
        sort: Some(Sort {
            column: "title".to_string(),
            descending: true,
        }),
        group_by: Some("status".to_string()),
        ..TableState::default()
    };
    let html = tbl
        .render_with_state(&cx, Vec::new().into(), &state, "/admin/tasks")
        .await
        .html(&cx)
        .await;
    let clear = last_link_named(&html, "Clear search");
    assert!(
        clear.contains("sort=title"),
        "clear search must keep sort: {clear}"
    );
    assert!(
        clear.contains("dir=desc"),
        "clear search must keep dir: {clear}"
    );
    assert!(
        clear.contains("f.status=published"),
        "clear search must keep filters: {clear}"
    );
    assert!(
        clear.contains("group_by=status"),
        "clear search must keep group_by: {clear}"
    );
    assert!(!clear.contains("q="), "clear search must drop q: {clear}");

    let state = TableState {
        filters: BTreeMap::from([("status".to_string(), "published".to_string())]),
        sort: Some(Sort {
            column: "title".to_string(),
            descending: true,
        }),
        group_by: Some("status".to_string()),
        ..TableState::default()
    };
    let html = tbl
        .render_with_state(&cx, Vec::new().into(), &state, "/admin/tasks")
        .await
        .html(&cx)
        .await;
    // The filter bar renders a "Clear filters" link earlier in the page;
    // the empty-cell one is the subject here.
    let clear = last_link_named(&html, "Clear filters");
    assert!(
        clear.contains("sort=title"),
        "clear filters must keep sort: {clear}"
    );
    assert!(
        clear.contains("group_by=status"),
        "clear filters must keep group_by: {clear}"
    );
    assert!(
        !clear.contains("filters="),
        "clear filters must drop filters: {clear}"
    );
}
