//! CSRF protection via double-submit cookie.
//!
//! Verifies state-changing POSTs with a double-submit `__Host-` cookie compared in constant time.
//! A form carries the cookie's token masked with a one-time pad drawn for each render, so no two
//! compressed pages hold the same bytes for an observer to guess from response sizes (BREACH).

use data_encoding::BASE64URL_NOPAD;
use subtle::ConstantTimeEq;
use topcoat::{
    context::{Cx, try_context},
    cookie::{Cookie, CookieJarCell, Cookies, cookies},
};

/// Names the `__Host-`-prefixed CSRF cookie.
pub const COOKIE_NAME: &str = "__Host-tablo_csrf";
pub const FIELD_NAME: &str = "csrf_token";

/// The bytes in a token.
const TOKEN_LEN: usize = 16;

/// Ensures a request token before headers send, returning `""` without a cookie layer.
pub fn ensure_token(cx: &Cx) -> String {
    if try_context::<CookieJarCell>(cx).is_none() {
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
    if try_context::<CookieJarCell>(cx).is_none() {
        return String::new();
    }
    cookies(cx)
        .get(COOKIE_NAME)
        .map(|c| c.value().to_string())
        .filter(|v| is_valid_token(v))
        .unwrap_or_default()
}

/// Renders the hidden field carrying the given token, [`mask`]ed afresh.
pub fn field<'a>(cx: &'a Cx, token: &str) -> topcoat::view::BoxView<'a> {
    use topcoat::view::ViewExt;

    let value = mask(token);
    topcoat::view::view! { <input type="hidden" name=(FIELD_NAME) value=(value)> }.boxed()
}

/// The value a form posts under [`FIELD_NAME`] for `token`: the token XORed with a random pad,
/// pad first, in unpadded base64url. Each call draws a new pad, so the same token never renders
/// twice the same; [`verify`] accepts any of them. `""` when `token` is not a token
/// [`ensure_token`] issued.
///
/// [`field`] renders it; call it for a form a page builds without the view.
pub fn mask(token: &str) -> String {
    let Ok(token) = token.parse::<uuid::Uuid>() else {
        return String::new();
    };
    let mut masked = [0u8; 2 * TOKEN_LEN];
    let (pad, sealed) = masked.split_at_mut(TOKEN_LEN);
    getrandom::fill(pad).expect("the CSRF pad randomness source failed");
    for ((sealed, pad), byte) in sealed.iter_mut().zip(&*pad).zip(token.as_bytes()) {
        *sealed = pad ^ byte;
    }
    BASE64URL_NOPAD.encode(&masked)
}

/// The token a [`mask`]ed value carries, or `None` when `value` is not one.
fn unmask(value: &str) -> Option<[u8; TOKEN_LEN]> {
    let masked = BASE64URL_NOPAD.decode(value.as_bytes()).ok()?;
    let (pad, sealed) = masked.split_at_checked(TOKEN_LEN)?;
    if sealed.len() != TOKEN_LEN {
        return None;
    }
    let mut token = [0u8; TOKEN_LEN];
    for ((byte, pad), sealed) in token.iter_mut().zip(pad).zip(sealed) {
        *byte = pad ^ sealed;
    }
    Some(token)
}

/// Verifies the value a POST `submitted` under [`FIELD_NAME`] is a [`mask`] of the cookie's
/// token with a constant-time compare, failing closed with 403. The cookie's token itself,
/// unmasked, is refused: no page renders it.
pub fn verify(cx: &Cx, submitted: Option<&str>) -> Result<(), topcoat::Error> {
    let expected = try_context::<CookieJarCell>(cx)
        .and_then(|_| cookies(cx).get(COOKIE_NAME).map(|c| c.value().to_string()))
        .and_then(|value| value.parse::<uuid::Uuid>().ok());
    let submitted = submitted.and_then(unmask);
    let (Some(expected), Some(submitted)) = (expected, submitted) else {
        return Err(topcoat::router::error::forbidden().into());
    };
    if submitted.as_slice().ct_ne(expected.as_bytes()).into() {
        return Err(topcoat::router::error::forbidden().into());
    }
    Ok(())
}

fn is_valid_token(value: &str) -> bool {
    value.parse::<uuid::Uuid>().is_ok()
}

#[cfg(test)]
mod tests;
