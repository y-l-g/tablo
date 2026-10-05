use topcoat::context::CxTestBuilder;

use super::{
    super::widths::{BULK_COLUMN_PERCENT, DEFAULT_WIDTH_BUDGET_PERCENT},
    *,
};
use crate::{
    ComputedColumn, lens,
    table::{ColumnWidth, QueryFilter, RowActions, SelectFilter, Sort, TextColumn},
};

#[derive(Debug, Clone, toasty::Model)]
pub(crate) struct User {
    #[key]
    #[auto]
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
}

#[derive(Debug, Clone, toasty::Model)]
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
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ..TableState::default()
    }
}

pub(crate) fn last_link_named<'a>(html: &'a str, label: &str) -> &'a str {
    html.rsplit('<')
        .find(|chunk| chunk.contains(label))
        .unwrap_or_else(|| panic!("missing {label} link in {html}"))
}

/// The `<table …>` opening tag of a rendered table, without its children.
pub(crate) fn table_tag(html: &str) -> &str {
    let start = html.find("<table").expect("the table element");
    let end = html[start..].find('>').expect("its tag end") + start;
    &html[start..end]
}

/// Return the layout a `<table ...>` tag declares, independent of attribute order.
pub(crate) fn normalized_table_tag(tag: &str) -> (String, String) {
    (table_attr(tag, "class"), table_attr(tag, "style"))
}

/// The value of one quoted attribute inside a tag, or empty when absent.
pub(crate) fn table_attr(tag: &str, name: &str) -> String {
    let marker = format!("{name}=\"");
    let Some(at) = tag.find(&marker) else {
        return String::new();
    };
    let rest = &tag[at + marker.len()..];
    let mut classes: Vec<&str> = rest
        .split('"')
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    classes.sort_unstable();
    classes.join(" ")
}

/// Return every whole-percent width a rendered table declares, in document order.
pub(crate) fn declared_percents(html: &str) -> Vec<u32> {
    html.match_indices("style=\"width: ")
        .filter_map(|(at, marker)| {
            html[at + marker.len()..]
                .split(['"', ';'])
                .next()?
                .strip_suffix('%')?
                .parse()
                .ok()
        })
        .collect()
}

#[tokio::test]
async fn table_for_columns_renders_with_keyed_rows() {
    let cx = CxTestBuilder::new().build();
    let tasks_table = Table::<Task>::new((
        TextColumn::new(lens!(Task.title)).searchable(),
        TextColumn::new(lens!(Task.status)).sortable(),
    ));
    let rows = vec![
        Task {
            id: uuid::Uuid::new_v4(),
            title: "Ada".to_string(),
            status: "draft".to_string(),
            featured: false,
            created_at: jiff::Timestamp::now(),
        },
        Task {
            id: uuid::Uuid::new_v4(),
            title: "Bob".to_string(),
            status: "published".to_string(),
            featured: true,
            created_at: jiff::Timestamp::now(),
        },
    ];
    let page: TablePage<Task> = rows.clone().into();
    let html = tasks_table
        .render(&cx, page)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    let title_at = html.find("Title").expect("the Title header");
    let title_th = html[..title_at].rfind("<th").expect("its <th>");
    let title_th_end = html[title_th..].find("</th>").expect("its </th>") + title_th;
    let title_head = &html[title_th..title_th_end];
    assert!(
        !title_head.contains("<svg") && !title_head.contains("<a "),
        "a searchable header must render no sort or loupe chrome, got {title_head}"
    );
    assert!(
        html.contains("aria-sort=\"none\""),
        "missing sortable indicator in {html}"
    );
    assert!(html.contains("Title"), "missing Title header in {html}");
    assert!(html.contains("Status"), "missing Status header in {html}");
    for row in &rows {
        assert!(
            html.contains(&row.title),
            "missing row title {} in {html}",
            row.title
        );
    }
}

