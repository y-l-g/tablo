use topcoat::context::CxTestBuilder;

use super::{
    super::core::tests::{User, normalized_table_tag, table_tag},
    *,
};
use crate::{TableState, TextColumn};

#[tokio::test]
async fn skeleton_shares_the_table_root_with_the_swapped_body() {
    let cx = CxTestBuilder::new().build();
    let tbl = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    );
    let html = tbl
        .render_skeleton(&cx, &tbl.normalize_state(&TableState::default()))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("data-table-root"),
        "skeleton must share table root, got {html}"
    );
    assert!(
        html.contains("aria-busy"),
        "skeleton must announce loading, got {html}"
    );
    assert_eq!(
        html.matches("aria-busy=\"true\"").count(),
        2,
        "busy must ride on the morph boundary and the table root (GH #160), got {html}"
    );
    assert!(
        html.contains("aria-hidden"),
        "skeleton must hold chrome placeholders, got {html}"
    );
    // The skeleton and the swapped table must declare the same layout, or
    // the swap re-measures the columns: comparing the two
    // opening tags states that without pinning a class literal.
    let skeleton_table = table_tag(&html).to_string();
    // The swap payload is the table itself, under the same boundary region.
    let rows = vec![User {
        id: uuid::Uuid::nil(),
        name: "Ada".to_string(),
    }];
    let html = tbl
        .render_with_state(&cx, rows.into(), &TableState::default(), "/admin/users")
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("data-table-root") && html.contains("data-boundary=\"table\""),
        "the swapped table must land in the skeleton's region, got {html}"
    );
    assert!(
        html.contains("Ada"),
        "swap payload must be rows, got {html}"
    );
    // Attribute order is a serializer detail: `table` merges its own
    // classes with the caller's `attrs`, so the skeleton may emit
    // `style` before `class` while the swapped table emits them the
    // other way round. What matters for GH #240 is the same layout —
    // the same classes and the same floor — not the same byte order.
    let swapped_tag = table_tag(&html);
    assert!(
        swapped_tag.contains("table-fixed") && skeleton_table.contains("table-fixed"),
        "the swapped table must declare the skeleton's layout (GH #240), got {html}"
    );
    assert_eq!(
        normalized_table_tag(swapped_tag),
        normalized_table_tag(&skeleton_table),
        "the swapped table must declare the skeleton's layout (GH #240), got {html}"
    );
}

#[tokio::test]
async fn skeleton_carries_the_action_column_for_view_only_chrome() {
    // The skeleton's action column must count every row link `render_inner`
    // renders, `with_view` included, or the swap changes the table width.
    let cx = CxTestBuilder::new().build();
    let tbl = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    )
    .with_view("/admin/users".to_string());
    let skeleton = tbl
        .render_skeleton(&cx, &tbl.normalize_state(&TableState::default()))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    let rows = vec![User {
        id: uuid::Uuid::nil(),
        name: "Ada".to_string(),
    }];
    let rendered = tbl
        .render_with_state(&cx, rows.into(), &TableState::default(), "/admin/users")
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert_eq!(
        rendered.matches(">Actions</span></th>").count(),
        1,
        "the real table renders one action column, got {rendered}"
    );
    assert_eq!(
        skeleton.matches(">Actions</span></th>").count(),
        rendered.matches(">Actions</span></th>").count(),
        "the skeleton must match the swapped table's column count, got {skeleton}"
    );
}

/// The skeleton pulses only the bars the loaded table renders: a table with
/// no searchable column and no filters swaps in without a search or filter
/// bar, so its skeleton must not show one that then disappears.
#[tokio::test]
async fn skeleton_pulses_only_the_bars_the_table_renders() {
    async fn skeleton(tbl: Table<User>) -> String {
        let cx = CxTestBuilder::new().build();
        tbl.render_skeleton(&cx, &tbl.normalize_state(&TableState::default()))
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx)
    }
    let plain = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    );
    let plain_bars = plain.search_enabled() || plain.filter_bar_enabled();
    let html = skeleton(plain).await;
    assert!(!plain_bars, "the plain table renders neither bar");
    assert_eq!(
        html.matches("border-b border-border p-3").count(),
        0,
        "no bar pulse for a table without bars, got {html}"
    );

    let searchable = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()).searchable(),
    );
    assert!(searchable.search_enabled() && !searchable.filter_bar_enabled());
    let html = skeleton(searchable).await;
    assert_eq!(
        html.matches("border-b border-border p-3").count(),
        1,
        "one pulse for the search bar the table renders, got {html}"
    );
}

/// The bulk control sits in the toolbar row above the table beside the
/// search, so it gets a pulse there; and the live page renders its placeholder from the shard's
/// table, whose search and filter bars are hoisted out of the swapped region.
#[tokio::test]
async fn skeleton_pulses_the_bulk_bar_and_not_the_hoisted_bars() {
    let cx = CxTestBuilder::new().build();
    let live_shape = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()).searchable(),
    )
    .with_delete("/admin/users".to_string())
    .with_bulk_delete(true)
    .hide_search()
    .hide_filter_bar();
    let html = live_shape
        .render_skeleton(&cx, &TableState::default())
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert_eq!(
        html.matches("border-b border-border p-3").count(),
        1,
        "one pulse, for the bulk bar the shard's table renders, got {html}"
    );
    assert!(
        html.contains("h-9 w-36"),
        "the pulse is the bulk bar's, got {html}"
    );
}
