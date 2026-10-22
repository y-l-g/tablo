//! Urlencoded and streamed-multipart body decoding: the text a form posts, and the bytes of the
//! files it chooses, staged as the caller asks.

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

use super::StagedUpload;
use crate::schema::MAX_FORM_BYTES;

/// A decoded form body: its text pairs plus any file parts.
pub(crate) struct FormParts {
    /// Every text value, and each file part's sanitized filename, in the order the body posts
    /// them.
    pub(crate) pairs: Vec<(String, String)>,
    /// The bytes of each file part the parse staged, by name.
    pub(crate) files: HashMap<String, StagedUpload>,
    /// Names arriving as multipart parts carrying a `filename`; only these may set a file field's
    /// value.
    pub(crate) file_part_names: HashSet<String>,
}

impl FormParts {
    /// The last value the body posts under `key`.
    pub(crate) fn last(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .rev()
            .find(|(posted, _)| posted == key)
            .map(|(_, value)| value.as_str())
    }
}

/// Parses an urlencoded or multipart body into `FormParts`, streaming file parts and staging
/// those `capture` asks for.
pub(crate) async fn parse_body(
    cx: &Cx,
    body: Body,
    capture: Capture,
) -> Result<FormParts, topcoat::Error> {
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
        return parse_multipart_values(cx, body, capture).await;
    }
    let bytes = Bytes::from_request(cx, body).await.map_err(|error| {
        // Maps over-limit reads to 413 and other read failures to 400.
        if error.is::<topcoat::router::error::ContentTooLargeError>() {
            error
        } else {
            topcoat::router::error::bad_request("cannot read form body").into()
        }
    })?;
    Ok(FormParts {
        pairs: form_pairs_from_request_parts(content_type.as_deref(), bytes.as_ref())?,
        files: HashMap::new(),
        file_part_names: HashSet::new(),
    })
}

/// Parses a multipart body, staging every chosen file's bytes whether or not the panel installs
/// an uploader, and whatever its name: the body an import posts its file in, which nothing
/// stores. Any other body answers 400.
pub(crate) async fn parse_file_body(cx: &Cx, body: Body) -> Result<FormParts, topcoat::Error> {
    let multipart = topcoat::context::try_request_context::<http::request::Parts>(cx)
        .and_then(|parts| parts.headers.get(http::header::CONTENT_TYPE))
        .and_then(|value| value.to_str().ok())
        .is_some_and(is_multipart_content_type);
    if !multipart {
        return Err(topcoat::router::error::bad_request("expected a multipart body").into());
    }
    parse_multipart_values(cx, body, Capture::Any).await
}

/// Which chosen files a multipart body stages the bytes of.
#[derive(Clone, Copy)]
pub(crate) enum Capture {
    /// None: nothing stores the request's files, such as an auth form's or a panel's without an
    /// uploader.
    None,
    /// Each whose sanitized name an uploader can store it under.
    Persistable,
    /// Every one, which nothing stores.
    Any,
}

fn is_multipart_content_type(ct: &str) -> bool {
    ct.split(';')
        .next()
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("multipart/form-data"))
}

/// Streams multipart fields in body order, a file part posting its sanitized filename, counting
/// every byte against `MAX_FORM_BYTES` and mapping over-limit bodies to 413.
async fn parse_multipart_values(
    cx: &Cx,
    body: Body,
    capture: Capture,
) -> Result<FormParts, topcoat::Error> {
    use topcoat::router::{content::multipart::Multipart, request::FromRequest};

    let mut out = FormParts {
        pairs: Vec::new(),
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
                let sanitized = super::sanitize_filename(&f);
                // Last part wins, replacing any earlier staged bytes.
                out.files.remove(&name);
                // Stages bytes as `capture` asks, else drains.
                let staged = match capture {
                    Capture::None => false,
                    Capture::Persistable => !sanitized.is_empty(),
                    Capture::Any => true,
                };
                if staged {
                    let mut bytes = Vec::new();
                    read_bounded(&mut field, &mut bytes_seen, Some(&mut bytes)).await?;
                    out.files.insert(
                        name.clone(),
                        StagedUpload {
                            filename: sanitized.clone(),
                            bytes,
                        },
                    );
                } else {
                    read_bounded(&mut field, &mut bytes_seen, None).await?;
                }
                // Only a chosen file part may set a file field's value.
                out.file_part_names.insert(name.clone());
                out.pairs.push((name, sanitized));
            }
            Some(_) => {
                // An empty filename still counts as a file part with an empty value.
                read_bounded(&mut field, &mut bytes_seen, None).await?;
                out.files.remove(&name);
                out.file_part_names.insert(name.clone());
                out.pairs.push((name, String::new()));
            }
            None => {
                // Counts text reads against the same byte cap.
                let text = field.text().await?;
                count_form_bytes(&mut bytes_seen, text.len())?;
                // A later text part clears the name from the file-part set.
                out.file_part_names.remove(&name);
                out.files.remove(&name);
                out.pairs.push((name, text));
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
