//! CSRF protection via double-submit cookie.
//!
//! Verifies state-changing POSTs with a double-submit `__Host-` cookie compared in constant time.

use subtle::ConstantTimeEq;
use topcoat::{
    context::{Cx, try_request_context},
    cookie::{Cookie, CookieJarCell, Cookies, cookies},
};

/// Names the `__Host-`-prefixed CSRF cookie.
pub const COOKIE_NAME: &str = "__Host-tablo_csrf";
pub const FIELD_NAME: &str = "csrf_token";

/// Ensures a request token before headers send, returning `""` without a cookie layer.
pub fn ensure_token(cx: &Cx) -> String {
    if try_request_context::<CookieJarCell>(cx).is_none() {
        return String::new();
    }
    let jar = cookies(cx);
    if let Some(cookie) = jar.get(COOKIE_NAME) {
        let value = cookie.value().to_string();
        if is_valid_token(&value) {
            return value;
        }
    }
    let token = uuid::Uuid::new_v4().to_string();
    let cookie = Cookie::build((COOKIE_NAME, token.clone()))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(topcoat::cookie::SameSite::Lax)
        .build();
    jar.add(cookie);
    token
}

/// Reads the current token without setting one, safe inside streamed children.
pub fn current_token(cx: &Cx) -> String {
    if try_request_context::<CookieJarCell>(cx).is_none() {
        return String::new();
    }
    cookies(cx)
        .get(COOKIE_NAME)
        .map(|c| c.value().to_string())
        .filter(|v| is_valid_token(v))
        .unwrap_or_default()
}

/// Renders the hidden field embedding the given token.
pub fn field<'a>(cx: &'a Cx, token: &str) -> topcoat::view::BoxView<'a> {
    use topcoat::view::ViewExt;

    let token = token.to_string();
    topcoat::view::view! { cx => <input type="hidden" name=(FIELD_NAME) value=(token)> }.boxed()
}

/// Verifies the submitted token matches the cookie with a constant-time compare, failing closed
/// with 403.
pub fn verify(
    cx: &Cx,
    values: &std::collections::HashMap<String, String>,
) -> Result<(), topcoat::Error> {
    let cookie_ok = try_request_context::<CookieJarCell>(cx)
        .map(|_| cookies(cx).get(COOKIE_NAME).map(|c| c.value().to_string()))
        .unwrap_or(None);
    let Some(expected) = cookie_ok else {
        return Err(topcoat::router::error::forbidden().into());
    };
    let Some(submitted) = values.get(FIELD_NAME) else {
        return Err(topcoat::router::error::forbidden().into());
    };
    if !is_valid_token(&expected) || submitted.as_bytes().ct_ne(expected.as_bytes()).into() {
        return Err(topcoat::router::error::forbidden().into());
    }
    Ok(())
}

fn is_valid_token(value: &str) -> bool {
    value.parse::<uuid::Uuid>().is_ok()
}

#[cfg(test)]
mod tests;
