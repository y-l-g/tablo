use toasty::Db;

use super::{
    super::common::{FormParts, MAX_FORM_BYTES},
    *,
};
use crate::panel::test_support::{Dummy, dummy_table, panel_for};

#[test]
fn form_values_decode_utf8_plus_and_encoded_separators() {
    // Multi-byte UTF-8: %C3%A9 must assemble to é, not the per-byte
    // mojibake `byte as char` would emit (item 6).
    let got = form_values_from_bytes(b"name=R%C3%A9mi");
    assert_eq!(got.get("name").map(String::as_str), Some("Rémi"));

    // `+` is a space; a literal plus is %2B — not double-decoded to space.
    let got = form_values_from_bytes(b"q=a+b&p=C%2B%2B");
    assert_eq!(got.get("q").map(String::as_str), Some("a b"));
    assert_eq!(got.get("p").map(String::as_str), Some("C++"));

    // Encoded separators survive as values.
    let got = form_values_from_bytes(b"a=1%262%3D3");
    assert_eq!(got.get("a").map(String::as_str), Some("1&2=3"));

    // Empty / blank input → empty map.
    assert!(form_values_from_bytes(b"").is_empty());
}

/// Build a request context carrying `content_type` and run the streaming
/// multipart parser over `body`.
///
/// `capture` mirrors the handler's "an uploader is installed" decision
/// the value half is what most of these tests read.
async fn multipart_parts(
    content_type: &str,
    body: Vec<u8>,
    capture: bool,
) -> Result<FormParts, topcoat::Error> {
    let (parts, ()) = http::Request::builder()
        .uri("/admin/users/create")
        .header(http::header::CONTENT_TYPE, content_type)
        .body(())
        .unwrap()
        .into_parts();
    let cx = topcoat::context::CxTestBuilder::new()
        .request_context(parts)
        .build();
    parse_multipart_values(&cx, Body::from(body), capture).await
}

/// The values half of [`multipart_parts`].
async fn multipart_values(
    content_type: &str,
    body: Vec<u8>,
) -> Result<HashMap<String, String>, topcoat::Error> {
    multipart_parts(content_type, body, false)
        .await
        .map(|parts| parts.values)
}

fn multipart_type(boundary: &str) -> String {
    format!("multipart/form-data; boundary={boundary}")
}

/// The multipart drain's byte accounting 413s one byte past the cap
/// (tripwire).
#[test]
fn multipart_drain_counts_bytes_and_413s_one_past_the_cap() {
    let mut seen = 0usize;
    count_form_bytes(&mut seen, MAX_FORM_BYTES / 2).unwrap();
    count_form_bytes(&mut seen, MAX_FORM_BYTES / 2).unwrap();
    assert_eq!(seen, MAX_FORM_BYTES);
    let err = count_form_bytes(&mut seen, 1).unwrap_err();
    assert!(
        err.downcast_ref::<topcoat::router::error::ContentTooLargeError>()
            .is_some(),
        "one byte past the cap must map to content-too-large (413), got {err}"
    );
    // A single chunk past the cap fires without a prior accumulation.
    let mut seen = 0usize;
    let err = count_form_bytes(&mut seen, MAX_FORM_BYTES + 1).unwrap_err();
    assert!(
        err.downcast_ref::<topcoat::router::error::ContentTooLargeError>()
            .is_some(),
        "a single over-cap chunk must 413, got {err}"
    );
}

/// An 11 MiB multipart upload 413s end to end through the panel (GH #149
/// acceptance). The installed `BodyLimit::max(MAX_FORM_BYTES)` layer and
/// the drain's own counter share the same threshold, so the body is over
/// both at once — the e2e pins the streaming path answers 413 rather than
/// draining; the counter's own accounting (which only answers if the
/// extractor's limit ever stops wrapping the stream — `BodyLimitKind` is
/// private, so no public configuration can disable it) is pinned by
/// [`count_form_bytes`]'s unit test above.
#[tokio::test]
async fn multipart_over_the_form_cap_413s_through_the_router() {
    use crate::resource::Resource;

    struct DummyResource;
    impl Resource for DummyResource {
        type Model = Dummy;
        type Form = DummyForm;
        fn form(_cx: &Cx) -> crate::schema::Schema {
            crate::schema::Schema::new(crate::schema::FileUpload::r#for(Dummy::fields().name()))
        }

        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_create(_cx: &Cx) -> bool {
            true
        }
        fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
            dummy_table(cx)
        }
    }
    #[derive(crate::RecordForm)]
    #[record_form(model = Dummy)]
    struct DummyForm {
        name: String,
    }
    let db = Db::builder().connect("sqlite::memory:").await.unwrap();
    let router = panel_for::<DummyResource>(db)
        .build()
        .expect("panel builds");
    let boundary = "----Boundary123";
    let payload = "x".repeat(MAX_FORM_BYTES + 1024);
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"name\"; filename=\"big.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n{payload}\r\n--{boundary}--\r\n"
    );
    let request = http::Request::builder()
        .method(http::Method::POST)
        .uri("/admin/dummies/create")
        .header(
            http::header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    let resp = router.handle(request).await;
    assert_eq!(
        resp.status(),
        http::StatusCode::PAYLOAD_TOO_LARGE,
        "an 11 MiB multipart upload must 413 through the router, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn multipart_stream_stores_text_and_filenames() {
    let boundary = "----Boundary123";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nHello\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"image_path\"; filename=\"photo.jpg\"\r\nContent-Type: image/jpeg\r\n\r\nBINARYBYTES\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"tags\"\r\n\r\nrust,async\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"tags\"\r\n\r\nsecond-wins\r\n\
             --{b}--\r\n",
        b = boundary
    );
    let got = multipart_values(&multipart_type(boundary), body.into_bytes())
        .await
        .unwrap();
    assert_eq!(got.get("title").map(String::as_str), Some("Hello"));
    // v1 stores the filename, not the bytes (FileUpload contract).
    assert_eq!(got.get("image_path").map(String::as_str), Some("photo.jpg"));
    assert_eq!(got.get("tags").map(String::as_str), Some("second-wins"));

    // Empty filename → empty value so `required` fires.
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"image_path\"; filename=\"\"\r\nContent-Type: application/octet-stream\r\n\r\n\r\n--{b}--\r\n",
        b = boundary
    );
    let got = multipart_values(&multipart_type(boundary), body.into_bytes())
        .await
        .unwrap();
    assert_eq!(got.get("image_path").map(String::as_str), Some(""));
}

