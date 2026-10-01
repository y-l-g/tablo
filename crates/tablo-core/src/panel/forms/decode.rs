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
/// buffered, plus `multipart/form-data` streamed when a file field is present.
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
                // A chosen file is the one thing that may set a file field's value
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
/// enforces the `BodyLimit::max(MAX_FORM_BYTES)` layer a mounted panel
/// installs under its prefix, so this branch is
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
mod tests;
