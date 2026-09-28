//! Urlencoded and streamed-multipart body decoding.

use std::collections::{HashMap, HashSet};

use percent_encoding::percent_decode_str;
use topcoat::{
    Result,
    context::Cx,
    router::{
        Body,
        request::{Bytes, FromRequest},
    },
};

use super::common::{FormParts, MAX_FORM_BYTES};

/// Helper: parse form bodies into a [`FormParts`] — `application/x-www-form-urlencoded`
/// buffered, plus `multipart/form-data` streamed when a `FileUpload` is present.
/// urlencoded decoding is delegated to `form_urlencoded` (already in the tree
/// via topcoat): it splits pairs, decodes `+` as space, assembles multi-byte
/// UTF-8 from `%XX` sequences (`%C3%A9` → `é`, not `Ã©`), and keeps encoded
/// separators (`%26` → `&`) intact. Invalid UTF-8 degrades per-value (lossy)
/// instead of discarding the whole form.
///
/// Multipart (file) parts stream through Topcoat's multer-based extractor.
/// With no installed uploader the bytes are drained in chunks and
/// discarded while the sanitized filename becomes the `String` value; with one
/// installed they are buffered up to the same body cap and handed to it.
/// Text parts store their content, and unknown content types fall
/// back to urlencoded.
pub(crate) async fn parse_form_body(cx: &Cx, body: Body) -> Result<FormParts, topcoat::Error> {
    let content_type =
        topcoat::context::try_request_context::<http::request::Parts>(cx).and_then(|parts| {
            parts
                .headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
        });
    if content_type
        .as_deref()
        .is_some_and(is_multipart_content_type)
    {
        // Stage bytes only when something will consume them: the
        // parser is the one place that knows whether an uploader exists.
        return parse_multipart_values(cx, body, crate::upload::installed(cx)).await;
    }
    let bytes = Bytes::from_request(cx, body).await.map_err(|error| {
        // A body over the route's limit is the extractor's 413, not a malformed
        // form; any other read failure is the 400. The multipart half
        // propagates the same over-limit error untouched.
        if error.is::<topcoat::router::error::ContentTooLargeError>() {
            error
        } else {
            topcoat::router::error::bad_request("cannot read form body").into()
        }
    })?;
    Ok(FormParts {
        values: form_values_from_request_parts(content_type.as_deref(), bytes.as_ref())?,
        files: HashMap::new(),
        file_part_names: HashSet::new(),
    })
}

/// Whether a content type is `multipart/form-data` (parameters ignored).
fn is_multipart_content_type(ct: &str) -> bool {
    ct.split(';')
        .next()
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("multipart/form-data"))
}

/// Streamed multipart half of [`parse_form_body`].
///
/// Fields stream one at a time with constant memory: text fields buffer
/// (bounded by the request body limit), file fields drain-and-discard while
/// only the sanitized filename is kept — or, when `capture` is set because an
/// uploader is installed, buffer up to the same cap so it can store them.
/// Duplicate part names are last-wins; nameless parts are skipped. A
/// missing boundary is a 400, an over-limit body a 413 — both classified by the
/// extractor, never silent fallbacks. Every byte the stream carries is also
/// counted against [`MAX_FORM_BYTES`]: file reads and skipped parts
/// go through the counter chunk by chunk, text fields join it after their
/// (extractor-bounded) read, so a large upload cannot be read chunk-by-chunk
/// holding the handler even if the extractor's limit stops wrapping the stream.
async fn parse_multipart_values(
    cx: &Cx,
    body: Body,
    capture: bool,
) -> Result<FormParts, topcoat::Error> {
    use topcoat::router::{content::multipart::Multipart, request::FromRequest};

    let mut out = FormParts {
        values: HashMap::new(),
        files: HashMap::new(),
        file_part_names: HashSet::new(),
    };
    let mut bytes_seen = 0usize;
    let mut multipart = Multipart::from_request(cx, body).await?;
    while let Some(mut field) = multipart.next_field().await? {
        let Some(name) = field.name().map(str::to_string) else {
            // Nameless parts carry bytes too: drain them through the counter
            // so the accounting covers the whole request stream.
            read_bounded(&mut field, &mut bytes_seen, None).await?;
            continue;
        };
        if name.is_empty() {
            read_bounded(&mut field, &mut bytes_seen, None).await?;
            continue;
        }
        // RFC 6266: `filename*=` (decoded) takes precedence over `filename=`.
        // Multer surfaces the plain `filename=` first, so the raw header is
        // read for `filename*=` before falling back.
        let filename =
            filename_star_from_headers(&field).or_else(|| field.file_name().map(str::to_string));
        match filename {
            Some(f) if !f.is_empty() => {
                let sanitized = sanitize_filename(&f);
                // Duplicate part names are last-write-wins, bytes included: a
                // later part replaces whatever an earlier one under the same
                // name staged, so a name that sanitizes to empty cannot leave
                // the earlier part's bytes behind.
                out.files.remove(&name);
                // Bytes are staged only for a name the framework would persist
                // (a rejected name sanitizes to empty) and only when
                // an uploader is installed to store them. Otherwise
                // drain to advance the stream.
                if capture && !sanitized.is_empty() {
                    let mut bytes = Vec::new();
                    read_bounded(&mut field, &mut bytes_seen, Some(&mut bytes)).await?;
                    out.files.insert(
                        name.clone(),
                        crate::upload::StagedUpload {
                            filename: sanitized.clone(),
                            bytes,
                        },
                    );
                } else {
                    read_bounded(&mut field, &mut bytes_seen, None).await?;
                }
                // A chosen file is the one thing that may set a `FileUpload`
                // value.
                out.file_part_names.insert(name.clone());
                out.values.insert(name, sanitized);
            }
            Some(_) => {
                // Empty filename (no file chosen) → empty value so `required`
                // validation fires instead of treating it as missing. It is
                // still a file part: the browser submits every file
                // input, and "keep" on edit must not read as a forged text
                // value. It chose no file, so it discards bytes an earlier part
                // staged under the same name.
                read_bounded(&mut field, &mut bytes_seen, None).await?;
                out.files.remove(&name);
                out.file_part_names.insert(name.clone());
                out.values.insert(name, String::new());
            }
            None => {
                // Text fields buffer (extractor-bounded); the read joins the
                // same counter so the backstop sees the per-request total.
                let text = field.text().await?;
                count_form_bytes(&mut bytes_seen, text.len())?;
                // A later text part under a name an earlier file part used
                // takes the name out of the file-part set, so the drop removes
                // the client-typed value instead of storing it.
                out.file_part_names.remove(&name);
                out.files.remove(&name);
                out.values.insert(name, text);
            }
        }
    }
    Ok(out)
}

