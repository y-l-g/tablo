use std::fmt::Write as _;

use topcoat::{
    Result,
    view::{View, component, view},
};

/// Pre-paint theme application — the blocking head script that reconciles the
/// `dark` class on `<html>` before the first pixel (shadcn `theme-provider`
/// parity: `attribute="class"` + pre-paint apply).
///
/// `theme.js` wires the toggle buttons and persists the choice to
/// `localStorage` + a `theme` cookie, but it runs on `DOMContentLoaded`, i.e.
/// after first paint. Render this in the `<head>` — before any body content —
/// so the class is right before the first pixel.
///
/// The stored preference is **authoritative in both directions**: a
/// stored `light` *removes* the server-rendered `dark` class rather than
/// leaving it in place.
///
/// The script deliberately avoids `&&`: topcoat has no DOM-safe inline-script
/// primitive, so the body goes through the normal HTML escape path, which
/// rewrites `&&` to `&amp;&amp;` — and character references are **not** decoded
/// inside `<script>` (HTML's script-data state), so the emitted script would
/// be a syntax error. Nested `if`s keep the body free of escapable characters.
fn theme_init_script_body(default_dark: bool) -> String {
    let mut script = String::from(
        "(function(){var t=null;try{t=localStorage.getItem('theme')}catch(e){}\
if(!t){var m=document.cookie.match(/theme=([^;]+)/);if(m)t=m[1]}\
if(t!=='light'){if(t!=='dark')t=",
    );
    let _ = write!(script, "'{}'", if default_dark { "dark" } else { "light" });
    script.push_str(
        ";}if(t==='dark')document.documentElement.classList.add('dark');\
else document.documentElement.classList.remove('dark')})();",
    );
    script
}

/// Render in the `<head>` of every layout, before the stylesheet link.
///
/// `default_dark` is the server's build-time preference (`Panel::dark_mode`),
/// used only when the visitor has no stored choice.
///
/// The body goes through the normal escaping path rather than `Unescaped`:
/// topcoat has no DOM-safe script primitive, and the body is ours — a fixed
/// ASCII program with one `true`/`false` interpolated — so escaping is
/// harmless as long as the program stays free of characters HTML would
/// rewrite (see `theme_init_script_body`).
///
/// ```ignore
/// <head>
///     theme_init_script(default_dark)
///     <link rel="stylesheet" href=(tailwind::stylesheet!())>
/// </head>
/// ```
#[component]
pub async fn theme_init_script(default_dark: bool) -> Result<impl View> {
    let body = theme_init_script_body(default_dark);
    Ok(view! { <script>(body)</script> })
}

#[cfg(test)]
mod tests;
