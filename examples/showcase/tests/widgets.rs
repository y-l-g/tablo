//! The dashboard's widget grid: which widgets a request sees, what its stat tiles and chart
//! count, and which posts its table lists.

use showcase::models::BLOCKED_TENANT;
use tablo::testing::{ChartSeries, StatTile, charts, record_tables, stats, widgets};

use crate::common::{body_string, demo_client, routers::router_for_tests as router, tenanted_db};

/// The tile linking to `href`, if the page renders one.
fn tile<'a>(tiles: &'a [StatTile], href: &str) -> Option<&'a StatTile> {
    tiles.iter().find(|tile| tile.href.as_deref() == Some(href))
}

/// Each tenant's tiles count only its own records, and link to the list they count.
#[tokio::test]
async fn stat_tiles_count_the_tenants_own_records() {
    let (db, t1, t2) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    for tenant in [t1, t2] {
        let html = body_string(client.tenant(tenant).get("/admin").await).await;
        assert_eq!(
            widgets(&html),
            ["overview", "posts-per-month", "latest-posts"]
        );
        let tiles = stats(&html);
        for href in ["/admin/posts", "/admin/authors"] {
            let tile = tile(&tiles, href).unwrap_or_else(|| panic!("a tile for {href}: {tiles:?}"));
            assert_eq!(tile.value, "1", "one record of {href} per tenant");
        }
    }
}

/// A request the post policy refuses sees no widget or tile over posts, and none over the
/// authors the same policy guards.
#[tokio::test]
async fn a_refused_request_sees_no_widget_or_tile_over_the_refused_resource() {
    let (db, _, _) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let response = client.tenant(BLOCKED_TENANT).get("/admin").await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let html = body_string(response).await;
    assert_eq!(widgets(&html), ["overview"]);
    assert_eq!(record_tables(&html), []);
    let tiles = stats(&html);
    for href in ["/admin/posts", "/admin/authors"] {
        assert!(
            tile(&tiles, href).is_none(),
            "no tile for {href}: {tiles:?}"
        );
    }
    assert!(
        !tiles.is_empty(),
        "the tiles the request may see still render"
    );
}

/// Each tenant's chart counts only its own posts: tenant 1's one published post of January 2024,
/// tenant 2's one draft of June 2024, each ending the six months its chart spans.
#[tokio::test]
async fn the_posts_chart_counts_the_tenants_own_posts() {
    let (db, t1, t2) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    for (tenant, last, published, draft) in [(t1, "Jan", "1", "0"), (t2, "Jun", "0", "1")] {
        let html = body_string(client.tenant(tenant).get("/admin").await).await;
        let charts = charts(&html);
        assert_eq!(charts.len(), 1, "{charts:?}");
        let chart = &charts[0];
        assert_eq!(chart.kind, "bar");
        assert_eq!(chart.heading.as_deref(), Some("Posts per month"));
        assert_eq!(chart.labels.len(), 6);
        assert_eq!(chart.labels.last().map(String::as_str), Some(last));
        let last_month = |series: &ChartSeries| series.values.last().cloned();
        let by_label: Vec<(&str, Option<String>)> = chart
            .series
            .iter()
            .map(|series| (series.label.as_str(), last_month(series)))
            .collect();
        assert_eq!(
            by_label,
            [
                ("Published", Some(published.to_string())),
                ("Draft", Some(draft.to_string())),
            ]
        );
        let total: u32 = chart
            .series
            .iter()
            .flat_map(|series| &series.values)
            .map(|value| value.parse::<u32>().expect("a count"))
            .sum();
        assert_eq!(total, 1, "only the tenant's own post: {chart:?}");
    }
}

/// Each tenant's latest posts list only its own post, under the columns the widget names, and
/// link to the post list.
#[tokio::test]
async fn the_latest_posts_list_the_tenants_own_posts() {
    let (db, t1, t2) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    for (tenant, title) in [(t1, "T1 Post"), (t2, "T2 Post")] {
        let html = body_string(client.tenant(tenant).get("/admin").await).await;
        let tables = record_tables(&html);
        assert_eq!(tables.len(), 1, "{tables:?}");
        let table = &tables[0];
        assert_eq!(table.resource, "posts");
        assert_eq!(table.heading, "Latest posts");
        assert_eq!(table.columns, ["title", "author", "status"]);
        assert_eq!(table.list.as_deref(), Some("/admin/posts"));
        let titles: Vec<&str> = table.rows.iter().map(|row| row.cells[0].as_str()).collect();
        assert_eq!(titles, [title], "{table:?}");
    }
}
