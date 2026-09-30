use topcoat::context::CxTestBuilder;

use super::*;
use crate::resource::{
    ColumnWidth, SelectFilter, Sort, TextColumn, VariantFilter,
    column::DEFAULT_WIDTH_BUDGET_PERCENT,
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

pub(crate) fn vehicule_filter() -> VariantFilter<Driver> {
    VariantFilter::r#for(
        "vehicule",
        "Véhicule",
        vec![
            ("Auto".to_string(), Driver::fields().vehicule().is_auto()),
            ("Moto".to_string(), Driver::fields().vehicule().is_moto()),
        ],
    )
}

pub(crate) fn status_table(_cx: &Cx) -> Table<Task> {
    Table::<Task>::new(
        |t| t.id.to_string(),
        TextColumn::r#for(Task::fields().title(), |t| t.title.clone()),
    )
    .filters(SelectFilter::r#for(
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

/// The layout a `<table ...>` tag declares, independent of attribute
/// order: the sorted `class`/`style` values the tag carries. Two tags
/// declaring the same layout compare equal even when the serializer
/// emits `style` before `class` in one and after it in the other.
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

/// Every whole-percent width a rendered table declares, in document order.
/// A length declaration is skipped: those carry a unit. A share paired
/// with a content floor (`width: 18%; min-width: 11rem`) still parses: the
/// share ends at the `;`, not at the attribute's closing quote.
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
    // columns need distinct names — title + status, not one
    // field twice.
    let tasks_table = Table::<Task>::new(
        |t| t.id.to_string(),
        (
            TextColumn::r#for(Task::fields().title(), |t: &Task| t.title.clone()).searchable(),
            TextColumn::r#for(Task::fields().status(), |t: &Task| t.status.clone()).sortable(),
        ),
    );
    // Use dummy rows for render check (no DB) — keyed by row.id
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
    // no Tailwind-class assertions. The chrome literals
    // (`rounded-xl`, `border-border`, `text-muted-foreground`,
    // `cursor-pointer`) are the showcase's business (#136), and pinning
    // them here meant every restyle broke a core test.
    //
    // Searchable columns render no extra header chrome: the
    // search input is the affordance, so the header cell holds its label
    // and nothing interactive. The sortable sibling next door *does* carry
    // an `<a>` and an icon, so this can fail.
    let title_at = html.find("Title").expect("the Title header");
    let title_th = html[..title_at].rfind("<th").expect("its <th>");
    let title_th_end = html[title_th..].find("</th>").expect("its </th>") + title_th;
    let title_head = &html[title_th..title_th_end];
    assert!(
        !title_head.contains("<svg") && !title_head.contains("<a "),
        "a searchable header must render no sort or loupe chrome, got {title_head}"
    );
    // Sortable ones carry the inactive `arrow-up-down` with
    // `aria-sort="none"`.
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

/// the table lays out fixed, and a declared column width reaches
/// the header cell and every row's cell as data — an inline `style`, never
/// a Tailwind class built at render.
#[tokio::test]
async fn table_lays_out_fixed_and_emits_declared_column_widths() {
    let cx = CxTestBuilder::new().build();
    let width_table = Table::<Task>::new(
        |t| t.id.to_string(),
        (
            // A field-backed column defaults to `Wide`: it declares no
            // width and takes the share the declared columns leave.
            TextColumn::r#for(Task::fields().title(), |t: &Task| t.title.clone()),
            // A computed column defaults to `Narrow`, overridden here.
            TextColumn::computed("Status", |t: &Task| t.status.clone())
                .width(ColumnWidth::Percent(30)),
        ),
    );
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
    // The fixed layout is the table's own contract, not paint: the
    // Done-when names it as the observable and a class is its only
    // transport, so this is the one class literal asserted here. The paint
    // classes stay the showcase's business (#136).
    let tag = table_tag(&html);
    assert!(
        tag.contains("table-fixed"),
        "the table must lay out fixed, got {tag}"
    );
    // The declared width is data on the header and on the row's cell: one
    // declaration, two carriers.
    assert_eq!(
        html.matches("style=\"width: 30%\"").count(),
        2,
        "the declared width must reach the th and the td, got {html}"
    );
    // The wide column declares nothing: an absent attribute, not a
    // generated class.
    assert_eq!(
        html.matches("style=\"width").count(),
        2,
        "only the declared column carries a width, got {html}"
    );
}

/// a column that declares nothing but its kind claims a share of
/// the table — a percentage, so it shrinks with the table instead of
/// outgrowing it — and the wide column beside it still declares none.
#[tokio::test]
async fn kind_defaults_claim_a_share_of_the_table() {
    let cx = CxTestBuilder::new().build();
    let default_table = Table::<Task>::new(
        |t| t.id.to_string(),
        (
            TextColumn::r#for(Task::fields().title(), |t: &Task| t.title.clone()),
            TextColumn::computed("Status", |t: &Task| t.status.clone()),
        ),
    );
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

/// the chrome columns declare a share of the table too — the
/// header row is the row `table-fixed` measures — and the share grows with
/// the number of row links, which sit side by side. The actions column
/// pairs its share with a content floor (`min-width: 11rem`), on the
/// header and on every row's cell, so the buttons fit instead of spilling
/// past the table on a narrow viewport.
#[tokio::test]
async fn chrome_columns_declare_their_widths() {
    // Each case: the row links to wire, the share Actions claims, and the
    // floor that holds its buttons.
    let cases: [(usize, &str, &str); 3] =
        [(1, "12%", "7rem"), (2, "18%", "11rem"), (3, "25%", "15rem")];
    for (links, expected, floor) in cases {
        let cx = CxTestBuilder::new().build();
        let mut chrome_table = Table::<User>::new(
            |u| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
        )
        .with_view("/admin/users".to_string());
        if links > 1 {
            chrome_table = chrome_table.with_edit("/admin/users".to_string());
        }
        if links > 2 {
            // Delete is what the bulk column pairs with.
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
        // The floor rides the header and every row's cell, so the buttons
        // fit whatever the share shrinks to.
        assert_eq!(
            html.matches(&format!("min-width: {floor}")).count(),
            2,
            "{links} row links must floor the actions column at {floor}, got {html}"
        );
        // The bulk checkbox claims its own share, and only when the table
        // renders one.
        let bulk = if links > 2 { 1 } else { 0 };
        assert_eq!(
            html.matches(&format!("style=\"width: {BULK_COLUMN_PERCENT}%\""))
                .count(),
            bulk,
            "the bulk column's share must follow the table's chrome, got {html}"
        );
    }
}

/// the kind defaults together stay inside their budget, whatever
/// the column set — a column that declares none is rendered at zero width
/// once the declared shares claim the whole table, header text included,
/// so the defaults scale down instead of spending the last percent.
#[tokio::test]
async fn kind_defaults_stay_inside_their_budget() {
    let cx = CxTestBuilder::new().build();
    // Four computed columns (4 × the 10% nominal) plus both chrome columns
    // (5% + 20%) overrun the budget, so every default is scaled down
    // together and the field column beside them keeps the rest.
    let crowded = Table::<Task>::new(
        |t| t.id.to_string(),
        (
            TextColumn::r#for(Task::fields().title(), |t: &Task| t.title.clone()),
            TextColumn::computed("Status", |t: &Task| t.status.clone()),
            TextColumn::computed("Featured", |t: &Task| t.featured.to_string()),
            TextColumn::computed("Created", |t: &Task| t.created_at.to_string()),
            TextColumn::computed("Id", |t: &Task| t.id.to_string()),
        ),
    )
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
    // One share per declared column, in the header row: the four computed
    // columns and the two chrome columns.
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
    // The field column declares nothing at all, so it takes what the
    // declared columns leave.
    let title_at = html.find(">Title<").expect("the Title header");
    let title_th = html[..title_at].rfind("<th").expect("its <th>");
    assert!(
        !html[title_th..title_at].contains("style="),
        "the field column must declare no width, got {}",
        &html[title_th..title_at]
    );
    // Every share rides its header cell, and each text column repeats its
    // own on the row's cell: four text columns twice, two chrome once.
    assert_eq!(
        declared_percents(&html).len(),
        10,
        "each share must reach its th and its td, got {html}"
    );
}

#[tokio::test]
async fn edit_links_render_beside_delete_in_actions_column() {
    // GH #162 (Filament's `recordActions` EditAction): `with_edit` wires
    // one `Edit` link per row into the shared Actions column.
    let cx = CxTestBuilder::new().build();
    let action_table = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    )
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
        html.contains(&format!("href=\"/admin/users/{id}/edit\"")) && html.contains(">Edit<"),
        "missing Edit link for {id} in {html}"
    );
    assert!(
        html.contains("Delete"),
        "Delete link must survive, got {html}"
    );
    // Without either prefix there is no Actions column at all — and a
    // chromeless table needs no `pk`: nothing emits URLs.
    let plain = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    );
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
    // the row policy gates the chrome per record, so a row the
    // resource refuses renders no Edit/Delete link and no bulk checkbox —
    // the rendered affordance and the route agree.
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
    let policy_table = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    )
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
    // The allowed row keeps all three links and an enabled checkbox.
    assert!(
        html.contains(&format!("href=\"/admin/users/{ada_id}/edit\""))
            && html.contains(&format!("href=\"/admin/users/{ada_id}\""))
            && html.contains(&format!("href=\"?delete={ada_id}\"")),
        "the allowed row must keep its View/Edit/Delete links, got {html}"
    );
    // The denied row keeps only the View link its policy allows: no Edit
    // link and no delete dialog opener.
    assert!(
        html.contains(&format!("href=\"/admin/users/{ken_id}\""))
            && !html.contains(&format!("/admin/users/{ken_id}/edit"))
            && !html.contains(&format!("delete={ken_id}")),
        "the denied row must render no Edit/Delete link, got {html}"
    );
    // Its row still renders, but with no checkbox at all: the one
    // `data-row-select` on the page is the allowed row's.
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
    // The allowed row's checkbox is present, so the count above is not
    // passing on a page with no bulk chrome at all.
    assert!(
        html.contains(&format!("value=\"{ada_id}\"")),
        "the allowed row must keep its checkbox, got {html}"
    );
}

