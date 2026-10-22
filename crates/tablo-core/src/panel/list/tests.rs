use super::*;
use crate::{
    Ability, ResourceDef, TextColumn, lens,
    test_support::{User, tableless_db},
};

#[test]
fn retry_url_for_error_drops_only_bad_cursors() {
    // a malformed cursor can never decode, so its retry link drops
    // pagination; any other failure keeps the full evidence.
    let state = TableState {
        search: Some("Ada".to_string()),
        cursor: Some(crate::table::Cursor::After("cur".to_string())),
        ..TableState::default()
    };
    let bad_cursor =
        crate::toasty_compat::cursor::decode("zz").expect_err("malformed cursor must fail");
    let retry = retry_url_for_error(&state, &bad_cursor, "/admin/users");
    assert!(
        !retry.contains("after="),
        "bad-cursor retry must drop pagination, got {retry}"
    );
    assert!(
        retry.contains("q=Ada"),
        "bad-cursor retry keeps the other state, got {retry}"
    );

    let db_error = crate::error::unavailable("connection reset");
    let retry = retry_url_for_error(&state, &db_error, "/admin/users");
    assert!(
        retry.contains("after=cur"),
        "transient failures retry the same evidence, got {retry}"
    );
}

#[tokio::test]
async fn wired_table_carries_the_declared_action_chrome() {
    struct ChromeResource;
    impl Resource for ChromeResource {
        type Model = User;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .policy_fn(|_cx, ability| {
                    matches!(ability, Ability::DeleteAny | Ability::Delete(_))
                })
                .table(Table::new(TextColumn::new(lens!(User.name))).paginate(25))
        }
    }

    let db = tableless_db(toasty::models!(User)).await;
    // The wiring derives the action URLs from the request path, so the Cx needs one.
    let parts = http::Request::builder()
        .uri("/admin/dummies")
        .body(())
        .unwrap()
        .into_parts()
        .0;
    let cx = crate::test_support::panel_cx::<ChromeResource>(&db).with(parts);
    let declared = crate::resource::require_mounted::<ChromeResource>(&cx).unwrap();
    let wired = crate::panel::wired_table::<ChromeResource>(&cx).unwrap();
    assert!(
        wired.bulk_delete_enabled(),
        "wired_table must attach the delete/bulk chrome the resource declares"
    );
    assert_eq!(
        wired.page_size(),
        declared.table.page_size(),
        "wired_table must keep the declared page size"
    );
}

/// A misdeclared table fails to render, naming what is wrong, rather than render columns that lie.
#[tokio::test]
async fn a_misdeclared_table_fails_to_render() {
    let table = crate::Table::<User>::new((
        TextColumn::new(lens!(User.name)),
        TextColumn::new(lens!(User.name)),
    ))
    .wired();
    let cx = topcoat::context::CxTestBuilder::new().build();
    let view = crate::topcoat_compat::async_page(async {
        let signals = TableSignals::new(&cx, None);
        let page = crate::table::TablePage::<User>::from(vec![]);
        table
            .render_page(&cx, page, &crate::TableState::default(), "", &signals)
            .await
    });
    let Err(error) = view.single().await else {
        panic!("a misdeclared table must not render");
    };
    assert!(
        error.to_string().contains("two columns are named 'name'"),
        "a duplicate column must name the field, got {error}"
    );
}