/// Read one multipart field chunk-by-chunk, accounting every byte against
/// [`MAX_FORM_BYTES`]: enforcement normally happens in the extractor
/// (`BodyLimit` wraps the multipart stream), but the reader owns its own
/// counter so an over-cap upload 413s here too instead of holding the handler.
///
/// `keep` decides the destination, not the accounting: `None` drains and
/// discards (the default path, constant memory), `Some(sink)` buffers for an
/// installed [`Uploader`](crate::Uploader). One loop, so the two
/// paths cannot disagree about the cap — and the buffered bytes are bounded by
/// that same cap, so installing an uploader trades the discard for at most one
/// body's worth of heap rather than widening the contract.
async fn read_bounded(
    field: &mut topcoat::router::content::multipart::Field<'_>,
    bytes_seen: &mut usize,
    keep: Option<&mut Vec<u8>>,
) -> Result<(), topcoat::Error> {
    let mut keep = keep;
    while let Some(chunk) = field.chunk().await? {
        count_form_bytes(bytes_seen, chunk.len())?;
        if let Some(sink) = keep.as_deref_mut() {
            sink.extend_from_slice(&chunk);
        }
    }
    Ok(())
}

/// Account one drained chunk against the form-body cap. Extracted
/// so the 413 mapping is testable at the boundary without building a
/// multipart body — through the router the extractor's own limit classifies
/// the same body first, so the counter only answers when that limit stops
/// applying (defense-in-depth alongside `BodyLimit`, never a competing cap).
fn count_form_bytes(bytes_seen: &mut usize, chunk_len: usize) -> Result<(), topcoat::Error> {
    // checked_add: the accumulator cannot overflow a usize at real chunk
    // sizes, but wrapping would silently disable the cap in release builds.
    let Some(total) = bytes_seen.checked_add(chunk_len) else {
        return Err(topcoat::router::error::content_too_large().into());
    };
    *bytes_seen = total;
    if *bytes_seen > MAX_FORM_BYTES {
        return Err(topcoat::router::error::content_too_large().into());
    }
    Ok(())
}

/// RFC 5987 `filename*=` from a field's raw `Content-Disposition` header.
fn filename_star_from_headers(
    field: &topcoat::router::content::multipart::Field<'_>,
) -> Option<String> {
    let raw = field
        .headers()
        .get(http::header::CONTENT_DISPOSITION)?
        .to_str()
        .ok()?;
    raw.split(';').find_map(|seg| {
        let seg = seg.trim();
        seg.get(..10)
            .filter(|h| h.eq_ignore_ascii_case("filename*="))
            .and_then(|_| decode_rfc5987(seg[10..].trim()))
    })
}

/// Pure urlencoded half of [`parse_form_body`] — testable without
/// a request. Rejects bodies over `MAX_FORM_BYTES` with 413. Multipart
/// never reaches here: it streams via [`parse_multipart_values`], where a
/// missing boundary is a 400 and an over-limit body a 413 (both classified
/// by the extractor).
///
/// The length check is a deliberate second layer: through the router
/// `Bytes::from_request` buffers via `to_bytes(body, body_limit(cx))`, which
/// enforces the `BodyLimit::max(MAX_FORM_BYTES)` layer
/// [`Panel::build`](crate::panel::Panel::build) installs, so this branch is
/// unreachable there. It is the urlencoded symmetric backstop to the multipart
/// [`count_form_bytes`] counter, and the only pin of the 10 MiB
/// urlencoded contract at unit level — a bare `CxTestBuilder` carries no
/// `BodyLimitKind`, so `body_limit(cx)` falls back to Topcoat's 2 MiB default
/// and cannot pin this cap. Do not collapse into [`form_values_from_bytes`]
/// without restoring both properties.
fn form_values_from_request_parts(
    content_type: Option<&str>,
    bytes: &[u8],
) -> Result<HashMap<String, String>, topcoat::Error> {
    if bytes.len() > MAX_FORM_BYTES {
        return Err(topcoat::router::error::content_too_large().into());
    }
    debug_assert!(
        content_type.is_none_or(|ct| !is_multipart_content_type(ct)),
        "multipart must stream via parse_multipart_values, not buffer here (GH #90)"
    );
    Ok(form_values_from_bytes(bytes))
}

