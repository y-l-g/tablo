use topcoat::context::CxTestBuilder;

use super::*;
use crate::{
    lens,
    table::{QueryFilter, RowActions, SelectFilter, Sort, Table, TextColumn},
    test_support::Html as _,
};

#[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
pub(crate) struct User {
    #[key]
    #[auto]
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
}

#[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
pub(crate) struct Task {
    #[key]
    #[auto]
    pub(crate) id: uuid::Uuid,
    pub(crate) title: String,
    pub(crate) status: String,
    pub(crate) featured: bool,
    pub(crate) created_at: jiff::Timestamp,
}

#[derive(Debug, Clone, PartialEq, toasty::Embed)]
pub(crate) enum Vehicule {
    Auto {
        #[shared(puissance)]
        puissance: String,
        seats: String,
    },
    Moto {
        #[shared(puissance)]
        puissance: String,
        cc: String,
    },
}

#[derive(Debug, Clone, toasty::Model)]
pub(crate) struct Driver {
    #[key]
    #[auto]
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
    vehicule: Vehicule,
}

pub(crate) fn vehicule_filter() -> QueryFilter<Driver> {
    QueryFilter::new("vehicule", "Véhicule")
        .option("Auto", Driver::fields().vehicule().is_auto())
        .option("Moto", Driver::fields().vehicule().is_moto())
}

pub(crate) fn status_table() -> Table<Task> {
    Table::<Task>::new(TextColumn::new(lens!(Task.title))).filters(SelectFilter::new(
        Task::fields().status(),
        vec!["published".to_string(), "draft".to_string()],
    ))
}

pub(crate) fn filters_state(pairs: &[(&str, &str)]) -> TableState {
    TableState {
        filters: pairs
            .iter()
            .map(|(k, v)| (k.to_string(), (*v).into()))
            .collect(),
        ..TableState::default()
    }
}

pub(crate) fn last_link_named<'a>(html: &'a str, label: &str) -> &'a str {
    html.rsplit('<')
        .find(|chunk| chunk.contains(label))
        .unwrap_or_else(|| panic!("missing {label} link in {html}"))
}

#[tokio::test]
async fn denied_rows_render_no_links_and_no_checkbox() {
    let cx = CxTestBuilder::new().build();
    let ada = User {
        id: uuid::Uuid::new_v4(),
        name: "Ada".to_string(),
    };
    let ken = User {
        id: uuid::Uuid::new_v4(),
        name: "Ken".to_string(),
    };
    let ken_id = ken.id.to_string();
    let ada_id = ada.id.to_string();
    let policy_table = Table::<User>::new(TextColumn::new(lens!(User.name)))
        .wired()
        .with_delete("/admin/users".to_string())
        .with_edit("/admin/users".to_string())
        .with_view("/admin/users".to_string())
        .with_bulk_delete(true)
        .row_actions(|u: &User| RowActions {
            view: true,
            edit: u.name != "Ken",
            delete: u.name != "Ken",
        });
    let page: TablePage<User> = vec![ada, ken].into();
    let html = policy_table.render_loaded(&cx, page).await.html(&cx).await;
    assert!(
        html.contains(&format!("href=\"/admin/users/{ada_id}/edit\""))
            && html.contains(&format!("href=\"/admin/users/{ada_id}\""))
            && html.contains(&format!("/admin/users/{ada_id}/delete")),
        "the allowed row must keep its View/Edit/Delete links, got {html}"
    );
    assert!(
        html.contains(&format!("href=\"/admin/users/{ken_id}\""))
            && !html.contains(&format!("/admin/users/{ken_id}/edit"))
            && !html.contains(&format!("/admin/users/{ken_id}/delete")),
        "the denied row must render no Edit/Delete link, got {html}"
    );
    assert!(
        html.contains(">Ken<"),
        "the denied row must still render, got {html}"
    );
    assert!(
        !html.contains(&format!("value=\"{ken_id}\"")),
        "the denied row must render no checkbox, got {html}"
    );
    assert_eq!(
        html.matches("aria-label=\"Select row\"").count(),
        1,
        "the allowed row owns the page's only checkbox, got {html}"
    );
    assert!(
        html.contains(&format!("value=\"{ada_id}\"")),
        "the allowed row must keep its checkbox, got {html}"
    );
}