/// Lay the table out fixed and emit declared column widths on the header and row cells.
#[tokio::test]
async fn table_lays_out_fixed_and_emits_declared_column_widths() {
    let cx = CxTestBuilder::new().build();
    let width_table = Table::<Task>::new((
        TextColumn::new(lens!(Task.title)),
        ComputedColumn::new("Status", |t: &Task| t.status.clone()).width(ColumnWidth::Percent(30)),
    ));
    let page: TablePage<Task> = vec![Task {
        id: uuid::Uuid::new_v4(),
        title: "Ada".to_string(),
        status: "draft".to_string(),
        featured: false,
        created_at: jiff::Timestamp::now(),
    }]
    .into();
    let html = width_table
        .render(&cx, page)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    let tag = table_tag(&html);
    assert!(
        tag.contains("table-fixed"),
        "the table must lay out fixed, got {tag}"
    );
    assert_eq!(
        html.matches("style=\"width: 30%\"").count(),
        2,
        "the declared width must reach the th and the td, got {html}"
    );
    assert_eq!(
        html.matches("style=\"width").count(),
        2,
        "only the declared column carries a width, got {html}"
    );
}

/// Claim a table share for kind defaults while wide columns declare none.
#[tokio::test]
async fn kind_defaults_claim_a_share_of_the_table() {
    let cx = CxTestBuilder::new().build();
    let default_table = Table::<Task>::new((
        TextColumn::new(lens!(Task.title)),
        ComputedColumn::new("Status", |t: &Task| t.status.clone()),
    ));
    let page: TablePage<Task> = vec![Task {
        id: uuid::Uuid::new_v4(),
        title: "Ada".to_string(),
        status: "draft".to_string(),
        featured: false,
        created_at: jiff::Timestamp::now(),
    }]
    .into();
    let html = default_table
        .render(&cx, page)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert_eq!(
        declared_percents(&html),
        [10, 10],
        "a computed column claims its kind's share on the th and the td, got {html}"
    );
    assert_eq!(
        html.matches("style=\"width").count(),
        2,
        "the field column must declare nothing, got {html}"
    );
}

/// Declare a share for the chrome columns, pairing the actions share with a content floor.
#[tokio::test]
async fn chrome_columns_declare_their_widths() {
    let cases: [(usize, &str, &str); 3] =
        [(1, "8%", "4rem"), (2, "12%", "7rem"), (3, "15%", "9rem")];
    for (links, expected, floor) in cases {
        let cx = CxTestBuilder::new().build();
        let mut chrome_table = Table::<User>::new(TextColumn::new(lens!(User.name)))
            .with_view("/admin/users".to_string());
        if links > 1 {
            chrome_table = chrome_table.with_edit("/admin/users".to_string());
        }
        if links > 2 {
            chrome_table = chrome_table
                .with_delete("/admin/users".to_string())
                .with_bulk_delete(true);
        }
        let page: TablePage<User> = vec![User {
            id: uuid::Uuid::new_v4(),
            name: "Ada".to_string(),
        }]
        .into();
        let html = chrome_table
            .render(&cx, page)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert_eq!(
            html.matches(&format!("width: {expected}")).count(),
            1,
            "{links} row links must claim {expected} in the header row, got {html}"
        );
        assert_eq!(
            html.matches(&format!("min-width: {floor}")).count(),
            2,
            "{links} row links must floor the actions column at {floor}, got {html}"
        );
        let bulk = if links > 2 { 1 } else { 0 };
        assert_eq!(
            html.matches(&format!("style=\"width: {BULK_COLUMN_PERCENT}%\""))
                .count(),
            bulk,
            "the bulk column's share must follow the table's chrome, got {html}"
        );
    }
}

/// Keep kind defaults inside their budget whatever the column set.
#[tokio::test]
async fn kind_defaults_stay_inside_their_budget() {
    let cx = CxTestBuilder::new().build();
    let crowded = Table::<Task>::new((
        TextColumn::new(lens!(Task.title)),
        ComputedColumn::new("Status", |t: &Task| t.status.clone()),
        ComputedColumn::new("Featured", |t: &Task| t.featured.to_string()),
        ComputedColumn::new("Created", |t: &Task| t.created_at.to_string()),
        ComputedColumn::new("Id", |t: &Task| t.id.to_string()),
    ))
    .with_delete("/admin/tasks".to_string())
    .with_edit("/admin/tasks".to_string())
    .with_view("/admin/tasks".to_string())
    .with_bulk_delete(true);
    let page: TablePage<Task> = vec![Task {
        id: uuid::Uuid::new_v4(),
        title: "Ada".to_string(),
        status: "draft".to_string(),
        featured: false,
        created_at: jiff::Timestamp::now(),
    }]
    .into();
    let html = crowded
        .render(&cx, page)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    let thead_at = html.find("<thead").expect("a header row");
    let thead_end = html.find("</thead>").expect("its end");
    let percents = declared_percents(&html[thead_at..thead_end]);
    assert_eq!(
        percents.len(),
        6,
        "one share per declared column, got {percents:?} in {html}"
    );
    assert!(
        percents.iter().all(|percent| *percent > 0),
        "a scaled share must keep its column visible, got {percents:?}"
    );
    let total: u32 = percents.iter().sum();
    assert!(
        total <= u32::from(DEFAULT_WIDTH_BUDGET_PERCENT),
        "the kind defaults must leave the field column a share, got {percents:?}"
    );
    let title_at = html.find(">Title<").expect("the Title header");
    let title_th = html[..title_at].rfind("<th").expect("its <th>");
    assert!(
        !html[title_th..title_at].contains("style="),
        "the field column must declare no width, got {}",
        &html[title_th..title_at]
    );
    assert_eq!(
        declared_percents(&html).len(),
        10,
        "each share must reach its th and its td, got {html}"
    );
}