/// Sanitize a client-supplied filename to a basename.
///
/// Strips directory components (`../../etc/passwd` → `passwd`,
/// `/abs/path` → `path`, `C:\fakepath\x` → `x`), trims whitespace, drops
/// control chars, and caps length at 255 bytes. Empty stays empty so
/// `required` validation fires.
///
/// Names that could never be a safely persisted file are rejected to empty
/// (before persistence lands): `.` and `..`, and Windows reserved
/// device names (`con`, `nul`, `com1` — also with an extension, and
/// case-insensitive). v1 stores only the basename `String` and never touches
/// the filesystem, so today this is latent; the required validation then
/// surfaces the empty value as an inline form error (on edit, the
/// untouched-file backfill preserves the stored value instead — an
/// explicitly rejected name falls back to "keep").
fn sanitize_filename(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw).trim();
    let clean: String = base.chars().filter(|c| !c.is_control()).collect();
    let trimmed = clean.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed == "." || trimmed == ".." || is_windows_reserved_name(trimmed) {
        return String::new();
    }
    // Cap at 255 bytes (common filename limit), preserving the tail. The cut
    // point is walked forward to a char boundary: slicing a multibyte char
    // would panic (a >255-byte non-ASCII filename is attacker-controlled).
    if trimmed.len() > 255 {
        let mut start = trimmed.len() - 255;
        while !trimmed.is_char_boundary(start) {
            start += 1;
        }
        trimmed[start..].to_string()
    } else {
        trimmed.to_string()
    }
}

/// Windows reserved device names: the stem before the first dot is
/// reserved case-insensitively — `con`, `nul`, `aux`, `prn`, `com1`–`com9`,
/// `lpt1`–`lpt9` — so `con.txt` cannot become a persisted basename either.
fn is_windows_reserved_name(name: &str) -> bool {
    let stem = match name.split_once('.') {
        Some((stem, _)) => stem,
        None => name,
    };
    let stem = stem.to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    let Some(n) = stem
        .strip_prefix("COM")
        .or_else(|| stem.strip_prefix("LPT"))
    else {
        return false;
    };
    n.parse::<u8>().is_ok_and(|n| (1..=9).contains(&n))
}

/// Decode an RFC 5987/6266 `filename*=UTF-8''...` value.
///
/// Three rules fail the whole value to `None`, so the caller falls back to
/// `filename=` instead of storing a mangled name: the charset is not UTF-8, a
/// `%` does not start a `pct-encoded` triplet (two hex digits), or the decoded
/// bytes are not valid UTF-8.
fn decode_rfc5987(value: &str) -> Option<String> {
    let (charset, rest) = value.split_once('\'')?;
    let (_lang, encoded) = rest.split_once('\'')?;
    if !charset.eq_ignore_ascii_case("utf-8") {
        return None;
    }
    if !is_pct_encoded(encoded) {
        return None;
    }
    percent_decode_str(encoded)
        .decode_utf8()
        .ok()
        .map(|decoded| decoded.into_owned())
}

/// Whether every `%` in `value` starts an RFC 5987 `pct-encoded` triplet (`%`
/// followed by two hex digits).
fn is_pct_encoded(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if !bytes
                .get(i + 1..i + 3)
                .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit))
            {
                return false;
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    true
}

/// Pure half of [`parse_form_body`] — testable without a request.
fn form_values_from_bytes(bytes: &[u8]) -> HashMap<String, String> {
    form_urlencoded::parse(bytes).into_owned().collect()
}
#[cfg(test)]
mod tests {
    use toasty::Db;

    use super::{
        super::common::{FormParts, MAX_FORM_BYTES},
        *,
    };
    use crate::panel::test_support::{Dummy, dummy_table, form_panel_for};

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
        impl crate::form::FormResource for DummyResource {
            type Form = DummyForm;
            fn form(_cx: &Cx) -> crate::schema::Schema {
                crate::schema::Schema::new(crate::schema::FileUpload::r#for(Dummy::fields().name()))
            }
        }

        let db = Db::builder().connect("sqlite::memory:").await.unwrap();
        let router = form_panel_for::<DummyResource>(db)
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
            form_values_from_request_parts(Some("application/x-www-form-urlencoded"), &big)
                .is_err()
        );
        // Normal urlencoded still parses.
        let ok =
            form_values_from_request_parts(Some("application/x-www-form-urlencoded"), b"name=Ada")
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
}
