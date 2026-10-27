//! The dashboard's widget grid: which widgets a request sees, and what its stat tiles count.

use showcase::models::BLOCKED_TENANT;
use tablo::testing::{StatTile, stats, widgets};

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
        assert_eq!(widgets(&html), ["overview", "latest-posts"]);
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
