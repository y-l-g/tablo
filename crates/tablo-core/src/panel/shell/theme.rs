//! The visitor's theme and sidebar state, as the request's cookies store them.

use http::header::COOKIE;
use topcoat::context::{Cx, try_context};

use super::current;

/// The cookie holding the visitor's theme, `light`, `dark` or `system`, which `assets/theme.js`
/// writes.
///
/// Prefixed because a cookie is shared by every app on a host whatever its port: another app's
/// `theme` on `localhost`, possibly HttpOnly and so beyond the toggle's reach, would otherwise
/// decide this panel's theme.
const THEME_COOKIE: &str = "tablo_theme";

/// The cookie holding the system's scheme, `light` or `dark`, which `assets/theme.js` mirrors
/// while the visitor's theme is `system`: the server cannot see the system's preference.
const SCHEME_COOKIE: &str = "tablo_scheme";

/// A visitor's theme choice, as [`THEME_COOKIE`] stores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Theme {
    Light,
    Dark,
    /// The system's scheme, which [`SCHEME_COOKIE`] mirrors.
    System,
}

impl Theme {
    /// The choice the request's cookie stores, if any.
    fn stored(cx: &Cx) -> Option<Self> {
        match request_cookie(cx, THEME_COOKIE).as_deref() {
            Some("light") => Some(Self::Light),
            Some("dark") => Some(Self::Dark),
            Some("system") => Some(Self::System),
            _ => None,
        }
    }

    /// The choice the user menu shows as checked: the stored one, else the panel's default.
    pub(super) fn current(cx: &Cx) -> Self {
        Self::stored(cx).unwrap_or(if Self::panel_default_dark(cx) {
            Self::Dark
        } else {
            Self::Light
        })
    }

    /// Whether the panel shows dark to a visitor who has not chosen.
    fn panel_default_dark(cx: &Cx) -> bool {
        current(cx).is_some_and(|panel| panel.dark_mode)
    }

    /// Whether the request's page renders dark: the stored choice, the mirrored system scheme
    /// under `system`, else `default_dark`, the panel's default.
    pub(super) fn renders_dark(cx: &Cx, default_dark: bool) -> bool {
        match Self::stored(cx) {
            Some(Self::Dark) => true,
            Some(Self::Light) => false,
            Some(Self::System) => match request_cookie(cx, SCHEME_COOKIE).as_deref() {
                Some("dark") => true,
                Some("light") => false,
                _ => default_dark,
            },
            None => default_dark,
        }
    }

    /// The name the cookie and the theme script use.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }
}

/// The cookie holding the desktop sidebar's state, which `assets/sidebar.js` writes.
pub(super) const SIDEBAR_COOKIE: &str = "tablo_sidebar";

/// The value of the request's `name` cookie.
///
/// Parsed from the raw `Cookie` header on purpose: `topcoat::cookie::cookies`
/// panics when the cookie router layer is absent (tests, minimal routers), and
/// the shell must render everywhere.
pub(super) fn request_cookie(cx: &Cx, name: &str) -> Option<String> {
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