/// Return the `<tr>…</tr>` chunk holding the row checkbox with `value`, without its closing tag.
pub(crate) fn row_chunk<'a>(html: &'a str, value: &str) -> &'a str {
    let at = html
        .find(&format!("value=\"{value}\""))
        .unwrap_or_else(|| panic!("missing the row checkbox with value {value}"));
    let start = html[..at].rfind("<tr").expect("the row's opening tag");
    let end = html[at..].find("</tr>").expect("the row's closing tag") + at;
    &html[start..end]
}

#[tokio::test]
async fn fully_locked_rows_keep_their_actions_cell_with_no_links() {
    let cx = CxTestBuilder::new().build();
    let ada = User {
        id: uuid::Uuid::new_v4(),
        name: "Ada".to_string(),
    };
    let ken = User {
        id: uuid::Uuid::new_v4(),
        name: "Ken".to_string(),
    };
    let ada_id = ada.id.to_string();
    let policy_table = Table::<User>::new(TextColumn::new(lens!(User.name)))
        .wired()
        .with_delete("/admin/users".to_string())
        .with_edit("/admin/users".to_string())
        .with_bulk_delete(true)
        .row_actions(|u: &User| {
            let allowed = u.name != "Ken";
            RowActions {
                view: allowed,
                edit: allowed,
                delete: allowed,
            }
        });
    let page: TablePage<User> = vec![ada, ken].into();
    let html = policy_table.render_loaded(&cx, page).await.html(&cx).await;
    let ken_at = html.find(">Ken<").expect("the locked row");
    let ken_start = html[..ken_at].rfind("<tr").expect("its row");
    let ken_end = html[ken_at..].find("</tr>").expect("its end") + ken_at;
    let ken_row = &html[ken_start..ken_end];
    assert!(
        !ken_row.contains("/admin/users/"),
        "the locked row must render no action link at all, got {ken_row}"
    );
    let ada_row = row_chunk(&html, &ada_id);
    assert!(
        ada_row.contains(&format!("/admin/users/{ada_id}/edit")),
        "the allowed row must keep its links, got {ada_row}"
    );
    let thead_at = html.find("<thead").expect("a header row");
    let thead_end = html.find("</thead>").expect("its end");
    assert_eq!(
        ken_row.matches("</td>").count(),
        html[thead_at..thead_end].matches("</th>").count(),
        "the locked row must carry a cell per header, got {ken_row}"
    );
}

#[tokio::test]
async fn a_chromeless_table_never_consults_the_row_policy() {
    let cx = CxTestBuilder::new().build();
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = calls.clone();
    let policy_table = Table::<User>::new(TextColumn::new(lens!(User.name)))
        .wired()
        .row_actions(move |_: &User| {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            RowActions::ALL
        });
    let page: TablePage<User> = vec![User {
        id: uuid::Uuid::new_v4(),
        name: "Ada".to_string(),
    }]
    .into();
    let html = policy_table.render_loaded(&cx, page).await.html(&cx).await;
    assert!(
        html.contains("Ada"),
        "the table must still render its row, got {html}"
    );
    assert_eq!(
        calls.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "a table with no action prefix must not call the row policy"
    );
}

#[tokio::test]
async fn rows_carry_the_primary_key_in_every_action() {
    let cx = CxTestBuilder::new().build();
    let tbl = Table::<User>::new(TextColumn::new(lens!(User.name)))
        .wired()
        .with_delete("/admin/users".to_string())
        .with_edit("/admin/users".to_string())
        .with_bulk_delete(true);
    let rows = vec![User {
        id: uuid::Uuid::new_v4(),
        name: "Ada".to_string(),
    }];
    let key = rows[0].id.to_string();
    let page: TablePage<User> = rows.into();
    let html = tbl.render_loaded(&cx, page).await.html(&cx).await;
    assert!(
        html.contains(&format!("href=\"/admin/users/{key}/edit\"")),
        "edit URL must carry the record key in {html}"
    );
    assert!(
        html.contains(&format!("value=\"{key}\"")),
        "bulk value must carry the record key in {html}"
    );
    assert!(
        html.contains(&format!("/admin/users/{key}/delete")),
        "the delete action must carry the record key in {html}"
    );
}

