use super::*;

const CAP: usize = 1024;

/// The urlencoded `bytes`' last value per key.
fn form_values_from_bytes(bytes: &[u8]) -> HashMap<String, String> {
    form_pairs_from_request_parts(None, bytes, CAP)
        .unwrap()
        .into_iter()
        .collect()
}

#[test]
fn form_values_decode_utf8_plus_and_encoded_separators() {
    let got = form_values_from_bytes(b"name=R%C3%A9mi");
    assert_eq!(got.get("name").map(String::as_str), Some("Rémi"));

    let got = form_values_from_bytes(b"q=a+b&p=C%2B%2B");
    assert_eq!(got.get("q").map(String::as_str), Some("a b"));
    assert_eq!(got.get("p").map(String::as_str), Some("C++"));

    let got = form_values_from_bytes(b"a=1%262%3D3");
    assert_eq!(got.get("a").map(String::as_str), Some("1&2=3"));

    assert!(form_values_from_bytes(b"").is_empty());
}

/// A multiple choice posts its key once per value: the body keeps them all, in order.
#[tokio::test]
async fn a_repeated_key_keeps_every_value_in_body_order() {
    let pairs = form_pairs_from_request_parts(None, b"tags=&tags=b&name=Ada&tags=a", CAP).unwrap();
    let tags: Vec<&str> = pairs
        .iter()
        .filter(|(key, _)| key == "tags")
        .map(|(_, value)| value.as_str())
        .collect();
    assert_eq!(tags, ["", "b", "a"]);
    let body = "--X\r\nContent-Disposition: form-data; name=\"tags\"\r\n\r\nb\r\n\
                --X\r\nContent-Disposition: form-data; name=\"tags\"\r\n\r\na\r\n--X--\r\n";
    let parts = multipart_parts(
        &multipart_type("X"),
        body.as_bytes().to_vec(),
        Capture::None,
    )
    .await
    .unwrap();
    let tags: Vec<&str> = parts
        .pairs
        .iter()
        .filter(|(key, _)| key == "tags")
        .map(|(_, value)| value.as_str())
        .collect();
    assert_eq!(tags, ["b", "a"]);
}

/// Runs the streaming multipart parser over `body` with the given content type.
async fn multipart_parts(
    content_type: &str,
    body: Vec<u8>,
    capture: Capture,
) -> Result<FormParts, topcoat::Error> {
    let (parts, ()) = http::Request::builder()
        .uri("/admin/users/-/create")
        .header(http::header::CONTENT_TYPE, content_type)
        .body(())
        .unwrap()
        .into_parts();
    let cx = topcoat::context::Cx::default().with(parts);
    parse_multipart_values(&cx, Body::from(body), capture).await
}

/// The multipart `body`'s last value per key.
async fn multipart_values(
    content_type: &str,
    body: Vec<u8>,
) -> Result<HashMap<String, String>, topcoat::Error> {
    multipart_parts(content_type, body, Capture::None)
        .await
        .map(|parts| parts.pairs.into_iter().collect())
}

fn multipart_type(boundary: &str) -> String {
    format!("multipart/form-data; boundary={boundary}")
}

/// Byte accounting rejects one byte past the cap with 413.
#[test]
fn multipart_drain_counts_bytes_and_413s_one_past_the_cap() {
    let mut counted = Counter { seen: 0, cap: CAP };
    counted.count(CAP / 2).unwrap();
    counted.count(CAP / 2).unwrap();
    assert_eq!(counted.seen, CAP);
    let err = counted.count(1).unwrap_err();
    assert!(
        err.downcast_ref::<topcoat::router::error::ContentTooLargeError>()
            .is_some(),
        "one byte past the cap must map to content-too-large (413), got {err}"
    );
    // A single chunk past the cap fires without a prior accumulation.
    let mut counted = Counter { seen: 0, cap: CAP };
    let err = counted.count(CAP + 1).unwrap_err();
    assert!(
        err.downcast_ref::<topcoat::router::error::ContentTooLargeError>()
            .is_some(),
        "a single over-cap chunk must 413, got {err}"
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
    assert_eq!(got.get("image_path").map(String::as_str), Some("photo.jpg"));
    assert_eq!(got.get("tags").map(String::as_str), Some("second-wins"));

    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"image_path\"; filename=\"\"\r\nContent-Type: application/octet-stream\r\n\r\n\r\n--{b}--\r\n",
        b = boundary
    );
    let got = multipart_values(&multipart_type(boundary), body.into_bytes())
        .await
        .unwrap();
    assert_eq!(got.get("image_path").map(String::as_str), Some(""));
}