/// The `<tr>…</tr>` chunk holding the row checkbox with `value`, without
/// its closing tag: the row's own cells scoped down from the page.
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
    // A row the policy locks out of every link keeps its actions cell all
    // the same: the cell stays aligned with the header instead of going
    // missing, and the row carries as many cells as the header.
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
    let policy_table = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    )
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
    // The locked row renders no action link at all. Found by its name
    // cell: it carries no checkbox value to search for.
    let ken_at = html.find(">Ken<").expect("the locked row");
    let ken_start = html[..ken_at].rfind("<tr").expect("its row");
    let ken_end = html[ken_at..].find("</tr>").expect("its end") + ken_at;
    let ken_row = &html[ken_start..ken_end];
    assert!(
        !ken_row.contains("/admin/users/"),
        "the locked row must render no action link at all, got {ken_row}"
    );
    // The allowed row keeps its links, so the absence above is not
    // passing on a page that renders no chrome at all.
    let ada_row = row_chunk(&html, &ada_id);
    assert!(
        ada_row.contains(&format!("/admin/users/{ada_id}/edit")),
        "the allowed row must keep its links, got {ada_row}"
    );
    // Alignment: the locked row carries a cell per header. Counted on
    // the closing tags: `<thead` itself opens with `<th`.
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
    // the policy is consulted only where chrome is wired, so a
    // resource that declares no chrome keeps its list page free of
    // per-record predicate calls — the coarse `TableChrome` gate is intact.
    let cx = CxTestBuilder::new().build();
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = calls.clone();
    let policy_table = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    )
    .row_actions(move |_: &User| {
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
async fn action_chrome_emits_record_keys_not_display_keys() {
    // a non-PK display projection drives keyed diffs and DOM ids
    // only — edit URLs, delete dialogs, and bulk values carry the record
    // projection handlers resolve as the typed PK.
    use topcoat::view::ViewExt;
    let cx = CxTestBuilder::new().build();
    let key_table = Table::<User>::new_split(
        |u| u.id.to_string().to_uppercase(),
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    )
    .with_delete("/admin/users".to_string())
    .with_edit("/admin/users".to_string())
    .with_bulk_delete(true);
    let rows = vec![User {
        id: uuid::Uuid::new_v4(),
        name: "Ada".to_string(),
    }];
    let lower = rows[0].id.to_string();
    let upper = lower.to_uppercase();
    let page: TablePage<User> = rows.into();
    let html = key_table
        .render(&cx, page)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    // URLs and bulk values: canonical PK text.
    assert!(
        html.contains(&format!("href=\"/admin/users/{lower}/edit\"")),
        "edit URL must carry the record key in {html}"
    );
    assert!(
        html.contains(&format!("value=\"{lower}\"")),
        "bulk value must carry the record key in {html}"
    );
    assert!(
        html.contains(&format!("?delete={lower}")),
        "delete dialog link must carry the record key in {html}"
    );
    assert!(
        !html.contains(&format!("value=\"{upper}\"")),
        "display key must never be a bulk value in {html}"
    );
    // Display key still drives the DOM identity.
    assert!(
        html.contains(&upper),
        "display key must still render (DOM/keyed diff) in {html}"
    );
}

#[tokio::test]
async fn pk_only_table_renders_display_from_record_key() {
    // a single-key table drives keyed diffs, DOM ids, and chrome URLs
    // from the one projection.
    use topcoat::view::ViewExt;
    let cx = CxTestBuilder::new().build();
    let tbl = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    )
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
    assert!(
        html.contains(&format!(
            "id=\"{}\"",
            crate::resource::state::row_dom_id(&key)
        )),
        "the row DOM id must fall back to the record key in {html}"
    );
}

#[tokio::test]
async fn id_only_non_pk_display_emits_display_urls() {
    // `new` declares both halves together, so a non-PK display without
    // `new_split` renders action URLs from the display value — handlers
    // 404 them. Authors must use `new_split` (see the tables guide).
    use topcoat::view::ViewExt;
    let cx = CxTestBuilder::new().build();
    let tbl = Table::<User>::new(
        |u| u.name.clone(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    )
    .with_delete("/admin/users".to_string())
    .with_edit("/admin/users".to_string())
    .with_bulk_delete(true);
    let page: TablePage<User> = vec![User {
        id: uuid::Uuid::new_v4(),
        name: "Ada".to_string(),
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
    assert!(
        html.contains("href=\"/admin/users/Ada/edit\""),
        "edit URL carries the display value without a split record key in {html}"
    );
    assert!(
        html.contains("value=\"Ada\""),
        "bulk value carries the display value without a split record key in {html}"
    );
    assert!(
        html.contains("?delete=Ada"),
        "delete dialog link carries the display value without a split record key in {html}"
    );
}

#[tokio::test]
async fn group_by_unknown_value_renders_no_headers_and_drops_param() {
    // `?group_by=` must name the declared group — any other
    // value renders no headers and vanishes from pager links instead of
    // silently grouping by the single declared key.
    let cx = CxTestBuilder::new().build();
    let grouped = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()).sortable(),
    )
    .group_by("status", |u| u.name.clone())
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
    // the page-local shim must actually group. The seed is
    // deliberately interleaved in query order (draft, published, draft,
    // published), so a legend-only shim — every header, then an ungrouped
    // table — cannot satisfy the ordering assertions below.
    let cx = CxTestBuilder::new().build();
    let grouped = Table::<Task>::new(
        |t| t.id.to_string(),
        TextColumn::r#for(Task::fields().title(), |t| t.title.clone()),
    )
    .group_by("status", |t| t.status.clone());
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
    // Rows are ordered by the group key (draft before published) and each
    // header sits immediately above its own rows — the stable sort keeps
    // the query's order inside a group.
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
    // The injected header carries an id derived from its group label, not
    // from its position, so the in-place morph can follow it:
    // the same contract the row ids have.
    for label in ["draft", "published"] {
        let expected = format!("id=\"{}\"", group_header_dom_id(label));
        assert!(
            html.contains(&expected),
            "the {label} header needs the stable id {expected:?}, got {html}"
        );
    }
}

/// a table render builds the row-action URLs from one shared base
/// — the encoded filter transport — before the row loop, so every row's
/// dialog opener is that base plus its own `delete=` key.
///
/// The base cannot be observed as a count: `filters_param` is a pure
/// function of the state, so a per-row rebuild produces identical bytes.
/// This pins the shape instead — every opener shares byte-identical bytes
/// before `delete=`, independent of the page size.
#[tokio::test]
async fn table_render_reuses_one_filter_transport_base_across_rows() {
    let cx = CxTestBuilder::new().build();
    let state = filters_state(&[("status", "published"), ("featured", "true")]);
    let tbl = Table::<User>::new(
        |u: &User| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u: &User| u.name.clone()),
    )
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
    // Each row's opener, keyed off the one attribute only an action link
    // carries, with the per-row `delete` key stripped: what is left is the
    // page's shared base.
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
    // The sorted, query-encoded transport every row's link must carry.
    let transport = "filters=featured%3Atrue%2Cstatus%3Apublished";
    for base in one_row.iter().chain(eight_rows.iter()) {
        assert!(
            base.contains(transport),
            "every opener must carry the page's filter transport, got {base}"
        );
        assert_eq!(
            *base, one_row[0],
            "rows must reuse the page's one encoded base, not rebuild it per row"
        );
    }
}

#[tokio::test]
async fn a_static_table_renders_no_runtime_bindings_at_all() {
    // a table without `live_search` has no shard to re-run, so a
    // mutation replaces its region with the response's. That is only sound
    // because the region is inert: no binding, no handler, nothing the
    // replacement could leave dead. The refresh control's absence is the
    // page's own answer to "can this table refresh in place?".
    let cx = CxTestBuilder::new().build();
    let tbl = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    )
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
    // every rendered row exposes its morph id; re-rendering the
    // same page yields the same ids.
    let cx = CxTestBuilder::new().build();
    let tbl = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    );
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
