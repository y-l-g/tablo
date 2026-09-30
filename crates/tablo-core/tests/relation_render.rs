//! The read-only relation table (item 6).

use tablo_core::{
    ColumnWidth, MAX_RELATION_ROWS, RelationColumn, RelationColumns, Resource, render_relation,
};
use topcoat::{
    context::{Cx, CxTestBuilder},
    view::ViewExt,
};

#[derive(Debug, Clone, toasty::Model)]
struct Row {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
}

fn row(name: &str) -> Row {
    Row {
        id: uuid::Uuid::new_v4(),
        name: name.to_string(),
    }
}

fn columns() -> RelationColumns<Row> {
    RelationColumns::columns((
        RelationColumn::computed("Name", |r: &Row| r.name.clone()),
        RelationColumn::computed("Shout", |r: &Row| r.name.to_uppercase()),
    ))
}

fn rows() -> Vec<Row> {
    vec![row("first"), row("second")]
}

/// The related resource of a table that shows every row.
struct AllRows;

impl Resource for AllRows {
    type Model = Row;
    type Form = tablo_core::NoForm<Self::Model>;

    fn table(_cx: &Cx) -> tablo_core::Table<Row> {
        tablo_core::Table::new(
            |r: &Row| r.id.to_string(),
            tablo_core::TextColumn::r#for(Row::fields().name(), |r: &Row| r.name.clone()),
        )
    }

    fn can_view(_cx: &Cx, _record: &Row) -> bool {
        true
    }
}

/// The related resource of a table that refuses the rows named `denied`.
struct NamedRows;

impl Resource for NamedRows {
    type Model = Row;
    type Form = tablo_core::NoForm<Self::Model>;

    fn table(_cx: &Cx) -> tablo_core::Table<Row> {
        tablo_core::Table::new(
            |r: &Row| r.id.to_string(),
            tablo_core::TextColumn::r#for(Row::fields().name(), |r: &Row| r.name.clone()),
        )
    }

    fn can_view(_cx: &Cx, record: &Row) -> bool {
        record.name != "denied-row"
    }
}

#[tokio::test]
async fn a_relation_table_renders_every_row_and_column() {
    let cx = CxTestBuilder::new().build();
    let html = render_relation::<AllRows>(&cx, "Related", columns(), &rows())
        .single()
        .await
        .unwrap()
        .render(&cx);
    for expected in [
        "Related", "Name", "Shout", "first", "FIRST", "second", "SECOND",
    ] {
        assert!(html.contains(expected), "missing {expected:?} in {html}");
    }
}

/// A cell is text, not markup.
#[tokio::test]
async fn a_relation_cell_is_escaped() {
    let cx = CxTestBuilder::new().build();
    let rows = vec![row("<script>alert(1)</script>")];
    let html = render_relation::<AllRows>(&cx, "Related", columns(), &rows)
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        !html.contains("<script>"),
        "a stored value must not become markup: {html}"
    );
    assert!(
        html.contains("&lt;script&gt;"),
        "the value renders escaped instead: {html}"
    );
}

#[tokio::test]
async fn an_empty_relation_says_none() {
    let cx = CxTestBuilder::new().build();
    let html = render_relation::<AllRows>(&cx, "Related", columns(), &[])
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(html.contains("None."), "empty relation: {html}");
    assert!(
        !html.contains("<table"),
        "an empty relation renders no table: {html}"
    );
}

/// The related resource's `can_view` decides which rows render.
///
/// The table is the surface that reads a fixed set of loaded rows, so the
/// predicate runs here rather than in the caller's projection: a row it refuses
/// must not reach a column's `display` closure.
#[tokio::test]
async fn a_relation_omits_rows_the_related_resource_refuses() {
    let cx = CxTestBuilder::new().build();
    let rows = vec![row("kept-a"), row("denied-row"), row("kept-b")];
    let html = render_relation::<NamedRows>(&cx, "Related", columns(), &rows)
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        !html.contains("denied-row"),
        "a refused row must not render: {html}"
    );
    for kept in ["kept-a", "kept-b"] {
        assert!(html.contains(kept), "an admitted row renders: {html}");
    }
}

/// A relation caps its rows and names the truncation.
///
/// The related rows are already loaded, so the cap bounds the page rather than
/// a query; a table that stopped at the cap without a line saying so would read
/// as the whole relation.
/// The policy runs before the cap, so a refused row never spends a slot.
///
/// The refused rows come first: a cap applied before the filter would spend
/// every slot on rows the reader may not see and render an empty table.
#[tokio::test]
async fn a_relation_filters_before_it_caps() {
    let cx = CxTestBuilder::new().build();
    let mut rows: Vec<Row> = (0..MAX_RELATION_ROWS + 10)
        .map(|_| row("denied-row"))
        .collect();
    rows.extend((0..3).map(|i| row(&format!("kept-{i}"))));
    let html = render_relation::<NamedRows>(&cx, "Related", columns(), &rows)
        .single()
        .await
        .unwrap()
        .render(&cx);
    for i in 0..3 {
        assert!(
            html.contains(&format!("kept-{i}")),
            "a visible row behind the refused ones still renders: {html}"
        );
    }
    assert!(
        !html.contains("None."),
        "refused rows must not fill the cap: {html}"
    );
    assert!(
        !html.contains("Showing the first"),
        "three visible rows do not overflow the cap: {html}"
    );
}

