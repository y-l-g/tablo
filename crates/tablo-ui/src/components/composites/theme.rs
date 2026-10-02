use std::fmt::Write as _;

use topcoat::{
    Result,
    view::{View, component, view},
};

/// Reconciles the `dark` class on `<html>` before first paint.
///
/// A stored `light` removes a server-rendered `dark` class.
///
/// Uses nested `if`s instead of `&&`, which HTML escaping would rewrite into a syntax error inside `<script>`.
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

/// Renders in the `<head>` of every layout, before the stylesheet link.
///
/// `default_dark` is the server's build-time preference, used only when the visitor has no stored choice.
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