/// file bytes are staged for the uploader only when one is
/// installed — otherwise today's drain-and-discard is what keeps a large
/// upload off the heap for every app that never installs one.
#[tokio::test]
async fn multipart_stages_file_bytes_only_when_capturing() {
    let boundary = "----CaptureBoundary";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nHello\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"image_path\"; filename=\"photo.jpg\"\r\nContent-Type: image/jpeg\r\n\r\nBINARYBYTES\r\n\
             --{b}--\r\n",
        b = boundary
    );

    // Capturing: the part's bytes and sanitized name reach the uploader's
    // staging area, and the value keeps the basename until an uploader
    // replaces it (the handler's step).
    let captured = multipart_parts(&multipart_type(boundary), body.clone().into_bytes(), true)
        .await
        .unwrap();
    let staged = captured.files.get("image_path").expect("staged file part");
    assert_eq!(staged.filename, "photo.jpg");
    assert_eq!(staged.bytes, b"BINARYBYTES");
    assert_eq!(
        captured.values.get("image_path").map(String::as_str),
        Some("photo.jpg")
    );
    assert!(
        !captured.files.contains_key("title"),
        "a text part is not a file part, got {:?}",
        captured.files.keys()
    );

    // Not capturing: same values, no bytes held.
    let drained = multipart_parts(&multipart_type(boundary), body.into_bytes(), false)
        .await
        .unwrap();
    assert!(drained.files.is_empty(), "no uploader, no buffering");
    assert_eq!(
        drained.values.get("image_path").map(String::as_str),
        Some("photo.jpg")
    );
}

/// A filename the framework refuses to persist sanitizes to empty,
/// and then nothing is staged: an uploader is never handed an empty name.
#[tokio::test]
async fn multipart_stages_nothing_for_a_rejected_filename() {
    let body = "--B\r\nContent-Disposition: form-data; name=\"image_path\"; filename=\"..\"\r\nContent-Type: application/octet-stream\r\n\r\nBYTES\r\n--B--\r\n";
    let parts = multipart_parts(&multipart_type("B"), body.as_bytes().to_vec(), true)
        .await
        .unwrap();
    assert!(parts.files.is_empty(), "a rejected name stages no bytes");
    assert_eq!(parts.values.get("image_path").map(String::as_str), Some(""));
}

#[tokio::test]
async fn multipart_stream_sanitizes_traversal_and_filename_star() {
    // Traversal filename lands sanitized.
    let body = "--B\r\nContent-Disposition: form-data; name=\"image_path\"; filename=\"../../../etc/passwd\"\r\nContent-Type: application/octet-stream\r\n\r\nBYTES\r\n--B--\r\n";
    let got = multipart_values(&multipart_type("B"), body.as_bytes().to_vec())
        .await
        .unwrap();
    assert_eq!(got.get("image_path").map(String::as_str), Some("passwd"));

    // RFC 5987 filename* decodes and wins over filename= (both present).
    let body = "--B\r\nContent-Disposition: form-data; name=\"image_path\"; filename=\"plain.jpg\"; filename*=UTF-8''%E2%82%ACphoto.jpg\r\nContent-Type: image/jpeg\r\n\r\nBYTES\r\n--B--\r\n";
    let got = multipart_values(&multipart_type("B"), body.as_bytes().to_vec())
        .await
        .unwrap();
    assert_eq!(
        got.get("image_path").map(String::as_str),
        Some("€photo.jpg"),
        "filename*=UTF-8 must decode and win, got {got:?}"
    );
    // Non-UTF-8 charset falls back to plain filename=.
    let body = "--B\r\nContent-Disposition: form-data; name=\"image_path\"; filename=\"plain.jpg\"; filename*=latin-1''%E9.jpg\r\nContent-Type: image/jpeg\r\n\r\nBYTES\r\n--B--\r\n";
    let got = multipart_values(&multipart_type("B"), body.as_bytes().to_vec())
        .await
        .unwrap();
    assert_eq!(
        got.get("image_path").map(String::as_str),
        Some("plain.jpg"),
        "unsupported charset must fall back, got {got:?}"
    );
}

