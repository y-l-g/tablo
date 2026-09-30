use topcoat::{context::CxTestBuilder, view::ViewExt};

use super::*;

async fn rendered(default_dark: bool) -> String {
    let cx = CxTestBuilder::new().build();
    let cx_ref = &cx;
    view! { cx_ref => theme_init_script(default_dark: default_dark) }
        .single()
        .await
        .unwrap()
        .render(&cx)
}

#[tokio::test]
async fn renders_blocking_head_script() {
    let html = rendered(false).await;
    assert!(
        html.starts_with("<script>"),
        "expected an inline script, got {html}"
    );
    assert!(
        html.contains("classList.add") && html.contains("classList.remove"),
        "the pre-paint script must reconcile the class both ways (GH #184), got {html}"
    );
}

/// The inline script must survive HTML escaping intact: character
/// references are not decoded inside `<script>`, so an escaped `&&` would
/// ship a broken program.
#[tokio::test]
async fn script_body_carries_no_html_escapes() {
    for default_dark in [true, false] {
        let html = rendered(default_dark).await;
        assert!(
            !html.contains("&amp;") && !html.contains("&lt;") && !html.contains("&quot;"),
            "inline theme script must not need HTML escaping, got {html}"
        );
    }
}

/// GH #184: the server's build-time preference is only the fallback — it
/// is interpolated for a visitor with no stored choice, and never widens
/// the script's authority over a stored one.
#[tokio::test]
async fn default_dark_is_the_fallback_only() {
    let dark = rendered(true).await;
    assert!(
        dark.contains("t='dark'"),
        "dark default must be interpolated as the fallback, got {dark}"
    );

    let light = rendered(false).await;
    assert!(
        light.contains("t='light'"),
        "light default must be interpolated as the fallback, got {light}"
    );
    assert!(
        !light.contains("t='dark'"),
        "a light default must not interpolate dark, got {light}"
    );
}
