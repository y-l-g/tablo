use std::collections::BTreeMap;

use topcoat::context::CxTestBuilder;

use super::{
    super::core::tests::{
        Driver, Task, filters_state, last_link_named, status_table, vehicule_filter,
    },
    *,
};
use crate::{
    TablePage, TableState, TextColumn,
    resource::{DateFilter, SelectFilter, Sort, TernaryFilter},
};

#[tokio::test]
async fn filter_widgets_render_typed_controls() {
    let cx = CxTestBuilder::new().build();
    let table_task1 = Table::<Task>::new(
        |t| t.id.to_string(),
        TextColumn::r#for(Task::fields().title(), |t| t.title.clone()),
    )
    .filters((
        SelectFilter::r#for(
            Task::fields().status(),
            vec!["draft".to_string(), "published".to_string()],
        ),
        TernaryFilter::r#for(Task::fields().featured()),
        DateFilter::r#for(Task::fields().created_at()),
    ));
    let page: TablePage<Task> = Vec::new().into();
    // State with an active select value pre-selects it.
    let mut filters = BTreeMap::new();
    filters.insert("status".to_string(), "published".to_string());
    let state = TableState {
        filters,
        ..TableState::default()
    };
    let html = table_task1
        .render_with_state(&cx, page, &state, "/admin/tasks")
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("data-filters-form"),
        "missing filters form in {html}"
    );
    for name in ["status", "featured", "created_at"] {
        assert!(
            html.contains(&format!("data-filter-name=\"{name}\"")),
            "missing control for {name} in {html}"
        );
    }
    // Select options + current selection.
    assert!(
        html.contains("draft") && html.contains("published"),
        "missing select options in {html}"
    );
    assert!(
        html.contains("value=\"published\" selected")
            || html.contains("value=\"published\" selected=\"\""),
        "published should be selected in {html}"
    );
    // Ternary + date controls.
    assert!(
        html.contains("value=\"true\"") && html.contains("value=\"false\""),
        "missing ternary options in {html}"
    );
    assert!(
        html.contains("type=\"date\""),
        "missing date input in {html}"
    );
    // Each control is its own `f.<name>` form field, so the GET form submits
    // the filters itself; only a live table carries a query transport.
    assert!(
        html.contains("name=\"f.status\"") && html.contains("name=\"f.created_at\""),
        "each filter control must be an f.<name> field in {html}"
    );
    assert!(
        !html.contains("data-filters-transport"),
        "a static table carries no query transport in {html}"
    );
    assert!(
        html.contains("<noscript>") && html.contains("Apply filters"),
        "missing no-JS filter fallback in {html}"
    );
}

#[tokio::test]
async fn variant_filter_renders_select_control() {
    let cx = CxTestBuilder::new().build();
    let table_driver1 = Table::<Driver>::new(
        |d| d.id.to_string(),
        TextColumn::r#for(Driver::fields().name(), |d| d.name.clone()),
    )
    .filters(vehicule_filter());
    let page: TablePage<Driver> = Vec::new().into();
    let html = table_driver1
        .render_with_state(&cx, page, &TableState::default(), "/admin/drivers")
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
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
    let table_task2 = Table::<Task>::new(
        |t| t.id.to_string(),
        TextColumn::r#for(Task::fields().title(), |t| t.title.clone()),
    )
    .filters(SelectFilter::r#for(
        Task::fields().status(),
        vec!["draft".to_string()],
    ));
    let mut filters = BTreeMap::new();
    filters.insert("status".to_string(), "draft".to_string());
    let state = TableState {
        filters,
        ..TableState::default()
    };
    let html = table_task2
        .render_with_state(&cx, Vec::new().into(), &state, "/admin/tasks")
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
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
    // GH #93 follow-up: the zero-rows branch returned before the warning
    // banner rendered, so a typo'd filter looked like an honest "no
    // results" on an empty table.
    let cx = CxTestBuilder::new().build();
    let html = status_table(&cx)
        .render_with_state(
            &cx,
            Vec::new().into(),
            &filters_state(&[("stauts", "published")]),
            "/admin/tasks",
        )
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
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
    let tbl = Table::<Task>::new(
        |t| t.id.to_string(),
        TextColumn::r#for(Task::fields().title(), |t| t.title.clone()).sortable(),
    )
    .filters(SelectFilter::r#for(
        Task::fields().status(),
        vec!["published".to_string()],
    ))
    .group_by("status", |t| t.status.clone());
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
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
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
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
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

#[tokio::test]
async fn unknown_filter_renders_alert_banner_and_keeps_200() {
    // the list keeps a 200 but warns instead of lying about
    // "these filters".
    let cx = CxTestBuilder::new().build();
    let tbl = status_table(&cx);
    let rows = vec![Task {
        id: uuid::Uuid::nil(),
        title: "Hello".to_string(),
        status: "published".to_string(),
        featured: false,
        created_at: jiff::Timestamp::now(),
    }];
    let html = tbl
        .render_with_state(
            &cx,
            rows.into(),
            &filters_state(&[("stauts", "published")]),
            "/admin/tasks",
        )
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("role=\"alert\"") && html.contains("stauts:published"),
        "typo filter must warn, got {html}"
    );

    let rows = vec![Task {
        id: uuid::Uuid::nil(),
        title: "Hello".to_string(),
        status: "published".to_string(),
        featured: false,
        created_at: jiff::Timestamp::now(),
    }];
    let html = tbl
        .render_with_state(
            &cx,
            rows.into(),
            &filters_state(&[("status", "published")]),
            "/admin/tasks",
        )
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        !html.contains("role=\"alert\""),
        "valid filter must not warn, got {html}"
    );
}
