use crate::common::{body_string, demo_client, full_db, routers::router_for_tests as router};

#[tokio::test]
async fn posts_export_bom_opt_in_prepends_bom() {
    // `?bom=1` opts into a UTF-8 BOM for Excel; default stays BOM-free.
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/posts/export?bom=1").await;
    assert!(resp.status().is_success());
    let csv = body_string(resp).await;
    assert!(
        csv.starts_with('\u{FEFF}'),
        "bom=1 export must start with BOM, got {csv:?}"
    );

    for uri in ["/admin/posts/export", "/admin/posts/export?bom=0"] {
        let csv = body_string(client.get(uri).await).await;
        assert!(
            !csv.starts_with('\u{FEFF}'),
            "{uri} must stay BOM-free, got {csv:?}"
        );
    }
}

#[tokio::test]
async fn posts_export_streams_csv_with_content_disposition() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/posts/export").await;
    assert!(
        resp.status().is_success(),
        "export should be 200, got {}",
        resp.status()
    );
    let content_type = resp
        .headers()
        .get(http::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("text/csv"),
        "content-type should be text/csv, got {}",
        content_type
    );
    assert!(
        content_type.contains("charset=utf-8"),
        "content-type should declare utf-8 for non-ASCII cells, got {}",
        content_type
    );
    let disposition = resp
        .headers()
        .get(http::header::CONTENT_DISPOSITION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        disposition.contains("attachment"),
        "should be attachment, got {}",
        disposition
    );
    assert!(
        disposition.contains("posts.csv"),
        "filename should be posts.csv, got {}",
        disposition
    );
    // Hardening headers: bodies must never be sniffed as HTML.
    assert_eq!(
        resp.headers()
            .get("x-content-type-options")
            .and_then(|v| v.to_str().ok()),
        Some("nosniff"),
        "export must carry nosniff"
    );
    let csv = body_string(resp).await;
    // Header row with column labels (Title, Author, etc.)
    let header = csv.lines().next().expect("the export writes a header row");
    assert!(
        header.contains("Title"),
        "missing Title header, got {header}"
    );
    assert!(
        header.contains("Author"),
        "missing Author header, got {header}"
    );
    // Data rows should include Hello Toasty and author name via include
    assert!(csv.contains("Hello Toasty"), "missing post title {}", csv);
    assert!(
        csv.contains("Ada Author"),
        "missing author name via include {}",
        csv
    );
    // Every relation-reading column declares the include its projection reads
    // so the export's narrowed query loads them all and no cell
    // falls back to the unloaded marker (the columns' `debug_assert` is the
    // other half of that contract — it panics first).
    assert!(
        !csv.contains("(unloaded)"),
        "a declared include must reach the export query, got {}",
        csv
    );
    // A filter narrows the export: only the published post survives.
    let resp = client.get("/admin/posts/export?f.status=published").await;
    let csv = body_string(resp).await;
    assert!(
        csv.contains("Hello Toasty"),
        "filtered export should contain published {}",
        csv
    );
    assert!(
        !csv.contains("Second Post"),
        "filtered export should not contain draft {}",
        csv
    );
}