#[tokio::test]
async fn edit_links_render_beside_delete_in_actions_column() {
    let cx = CxTestBuilder::new().build();
    let action_table = Table::<User>::new(TextColumn::new(lens!(User.name)))
        .with_delete("/admin/users".to_string())
        .with_edit("/admin/users".to_string());
    let rows = vec![User {
        id: uuid::Uuid::new_v4(),
        name: "Ada".to_string(),
    }];
    let id = rows[0].id.to_string();
    let page: TablePage<User> = rows.into();
    let html = action_table
        .render(&cx, page)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(html.contains("Actions"), "missing Actions header in {html}");
    assert!(
        html.contains(&format!("href=\"/admin/users/{id}/edit\""))
            && html.contains("aria-label=\"Edit\""),
        "missing Edit link for {id} in {html}"
    );
    assert!(
        html.contains("Delete"),
        "Delete link must survive, got {html}"
    );
    let plain = Table::<User>::new(TextColumn::new(lens!(User.name)));
    let page: TablePage<User> = vec![User {
        id: uuid::Uuid::new_v4(),
        name: "Ada".to_string(),
    }]
    .into();
    let html = plain
        .render(&cx, page)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        !html.contains("Actions") && !html.contains(">Edit<"),
        "plain table must not render action chrome, got {html}"
    );
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
    let html = policy_table
        .render(&cx, page)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains(&format!("href=\"/admin/users/{ada_id}/edit\""))
            && html.contains(&format!("href=\"/admin/users/{ada_id}\""))
            && html.contains(&format!("href=\"?delete={ada_id}\"")),
        "the allowed row must keep its View/Edit/Delete links, got {html}"
    );
    assert!(
        html.contains(&format!("href=\"/admin/users/{ken_id}\""))
            && !html.contains(&format!("/admin/users/{ken_id}/edit"))
            && !html.contains(&format!("delete={ken_id}")),
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
        html.matches("data-row-select").count(),
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
    let html = policy_table
        .render(&cx, page)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
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
    let policy_table =
        Table::<User>::new(TextColumn::new(lens!(User.name))).row_actions(move |_: &User| {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            RowActions::ALL
        });
    let page: TablePage<User> = vec![User {
        id: uuid::Uuid::new_v4(),
        name: "Ada".to_string(),
    }]
    .into();
    let html = policy_table
        .render(&cx, page)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
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
    use topcoat::view::ViewExt;
    let cx = CxTestBuilder::new().build();
    let tbl = Table::<User>::new(TextColumn::new(lens!(User.name)))
        .with_delete("/admin/users".to_string())
        .with_edit("/admin/users".to_string())
        .with_bulk_delete(true);
    let rows = vec![User {
        id: uuid::Uuid::new_v4(),
        name: "Ada".to_string(),
    }];
    let key = rows[0].id.to_string();
    let page: TablePage<User> = rows.into();
    let html = tbl
        .render(&cx, page)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains(&format!("href=\"/admin/users/{key}/edit\"")),
        "edit URL must carry the record key in {html}"
    );
    assert!(
        html.contains(&format!("value=\"{key}\"")),
        "bulk value must carry the record key in {html}"
    );
    assert!(
        html.contains(&format!("?delete={key}")),
        "delete dialog link must carry the record key in {html}"
    );
}

