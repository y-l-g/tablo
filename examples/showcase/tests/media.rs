//! The media library: the app-level upload path writes a `medias` row and
//! the stored rows render a thumbnail or a link.

use http_body_util::BodyExt;
use showcase::{media::MEDIA_PATH, models::MediaAsset};
use topcoat::router::{Body, Router};

use crate::common::{
    TestClient, body_string, demo_client, full_db, routers::router_with_app_uploads,
    tenantless_client,
};

/// Bytes of an uploaded file: ASCII, so the multipart body can be a `String`,
/// which is all the test client takes.
const PAYLOAD: &str = "PNG-FAKE-BYTES";

/// How many media rows the database holds.
async fn media_count(db: &toasty::Db) -> usize {
    let mut db = db.clone();
    MediaAsset::all().exec(&mut db).await.unwrap().len()
}

/// POST one multipart upload as the page's form does, returning the response.
///
/// `csrf` is the token to embed; the matching cookie goes on the client. `None`
/// posts no token at all, which is the forged-request case.
async fn post_upload(
    client: &TestClient<'_>,
    filename: &str,
    content_type: &str,
    payload: &str,
    csrf: Option<&str>,
) -> http::Response<Body> {
    let client = match csrf {
        Some(token) => client.csrf(token),
        None => client.clone(),
    };
    let boundary = "----MediaBoundary";
    // Ad-hoc, not `tablo::testing::multipart_body`: the file part carries a
    // caller-chosen `Content-Type`, which the kind assertions need.
    let token = csrf
        .map(|token| {
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"csrf_token\"\r\n\r\n{token}\r\n"
            )
        })
        .unwrap_or_default();
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n{payload}\r\n\
         {token}--{b}--\r\n",
        b = boundary,
    );
    client.post_multipart(MEDIA_PATH, boundary, body).await
}

/// One upload with a freshly minted CSRF pair, as the demo admin.
async fn upload(
    router: &Router,
    db: &toasty::Db,
    filename: &str,
    content_type: &str,
    payload: &str,
) -> http::Response<Body> {
    let client = demo_client(router, db).await;
    let csrf = uuid::Uuid::new_v4().to_string();
    post_upload(&client, filename, content_type, payload, Some(&csrf)).await
}

#[tokio::test]
async fn an_upload_without_the_csrf_token_is_refused() {
    let db = full_db().await;
    let router = router_with_app_uploads(db.clone());
    let before = media_count(&db).await;
    let client = demo_client(&router, &db).await;

    let response = post_upload(&client, "cover.png", "image/png", PAYLOAD, None).await;

    assert_eq!(response.status(), 403, "the app's own form is CSRF-checked");
    assert_eq!(media_count(&db).await, before, "and writes no row");
}

#[tokio::test]
async fn a_tenantless_request_is_refused() {
    let db = full_db().await;
    let router = router_with_app_uploads(db.clone());
    let client = tenantless_client(&router, &db).await;

    assert_eq!(
        client.get(MEDIA_PATH).await.status(),
        403,
        "the library lists one tenant's media, so a tenantless request is refused"
    );

    let before = media_count(&db).await;
    let response = post_upload(
        &client,
        "cover.png",
        "image/png",
        PAYLOAD,
        Some(&uuid::Uuid::new_v4().to_string()),
    )
    .await;
    assert_eq!(response.status(), 403);
    assert_eq!(media_count(&db).await, before, "and writes no row");
}

#[tokio::test]
async fn a_client_filename_is_stored_as_a_basename_inside_the_served_directory() {
    let db = full_db().await;
    let router = router_with_app_uploads(db.clone());

    upload(&router, &db, "../../escape.png", "image/png", PAYLOAD).await;

    let mut db_q = db.clone();
    let row = MediaAsset::all().exec(&mut db_q).await.unwrap().remove(0);
    assert_eq!(row.filename, "escape.png", "the row keeps the basename");
    assert!(
        !row.path.contains(".."),
        "the stored URL cannot climb out of the served directory, got {}",
        row.path
    );

    // Serving it back is what proves it landed inside the directory the panel
    // serves: a file written anywhere else is not reachable at this path.
    let client = demo_client(&router, &db).await;
    assert_eq!(
        client.get(&row.path).await.status(),
        200,
        "{} must be inside the served directory",
        row.path
    );
}

#[tokio::test]
async fn a_filename_that_would_break_the_url_still_fetches_back() {
    let db = full_db().await;
    let router = router_with_app_uploads(db.clone());
    let client = demo_client(&router, &db).await;

    // Every name here reaches the store as the browser sent it, and every one
    // must come back: `#` would start a fragment, a space would end the URL,
    // `%22` is what Chrome sends for a quote, and a `%` would decode to
    // something the file on disk is not named.
    for (sent, recorded) in [
        ("cover #1.png", "cover #1.png"),
        (
            "quote%22 onerror=%22boom.png",
            "quote%22 onerror=%22boom.png",
        ),
        ("100%.png", "100%.png"),
        ("trailing .png ", "trailing .png"),
    ] {
        let csrf = uuid::Uuid::new_v4().to_string();
        let response = post_upload(&client, sent, "image/png", PAYLOAD, Some(&csrf)).await;
        assert!(
            response.status().is_redirection(),
            "{sent:?} must save, got {}",
            response.status()
        );

        let mut db_q = db.clone();
        let rows = MediaAsset::all().exec(&mut db_q).await.unwrap();
        let row = rows
            .iter()
            .find(|row| row.filename == recorded)
            .unwrap_or_else(|| panic!("no row recorded {recorded:?} for {sent:?}"));
        assert!(
            !row.path.contains(['#', '"', ' ']),
            "{sent:?} stored a path that is not one URL segment: {}",
            row.path
        );

        let response = client.get(&row.path).await;
        assert_eq!(
            response.status(),
            200,
            "{} must resolve to the stored bytes",
            row.path
        );
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("collect the served file")
            .to_bytes();
        assert_eq!(
            bytes.as_ref(),
            PAYLOAD.as_bytes(),
            "{} served the wrong bytes",
            row.path
        );
    }
}

#[tokio::test]
async fn the_library_lists_one_tenants_rows() {
    let db = full_db().await;
    let router = router_with_app_uploads(db.clone());

    // Another tenant uploads: the row carries the tenant that uploaded it.
    let other = uuid::Uuid::from_u128(4242);
    let csrf = uuid::Uuid::new_v4().to_string();
    let response = post_upload(
        &demo_client(&router, &db).await.tenant(other),
        "avatar.png",
        "image/png",
        PAYLOAD,
        Some(&csrf),
    )
    .await;
    assert!(
        response.status().is_redirection(),
        "the upload must save for its own tenant, got {}",
        response.status()
    );

    let html = body_string(demo_client(&router, &db).await.get(MEDIA_PATH).await).await;
    assert!(
        html.contains("No media has been uploaded yet."),
        "another tenant's media must not be listed: {html}"
    );
}
