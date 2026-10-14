use std::sync::Arc;

use topcoat::context::CxTestBuilder;

use crate::{
    TablePage, lens,
    table::{Table, TableAction, TextColumn},
    test_support::Html as _,
};

#[derive(Debug, Clone, toasty::Model)]
struct User {
    #[key]
    #[auto]
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
}

fn action(name: &'static str, label: &str) -> TableAction<User> {
    TableAction {
        name,
        label: label.to_string(),
        row: true,
        bulk: false,
        confirm: false,
        input: None,
        allowed: Arc::new(|_: &User| true),
    }
}

fn name_column() -> TextColumn<User, String> {
    TextColumn::new(lens!(User.name))
}

/// The actions column takes the width its controls need, its head and its cells declare that
/// `width`, and the table's floor carries the same term: a fixed layout honors a cell's `width`,
/// never its `min-width`.
#[tokio::test]
async fn the_actions_column_takes_the_width_its_controls_need() {
    let table = Table::<User>::new(name_column())
        .wired()
        .with_view("/admin/users".to_string())
        .with_edit("/admin/users".to_string())
        .with_delete("/admin/users".to_string())
        .with_custom_actions(
            "/admin/users".to_string(),
            vec![action("publish", "Publish"), action("feature", "Feature")],
        );
    let widths = table.frame().column_widths();
    assert_eq!(widths.actions.as_deref(), Some("width: 18.625rem"));
    assert_eq!(
        widths.table_min_width.as_deref(),
        Some("min-width: 24.625rem")
    );
    let cx = CxTestBuilder::new().build();
    let page: TablePage<User> = vec![User {
        id: uuid::Uuid::nil(),
        name: "Ada".to_string(),
    }]
    .into();
    let html = table.render_loaded(&cx, page).await.html(&cx).await;
    assert_eq!(
        html.matches("style=\"width: 18.625rem\"").count(),
        2,
        "the actions head and cell carry the controls' width, got {html}"
    );
}

/// A wide-script label takes two columns a character: one control cannot wrap, so an
/// under-estimate paints its button over the column beside it.
#[test]
fn a_wide_script_label_takes_two_columns_a_character() {
    let table = Table::<User>::new(name_column())
        .wired()
        .with_custom_actions("/admin/users".to_string(), vec![action("stock", "在庫")]);
    let widths = table.frame().column_widths();
    assert_eq!(widths.actions.as_deref(), Some("width: 4.875rem"));
}

/// "Publishing date" needs nine rem where the plain wide floor gives it six, and a column narrower
/// than its header paints it over its neighbour.
#[test]
fn a_wide_column_floors_at_its_header() {
    let table = Table::<User>::new(name_column().label("Publishing date")).wired();
    let widths = table.frame().column_widths();
    assert_eq!(widths.cells[0].as_deref(), Some("width: 9rem"));
    assert_eq!(widths.table_min_width.as_deref(), Some("min-width: 9rem"));
}