/// Stages file bytes only when capturing for an installed uploader.
#[tokio::test]
async fn multipart_stages_file_bytes_only_when_capturing() {
    let boundary = "----CaptureBoundary";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nHello\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"image_path\"; filename=\"photo.jpg\"\r\nContent-Type: image/jpeg\r\n\r\nBINARYBYTES\r\n\
             --{b}--\r\n",
        b = boundary
    );

    let captured = multipart_parts(
        &multipart_type(boundary),
        body.clone().into_bytes(),
        Capture::Persistable,
    )
    .await
    .unwrap();
    let staged = captured
        .files
        .get("image_path")
        .and_then(|staged| staged.last())
        .expect("staged file part");
    assert_eq!(staged.filename, "photo.jpg");
    assert_eq!(staged.bytes, b"BINARYBYTES");
    assert_eq!(captured.last("image_path"), Some("photo.jpg"));
    assert!(
        !captured.files.contains_key("title"),
        "a text part is not a file part, got {:?}",
        captured.files.keys()
    );

    // Not capturing: same values, no bytes held.
    let drained = multipart_parts(&multipart_type(boundary), body.into_bytes(), Capture::None)
        .await
        .unwrap();
    assert!(drained.files.is_empty(), "no uploader, no buffering");
    assert_eq!(drained.last("image_path"), Some("photo.jpg"));
}

/// Stages every file part posted under one name, in body order: a file list's input posts one per
/// chosen file.
#[tokio::test]
async fn multipart_stages_each_file_part_under_one_name() {
    let part = |filename: &str, bytes: &str| {
        format!(
            "--B\r\nContent-Disposition: form-data; name=\"photos\"; filename=\"{filename}\"\r\n\
             Content-Type: application/octet-stream\r\n\r\n{bytes}\r\n"
        )
    };
    let body = format!("{}{}--B--\r\n", part("a.png", "AAA"), part("b.png", "BBB"));
    let parts = multipart_parts(
        &multipart_type("B"),
        body.into_bytes(),
        Capture::Persistable,
    )
    .await
    .unwrap();
    let staged: Vec<(&str, &[u8])> = parts.files["photos"]
        .iter()
        .map(|file| (file.filename.as_str(), file.bytes.as_slice()))
        .collect();
    assert_eq!(
        staged,
        [("a.png", b"AAA".as_slice()), ("b.png", b"BBB".as_slice())]
    );
}

/// Stages nothing for a filename sanitized to empty.
#[tokio::test]
async fn multipart_stages_nothing_for_a_rejected_filename() {
    let body = "--B\r\nContent-Disposition: form-data; name=\"image_path\"; filename=\"..\"\r\nContent-Type: application/octet-stream\r\n\r\nBYTES\r\n--B--\r\n";
    let parts = multipart_parts(
        &multipart_type("B"),
        body.as_bytes().to_vec(),
        Capture::Persistable,
    )
    .await
    .unwrap();
    assert!(parts.files.is_empty(), "a rejected name stages no bytes");
    assert_eq!(parts.last("image_path"), Some(""));

    // An import's file is read, never stored, so its name does not decide whether it is staged.
    let parts = multipart_parts(&multipart_type("B"), body.as_bytes().to_vec(), Capture::Any)
        .await
        .unwrap();
    assert_eq!(
        parts
            .files
            .get("image_path")
            .and_then(|staged| staged.last())
            .map(|file| file.bytes.as_slice()),
        Some(b"BYTES".as_slice())
    );
}

#[tokio::test]
async fn multipart_stream_sanitizes_traversal_and_filename_star() {
    let body = "--B\r\nContent-Disposition: form-data; name=\"image_path\"; filename=\"../../../etc/passwd\"\r\nContent-Type: application/octet-stream\r\n\r\nBYTES\r\n--B--\r\n";
    let got = multipart_values(&multipart_type("B"), body.as_bytes().to_vec())
        .await
        .unwrap();
    assert_eq!(got.get("image_path").map(String::as_str), Some("passwd"));

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
    // A malformed `filename*` falls back to `filename=`.
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
    // A multipart body without a boundary rejects with 400.
    assert!(
        multipart_values("multipart/form-data", b"name=x".to_vec())
            .await
            .is_err()
    );
}

#[test]
fn an_urlencoded_body_past_the_cap_is_refused() {
    let big = vec![b'a'; CAP + 1];
    assert!(
        form_pairs_from_request_parts(Some("application/x-www-form-urlencoded"), &big, CAP)
            .is_err()
    );
    let ok =
        form_pairs_from_request_parts(Some("application/x-www-form-urlencoded"), b"name=Ada", CAP)
            .unwrap();
    assert_eq!(ok, [("name".to_string(), "Ada".to_string())]);
}