#[tokio::test]
async fn a_relation_caps_its_rows_and_says_so() {
    let cx = CxTestBuilder::new().build();
    let total = MAX_RELATION_ROWS + 3;
    let rows: Vec<Row> = (0..total).map(|i| row(&format!("r{i:03}"))).collect();
    let html = render_relation::<AllRows>(&cx, "Related", columns(), &rows)
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains(&format!("r{:03}", MAX_RELATION_ROWS - 1)),
        "the table renders up to the cap: {html}"
    );
    assert!(
        !html.contains(&format!("r{MAX_RELATION_ROWS:03}")),
        "the table renders no row past the cap: {html}"
    );
    assert!(
        html.contains(&format!(
            "the first {MAX_RELATION_ROWS} of {total} related rows you can view"
        )),
        "a truncated table names the cap and the total: {html}"
    );
}

/// GH #264: the relation table lays out fixed like the list table — the
/// declared width reaches the header and every row's cell as data, a wide
/// column declares nothing, long values truncate instead of widening the
/// column, and the wide column's floor keeps its measure on a narrow
/// viewport so the wrapper scrolls instead of collapsing it to zero.
#[tokio::test]
async fn a_relation_table_lays_out_fixed_with_declared_widths() {
    let cx = CxTestBuilder::new().build();
    let declared = RelationColumns::columns((
        RelationColumn::computed("Name", |r: &Row| r.name.clone()),
        RelationColumn::computed("Status", |r: &Row| r.name.clone())
            .width(ColumnWidth::Percent(30)),
    ));
    let html = render_relation::<AllRows>(&cx, "Related", declared, &[row("first")])
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("table-fixed"),
        "the relation table must lay out fixed, got {html}"
    );
    assert_eq!(
        html.matches("style=\"width: 30%\"").count(),
        2,
        "the declared width must reach the th and the td, got {html}"
    );
    assert_eq!(
        html.matches("style=\"width").count(),
        2,
        "the wide column must declare no width, got {html}"
    );
    assert!(
        html.contains("truncate"),
        "a value wider than its column must truncate, got {html}"
    );
    assert!(
        html.contains("min-width: calc(30% + 6rem)"),
        "the wide column keeps its floor so a narrow viewport scrolls, got {html}"
    );

    // The defaults declare nothing at all: two wide columns split the table
    // and still carry the floor that keeps them readable.
    let html = render_relation::<AllRows>(&cx, "Related", columns(), &[row("first")])
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("table-fixed"),
        "a default relation table must lay out fixed, got {html}"
    );
    assert!(
        !html.contains("style=\"width"),
        "default wide columns declare no width, got {html}"
    );
    assert!(
        html.contains("min-width: 12rem"),
        "two wide columns keep two floors, got {html}"
    );
}

/// A relation table shares the list table's width budget: kind defaults that
/// would together claim more than 60% of the table scale down so the wide
/// column keeps a share. Seven narrow columns claim 10% each nominally (70%),
/// so each is scaled to a whole-percent share of the 60% budget.
#[tokio::test]
async fn a_relation_tables_narrow_defaults_share_the_list_budget() {
    let cx = CxTestBuilder::new().build();
    let narrow = |label: &'static str| {
        RelationColumn::computed(label, |r: &Row| r.name.clone()).width(ColumnWidth::Narrow)
    };
    let declared = RelationColumns::columns((
        narrow("A"),
        narrow("B"),
        narrow("C"),
        narrow("D"),
        narrow("E"),
        narrow("F"),
        narrow("G"),
        RelationColumn::computed("Name", |r: &Row| r.name.clone()),
    ));
    let html = render_relation::<AllRows>(&cx, "Related", declared, &[row("first")])
        .single()
        .await
        .unwrap()
        .render(&cx);
    let share = 10 * 60 / 70;
    assert_eq!(
        html.matches(&format!("style=\"width: {share}%\"")).count(),
        14,
        "every narrow header and cell takes the scaled share, got {html}"
    );
    assert!(
        !html.contains("width: 10%"),
        "no narrow column keeps its nominal share over budget, got {html}"
    );
    let terms = format!("{share}% + ").repeat(7);
    assert!(
        html.contains(&format!("min-width: calc({terms}6rem)")),
        "the floor sums the scaled shares and the wide column's rem, got {html}"
    );
}