#[tokio::test]
async fn composite_key_rows_render_no_action() {
    #[derive(Debug, Clone, toasty::Model)]
    #[key(owner, slot)]
    struct Seat {
        owner: String,
        slot: i64,
        label: String,
    }

    let cx = CxTestBuilder::new().build();
    let tbl = Table::<Seat>::new(TextColumn::new(lens!(Seat.label)))
        .wired()
        .with_delete("/admin/seats".to_string())
        .with_edit("/admin/seats".to_string())
        .with_bulk_delete(true);
    let page: TablePage<Seat> = vec![Seat {
        owner: "ada".to_string(),
        slot: 2,
        label: "Aisle".to_string(),
    }]
    .into();
    let html = tbl.render_loaded(&cx, page).await.html(&cx).await;
    assert!(html.contains("Aisle"), "the row renders in {html}");
    assert!(
        !html.contains("/admin/seats/") && !html.contains(r#"aria-label="Select row""#),
        "a key with no URL form must render no action in {html}"
    );
}

#[tokio::test]
async fn group_by_unknown_value_renders_no_headers_and_drops_param() {
    let cx = CxTestBuilder::new().build();
    let grouped = Table::<User>::new(TextColumn::new(lens!(User.name)).sortable())
        .group_by(lens!(User.name))
        .paginate(1);
    let state = TableState {
        group_by: Some("email".to_string()),
        sort: Some(Sort {
            column: "name".to_string(),
            descending: false,
        }),
        ..TableState::default()
    };
    let rows = vec![User {
        id: uuid::Uuid::nil(),
        name: "Ada".to_string(),
    }];
    let page = TablePage {
        rows,
        next_cursor: Some("abc".to_string()),
        prev_cursor: None,
        position: None,
    };
    let html = grouped
        .render_with_state(&cx, page, &state, "/admin/users")
        .await
        .html(&cx)
        .await;
    assert!(
        !html.contains("on this page"),
        "unknown group_by must render no headers, got {html}"
    );
    assert!(
        !html.contains("group_by"),
        "unknown group_by must drop from links, got {html}"
    );
}

#[tokio::test]
async fn group_by_orders_each_row_under_its_own_header() {
    let cx = CxTestBuilder::new().build();
    let grouped =
        Table::<Task>::new(TextColumn::new(lens!(Task.title))).group_by(lens!(Task.status));
    let state = TableState {
        group_by: Some("status".to_string()),
        ..TableState::default()
    };
    let task = |title: &str, status: &str| Task {
        id: uuid::Uuid::new_v4(),
        title: title.to_string(),
        status: status.to_string(),
        featured: false,
        created_at: jiff::Timestamp::now(),
    };
    let page = TablePage::from(vec![
        task("alpha", "draft"),
        task("bravo", "published"),
        task("charlie", "draft"),
        task("delta", "published"),
    ]);
    let html = grouped
        .render_with_state(&cx, page, &state, "/admin/tasks")
        .await
        .html(&cx)
        .await;
    let at = |needle: &str| {
        html.find(needle)
            .unwrap_or_else(|| panic!("missing {needle:?} in {html}"))
    };
    let draft_header = at("draft (2 on this page)");
    let alpha = at("alpha");
    let charlie = at("charlie");
    let published_header = at("published (2 on this page)");
    let bravo = at("bravo");
    let delta = at("delta");
    assert!(
        draft_header < alpha && alpha < charlie,
        "both draft rows must sit under the draft header, got {html}"
    );
    assert!(
        charlie < published_header,
        "the published header must follow the draft group, got {html}"
    );
    assert!(
        published_header < bravo && bravo < delta,
        "both published rows must sit under the published header, got {html}"
    );
    assert!(
        html.contains("group-draft-") && html.contains("group-published-"),
        "each group header needs a stable id naming its group, got {html}"
    );
}

/// A row key holding the selection wire's separator travels percent-encoded: the row checkbox and
/// the select-all box each spell one token for it, never one token per part.
#[tokio::test]
async fn a_key_holding_the_selection_separator_renders_one_token() {
    #[derive(Debug, Clone, toasty::Model)]
    struct Tag {
        #[key]
        code: String,
        label: String,
    }

    let cx = CxTestBuilder::new().build();
    let tbl = Table::<Tag>::new(TextColumn::new(lens!(Tag.label)))
        .wired()
        .with_delete("/admin/tags".to_string())
        .with_edit("/admin/tags".to_string())
        .with_bulk_delete(true);
    let page: TablePage<Tag> = vec![Tag {
        code: "Doe, John".to_string(),
        label: "Doe, John".to_string(),
    }]
    .into();
    let html = tbl.render_loaded(&cx, page).await.html(&cx).await;
    let token = ",Doe%2C%20John,";
    let row = row_chunk(&html, "Doe, John");
    assert!(
        row.contains(token),
        "the row checkbox must select the whole key, got {row}"
    );
    let thead = &html[..html.find("</thead>").expect("a header row")];
    assert!(
        thead.contains(token),
        "select all must select the whole key, got {thead}"
    );
}

#[tokio::test]
async fn select_all_hands_its_handler_the_tokens_as_one_string() {
    #[derive(Debug, Clone, toasty::Model)]
    struct Tag {
        #[key]
        code: String,
        label: String,
    }

    let cx = CxTestBuilder::new().build();
    let tbl = Table::<Tag>::new(TextColumn::new(lens!(Tag.label)))
        .wired()
        .with_delete("/admin/tags".to_string())
        .with_bulk_delete(true);
    let page: TablePage<Tag> = vec![
        Tag {
            code: "a".to_string(),
            label: "A".to_string(),
        },
        Tag {
            code: "b".to_string(),
            label: "B".to_string(),
        },
    ]
    .into();
    let html = tbl.render_loaded(&cx, page).await.html(&cx).await;
    let thead = &html[..html.find("</thead>").expect("a header row")];
    let start = thead.rfind("<input").expect("a select-all box");
    let select_all = &thead[start..start + thead[start..].find("></th>").expect("its cell")];
    assert_eq!(
        select_all
            .matches("cx.hydrate(&quot;,a,|,b,&quot;)")
            .count(),
        4,
        "every select-all expression must take the tokens as one string, got {select_all}"
    );
}

/// Every `<th …>` opening tag in `html`, then the text up to its closing tag.
fn header_cells(html: &str) -> Vec<&str> {
    html.match_indices("<th")
        .map(|(at, _)| {
            let end = html[at..].find("</th>").expect("the head's closing tag") + at;
            &html[at..end]
        })
        .collect()
}

fn task(title: &str) -> Task {
    Task {
        id: uuid::Uuid::new_v4(),
        title: title.to_string(),
        status: "draft".to_string(),
        featured: false,
        created_at: "2024-01-15T09:30:00Z".parse().expect("a timestamp"),
    }
}

/// The selection and actions columns shrink to their controls, which stay on one line: a row's
/// Delete never wraps under its Edit, whatever the font.
#[tokio::test]
async fn chrome_columns_shrink_to_controls_on_one_line() {
    let cx = CxTestBuilder::new().build();
    let wired = Table::<User>::new(TextColumn::new(lens!(User.name)))
        .wired()
        .with_view("/admin/users".to_string())
        .with_edit("/admin/users".to_string())
        .with_delete("/admin/users".to_string())
        .with_bulk_delete(true);
    let page: TablePage<User> = vec![User {
        id: uuid::Uuid::new_v4(),
        name: "Ada".to_string(),
    }]
    .into();
    let html = wired.render_loaded(&cx, page).await.html(&cx).await;
    assert_eq!(
        html.matches(super::super::CHROME_CELL_CLASS).count(),
        4,
        "the selection and actions heads and cells shrink to their controls, got {html}"
    );
    let body = &html[html.find("<tbody").expect("a body")..html.find("</tbody>").expect("its end")];
    assert!(
        !body.contains("flex-wrap") && !html.contains("table-fixed"),
        "the controls never wrap and the layout is automatic, got {html}"
    );
}

/// The first growing column takes the spare width; the others fit their content, so one long
/// value cannot squeeze a short one.
#[tokio::test]
async fn only_the_first_growing_column_takes_the_spare_width() {
    let cx = CxTestBuilder::new().build();
    let wired = Table::<Task>::new((
        crate::BooleanColumn::new(lens!(Task.featured)),
        TextColumn::new(lens!(Task.title)),
        TextColumn::new(lens!(Task.status)),
    ))
    .wired();
    let page: TablePage<Task> = vec![task("Ship it")].into();
    let html = wired.render_loaded(&cx, page).await.html(&cx).await;
    let growing: Vec<&str> = header_cells(&html)
        .into_iter()
        .filter(|head| head.contains("w-full"))
        .collect();
    assert_eq!(growing.len(), 1, "one column takes the slack, got {html}");
    assert!(
        growing[0].contains("Title"),
        "the first growing column takes it, got {growing:?}"
    );
    assert_eq!(
        html.matches("max-w-xs truncate").count(),
        2,
        "both growing columns cap their value, got {html}"
    );
}

/// An exact width holds on the head and on every cell, so a long value truncates within it.
#[tokio::test]
async fn an_exact_width_holds_on_head_and_cells() {
    let cx = CxTestBuilder::new().build();
    let wired =
        Table::<Task>::new(TextColumn::new(lens!(Task.title)).width(crate::ColumnWidth::Rem(8)))
            .wired();
    let page: TablePage<Task> = vec![task("A"), task("B")].into();
    let html = wired.render_loaded(&cx, page).await.html(&cx).await;
    assert_eq!(
        html.matches("width: 8rem; max-width: 8rem").count(),
        3,
        "the head and both cells hold eight rem, got {html}"
    );
}

/// A row opens its record's view page, or its edit page when the table has none, from anywhere on
/// the row; a row with neither stays plain.
#[tokio::test]
async fn rows_open_their_record() {
    let cx = CxTestBuilder::new().build();
    let ada = User {
        id: uuid::Uuid::new_v4(),
        name: "Ada".to_string(),
    };
    let id = ada.id.to_string();
    let render = |wired: crate::table::WiredTable<User>| {
        let page: TablePage<User> = vec![ada.clone()].into();
        let cx = &cx;
        async move { wired.render_loaded(cx, page).await.html(cx).await }
    };
    let base = || Table::<User>::new(TextColumn::new(lens!(User.name))).wired();

    let html = render(
        base()
            .with_view("/admin/users".to_string())
            .with_edit("/admin/users".to_string()),
    )
    .await;
    let link = last_link_named(&html, "data-record-link");
    assert!(
        link.contains(&format!("href=\"/admin/users/{id}\"")),
        "the row opens the view page, got {link}"
    );

    let html = render(base().with_edit("/admin/users".to_string())).await;
    let link = last_link_named(&html, "data-record-link");
    assert!(
        link.contains(&format!("href=\"/admin/users/{id}/edit\"")),
        "without a view page the row opens the edit page, got {link}"
    );

    let html = render(base()).await;
    assert!(
        !html.contains("data-record-link"),
        "a row with nowhere to go links nowhere, got {html}"
    );
}

/// A derived table spells a timestamp as its calendar day.
#[tokio::test]
async fn a_derived_table_spells_a_timestamp_as_a_day() {
    let cx = CxTestBuilder::new().build();
    let page: TablePage<Task> = vec![task("Ship it")].into();
    let html = <Task as crate::ModelColumns>::table()
        .wired()
        .render_loaded(&cx, page)
        .await
        .html(&cx)
        .await;
    assert!(
        html.contains("Jan 15, 2024") && !html.contains("2024-01-15T"),
        "the timestamp reads as a day, got {html}"
    );
}
