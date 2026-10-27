//! Reading a cookie off the request.

use http::header::COOKIE;
use topcoat::context::{Cx, try_context};

/// The value of the request's `name` cookie.
///
/// Parsed from the raw `Cookie` header on purpose: `topcoat::cookie::cookies`
/// panics when the cookie router layer is absent (tests, minimal routers), and
/// the shell must render everywhere.
pub(crate) fn request_cookie(cx: &Cx, name: &str) -> Option<String> {
    let header = try_context::<http::request::Parts>(cx)?
        .headers
        .get(COOKIE)?
        .to_str()
        .ok()?;
    header.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        (key == name).then(|| value.to_string())
    })
}