#[tokio::test]
async fn multipart_stream_refuses_a_malformed_filename_star() {
    // A malformed `pct-encoded` triplet fails the ext-value whole (RFC
    // 5987), so the plain `filename=` wins. `%+1` is the case the strict
    // check adds: the removed decoder accepted `+` as a sign character.
    for malformed in ["%ZZ.jpg", "%+1.jpg"] {
        let body = format!(
            "--B\r\nContent-Disposition: form-data; name=\"image_path\"; \
                 filename=\"plain.jpg\"; filename*=UTF-8''{malformed}\r\n\
                 Content-Type: image/jpeg\r\n\r\nBYTES\r\n--B--\r\n"
        );
        let got = multipart_values(&multipart_type("B"), body.into_bytes())
            .await
            .unwrap();
        assert_eq!(
            got.get("image_path").map(String::as_str),
            Some("plain.jpg"),
            "a malformed filename* ({malformed}) must fall back to filename=, got {got:?}"
        );
    }
}

#[tokio::test]
async fn multipart_stream_rejects_missing_boundary() {
    // Bare multipart without boundary is a 400, not a silent urlencoded
    // fallback that turns binary bytes into confusing required-errors.
    assert!(
        multipart_values("multipart/form-data", b"name=x".to_vec())
            .await
            .is_err()
    );
}

#[test]
fn filenames_sanitize_to_basename_and_dispatch_guards_size() {
    assert_eq!(sanitize_filename("upload.jpg"), "upload.jpg");
    assert_eq!(sanitize_filename("../../../etc/cron.d/x"), "x");
    assert_eq!(sanitize_filename("/abs/path"), "path");
    assert_eq!(sanitize_filename("C:\\fakepath\\x"), "x");
    assert_eq!(sanitize_filename(""), "");
    // Names that could never be a safe persisted file are rejected to
    // empty: dot/dot-dot, and Windows reserved device names —
    // case-insensitively and with an extension too.
    assert_eq!(sanitize_filename("."), "");
    assert_eq!(sanitize_filename(".."), "");
    assert_eq!(sanitize_filename("../.."), "");
    assert_eq!(sanitize_filename("..."), "...");
    assert_eq!(sanitize_filename("con"), "");
    assert_eq!(sanitize_filename("NUL"), "");
    assert_eq!(sanitize_filename("Com1.txt"), "");
    assert_eq!(sanitize_filename("lpt9"), "");
    assert_eq!(sanitize_filename("console.txt"), "console.txt");
    assert_eq!(sanitize_filename("companion"), "companion");
    assert_eq!(sanitize_filename("...."), "....");
    // Cap keeps the tail without splitting a multibyte char: a naive
    // `[len - 255..]` slice panics here (the cut lands inside `é`).
    let multibyte = format!("{}{}", "é".repeat(200), "a".repeat(200));
    let capped = sanitize_filename(&multibyte);
    assert!(
        capped.len() <= 255,
        "cap must bound bytes, got {}",
        capped.len()
    );
    assert!(
        capped.ends_with('a'),
        "tail must be preserved, got {capped:?}"
    );
    // Over-cap body is rejected before buffering into maps.
    let big = vec![b'a'; MAX_FORM_BYTES + 1];
    assert!(
        form_values_from_request_parts(Some("application/x-www-form-urlencoded"), &big).is_err()
    );
    // Normal urlencoded still parses.
    let ok = form_values_from_request_parts(Some("application/x-www-form-urlencoded"), b"name=Ada")
        .unwrap();
    assert_eq!(ok.get("name").map(String::as_str), Some("Ada"));
}

#[test]
fn sanitize_filename_invariants_hold() {
    // GH #136 §5 property candidates: no `/` or `\`, ≤255 bytes, never
    // panics on multibyte input.
    for raw in [
        "a/b\\c".to_string(),
        "é".repeat(300),
        "../..".to_string(),
        "con".to_string(),
        " normal.jpg ".to_string(),
        "a".repeat(500),
        "\u{0}bad\nname\"".to_string(),
    ] {
        let out = sanitize_filename(&raw);
        assert!(
            !out.contains('/') && !out.contains('\\'),
            "separators must be gone, got {out:?} from {raw:?}"
        );
        assert!(
            out.len() <= 255,
            "cap must bound bytes, got {} from {raw:?}",
            out.len()
        );
        assert!(
            out.chars().all(|c| !c.is_control()),
            "controls must be stripped, got {out:?}"
        );
    }
}
