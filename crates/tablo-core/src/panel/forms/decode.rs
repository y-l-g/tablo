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

/// Parses urlencoded and multipart bodies into `FormParts`, streaming file parts.
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
        return parse_multipart_values(cx, body, crate::upload::installed(cx)).await;
    }
    let bytes = Bytes::from_request(cx, body).await.map_err(|error| {
        // Maps over-limit reads to 413 and other read failures to 400.
        if error.is::<topcoat::router::error::ContentTooLargeError>() {
            error
        } else {
            topcoat::router::error::bad_request("cannot read form body").into()
        }
    })?;
    let pairs = form_pairs_from_request_parts(content_type.as_deref(), bytes.as_ref())?;
    let mut lists: HashMap<String, Vec<String>> = HashMap::new();
    for (name, value) in &pairs {
        lists.entry(name.clone()).or_default().push(value.clone());
    }
    Ok(FormParts {
        values: pairs.into_iter().collect(),
        lists,
        files: HashMap::new(),
        file_part_names: HashSet::new(),
    })
}

fn is_multipart_content_type(ct: &str) -> bool {
    ct.split(';')
        .next()
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("multipart/form-data"))
}

/// Streams multipart fields with last-wins duplicates, counting every byte against `MAX_FORM_BYTES`
/// and mapping over-limit bodies to 413.
async fn parse_multipart_values(
    cx: &Cx,
    body: Body,
    capture: bool,
) -> Result<FormParts, topcoat::Error> {
    use topcoat::router::{content::multipart::Multipart, request::FromRequest};

    let mut out = FormParts {
        values: HashMap::new(),
        lists: HashMap::new(),
        files: HashMap::new(),
        file_part_names: HashSet::new(),
    };
    let mut bytes_seen = 0usize;
    let mut multipart = Multipart::from_request(cx, body).await?;
    while let Some(mut field) = multipart.next_field().await? {
        let Some(name) = field.name().map(str::to_string) else {
            // Drains nameless parts through the byte counter.
            read_bounded(&mut field, &mut bytes_seen, None).await?;
            continue;
        };
        if name.is_empty() {
            read_bounded(&mut field, &mut bytes_seen, None).await?;
            continue;
        }
        // RFC 6266: `filename*=` takes precedence over `filename=`.
        let filename =
            filename_star_from_headers(&field).or_else(|| field.file_name().map(str::to_string));
        match filename {
            Some(f) if !f.is_empty() => {
                let sanitized = crate::upload::sanitize_filename(&f);
                // Last part wins, replacing any earlier staged bytes.
                out.files.remove(&name);
                // Stages bytes only for persistable names when capturing, else drains.
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
                // Only a chosen file part may set a file field's value.
                out.file_part_names.insert(name.clone());
                out.values.insert(name, sanitized);
            }
            Some(_) => {
                // An empty filename still counts as a file part with an empty value.
                read_bounded(&mut field, &mut bytes_seen, None).await?;
                out.files.remove(&name);
                out.file_part_names.insert(name.clone());
                out.values.insert(name, String::new());
            }
            None => {
                // Counts text reads against the same byte cap.
                let text = field.text().await?;
                count_form_bytes(&mut bytes_seen, text.len())?;
                // A later text part clears the name from the file-part set.
                out.file_part_names.remove(&name);
                out.files.remove(&name);
                out.lists
                    .entry(name.clone())
                    .or_default()
                    .push(text.clone());
                out.values.insert(name, text);
            }
        }
    }
    Ok(out)
}

/// Reads one field chunk-by-chunk, counting every byte against `MAX_FORM_BYTES`.
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

/// Counts one chunk against the form-body cap, mapping over-limit totals to 413.
fn count_form_bytes(bytes_seen: &mut usize, chunk_len: usize) -> Result<(), topcoat::Error> {
    // Uses checked addition so the accumulator cannot wrap past the cap.
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

/// Parses urlencoded bytes into their pairs in body order, rejecting bodies over
/// `MAX_FORM_BYTES` with 413.
fn form_pairs_from_request_parts(
    content_type: Option<&str>,
    bytes: &[u8],
) -> Result<Vec<(String, String)>, topcoat::Error> {
    if bytes.len() > MAX_FORM_BYTES {
        return Err(topcoat::router::error::content_too_large().into());
    }
    debug_assert!(
        content_type.is_none_or(|ct| !is_multipart_content_type(ct)),
        "multipart must stream via parse_multipart_values, not buffer here"
    );
    Ok(form_urlencoded::parse(bytes).into_owned().collect())
}

/// Decodes an RFC 5987/6266 `filename*=UTF-8''...` value.
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

#[cfg(test)]
mod tests;