#[tokio::test]
async fn composite_key_rows_render_no_action() {
    use topcoat::view::ViewExt;

    #[derive(Debug, Clone, toasty::Model)]
    #[key(owner, slot)]
    struct Seat {
        owner: String,
        slot: i64,
        label: String,
    }

    let cx = CxTestBuilder::new().build();
    let tbl = Table::<Seat>::new(TextColumn::new(lens!(Seat.label)))
        .with_delete("/admin/seats".to_string())
        .with_edit("/admin/seats".to_string())
        .with_bulk_delete(true);
    let page: TablePage<Seat> = vec![Seat {
        owner: "ada".to_string(),
        slot: 2,
        label: "Aisle".to_string(),
    }]
    .into();
    let html = tbl
        .render(&cx, page)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(html.contains("Aisle"), "the row renders in {html}");
    assert!(
        !html.contains("/admin/seats/") && !html.contains("data-row-select"),
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
    };
    let html = grouped
        .render_with_state(&cx, page, &state, "/admin/users")
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
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
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
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

/// Reuse one list URL base for row-action URLs across rows.
#[tokio::test]
async fn table_render_reuses_one_list_url_base_across_rows() {
    let cx = CxTestBuilder::new().build();
    let state = filters_state(&[("status", "published"), ("featured", "true")]);
    let tbl = Table::<User>::new(TextColumn::new(lens!(User.name)))
        .with_delete("/admin/users".to_string());
    let rows = |n: usize| -> Vec<User> {
        (0..n)
            .map(|i| User {
                id: uuid::Uuid::from_u128(i as u128),
                name: format!("user-{i}"),
            })
            .collect()
    };
    let render = async |page: TablePage<User>| {
        tbl.render_with_state(&cx, page, &state, "/admin/users")
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx)
    };
    fn delete_bases(html: &str) -> Vec<&str> {
        html.split("href=\"")
            .skip(1)
            .filter_map(|chunk| chunk.split('"').next())
            .filter(|href| href.contains("delete="))
            .map(|href| href.split("delete=").next().unwrap())
            .collect()
    }

    let one_html = render(TablePage::from(rows(1))).await;
    let eight_html = render(TablePage::from(rows(8))).await;
    let one_row = delete_bases(&one_html);
    let eight_rows = delete_bases(&eight_html);
    assert_eq!(one_row.len(), 1, "one row, one dialog opener");
    assert_eq!(eight_rows.len(), 8, "eight rows, eight dialog openers");
    let transport = "f.featured=true&amp;f.status=published";
    for base in one_row.iter().chain(eight_rows.iter()) {
        assert!(
            base.contains(transport),
            "every opener must carry the page's filters, got {base}"
        );
        assert_eq!(
            *base, one_row[0],
            "rows must reuse the page's one encoded base, not rebuild it per row"
        );
    }
}

#[tokio::test]
async fn a_static_table_renders_no_runtime_bindings_at_all() {
    let cx = CxTestBuilder::new().build();
    let tbl = Table::<User>::new(TextColumn::new(lens!(User.name)))
        .with_delete("/admin/users".to_string())
        .with_bulk_delete(true);
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
        !html.contains("data-table-revision"),
        "a static table must carry no refresh control, got {html}"
    );
    assert!(
        !html.contains("data-topcoat-"),
        "a static table's region must be inert markup, got {html}"
    );
}

#[tokio::test]
async fn rendered_rows_carry_stable_dom_ids() {
    let cx = CxTestBuilder::new().build();
    let tbl = Table::<User>::new(TextColumn::new(lens!(User.name)));
    let rows = vec![
        User {
            id: uuid::Uuid::nil(),
            name: "Ada".to_string(),
        },
        User {
            id: uuid::Uuid::max(),
            name: "Alan".to_string(),
        },
    ];
    let render = async |rows: Vec<User>| {
        tbl.render_with_state(&cx, rows.into(), &TableState::default(), "/admin/users")
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx)
    };
    let first = render(rows.clone()).await;
    assert!(
        first.contains("id=\"row-00000000-0000-0000-0000-000000000000-"),
        "missing morph id for first row, got {first}"
    );
    assert!(
        first.contains("id=\"row-ffffffff-ffff-ffff-ffff-ffffffffffff-"),
        "missing morph id for second row, got {first}"
    );
    let second = render(rows).await;
    assert_eq!(
        first.matches("id=\"row-").count(),
        second.matches("id=\"row-").count(),
        "reruns must keep stable row ids"
    );
}
