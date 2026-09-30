use super::*;

#[test]
fn brand_trims_name_and_ignores_blank_logo() {
    assert_eq!(Brand::new("  Acme  ").name, "Acme");
    assert_eq!(Brand::new("Acme").logo, None);
    assert_eq!(
        Brand::new("Acme").logo("  /logo.svg  ").logo.as_deref(),
        Some("/logo.svg")
    );
    // Blank logos fall back to the name-only render.
    assert_eq!(Brand::new("Acme").logo("   ").logo, None);
}

#[tokio::test]
async fn layout_shell_renders_a_complete_document() {
    use topcoat::{context::CxTestBuilder, view::view};

    use crate::resource::{NavTarget, NavigationItem};

    let (parts, ()) = http::Request::builder()
        .uri("/admin/users")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new()
        .request_context(parts)
        .app_context(vec![NavigationItem {
            label: "Users".to_string(),
            target: NavTarget::Url("/admin/users".to_string()),
            order: 0,
        }])
        .build();
    let cx_ref = &cx;
    let slot = view! { cx_ref => "hello" }.boxed().into();
    let html = Panel::layout_shell(&cx, slot)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);

    assert!(
        html.starts_with("<!DOCTYPE html>"),
        "missing doctype in {html}"
    );
    assert!(
        html.contains("<html>") && html.contains("<head>"),
        "missing document head in {html}"
    );
    assert!(
        html.contains("<title>Tablo</title>"),
        "missing document title in {html}"
    );
    assert!(html.contains("hello"), "missing layout slot in {html}");
}

/// A page outside the panel takes the panel's document: the same head, around
/// the body the page owns.
#[tokio::test]
async fn document_wraps_a_public_page() {
    use topcoat::{context::CxTestBuilder, view::view};

    let (parts, ()) = http::Request::builder()
        .uri("/blog")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new().request_context(parts).build();
    let cx_ref = &cx;
    let html = Panel::document(
        &cx,
        "Tablo Blog",
        view! { cx_ref => <body class="blog">"hello"</body> },
    )
    .await
    .unwrap()
    .single()
    .await
    .unwrap()
    .render(&cx);

    assert!(
        html.starts_with("<!DOCTYPE html>"),
        "missing doctype in {html}"
    );
    assert!(
        html.contains("<title>Tablo Blog</title>"),
        "missing document title in {html}"
    );
    assert!(
        html.contains("<body class=\"blog\">"),
        "missing the page's own body in {html}"
    );
    assert!(html.contains("hello"), "missing page content in {html}");
    // No `ShellAssets` in app context: the head carries no asset URL, so a
    // router built without `.assets(..)` renders instead of panicking.
    assert!(
        !html.contains("rel=\"stylesheet\""),
        "no stylesheet without shell assets, got {html}"
    );
}

#[tokio::test]
async fn shell_escapes_brand_name_and_logo() {
    use topcoat::{context::CxTestBuilder, view::view};

    use crate::resource::{NavTarget, NavigationItem};

    // Attribute-injection safety rests on `view!` escaping:
    // lock it with a hostile brand on both render paths (header + sidebar).
    let (parts, ()) = http::Request::builder()
        .uri("/admin/users")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new()
        .request_context(parts)
        .app_context(Brand::new("<script>alert(1)</script>").logo("\"><script>alert(2)</script>"))
        .build();
    let nav_items = vec![NavigationItem {
        label: "Users".to_string(),
        target: NavTarget::Url("/admin/users".to_string()),
        order: 0,
    }];
    let cx_ref = &cx;
    let slot = view! { cx_ref => "hello" }.boxed().into();
    let html = Panel::render_shell(&cx, &nav_items, "/admin/users", slot, None)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        !html.contains("\"><script>"),
        "brand must not break out of attributes, got {html}"
    );
    assert!(
        html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "missing escaped brand text, got {html}"
    );
    assert!(
        html.contains("&quot;"),
        "attribute quotes must be escaped, got {html}"
    );
}

#[tokio::test]
async fn shell_dark_mode_sets_html_class_and_toggle() {
    fn cx_with(dark: Option<bool>) -> Cx {
        use topcoat::context::CxTestBuilder;
        let (parts, ()) = http::Request::builder()
            .uri("/admin/users")
            .body(())
            .unwrap()
            .into_parts();
        match dark {
            Some(v) => CxTestBuilder::new()
                .request_context(parts)
                .app_context(DarkMode(v)),
            None => CxTestBuilder::new().request_context(parts),
        }
        .build()
    }

    async fn document_html(cx: &Cx) -> String {
        let slot = view! { cx => "hello" }.boxed().into();
        Panel::layout_shell(cx, slot)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(cx)
    }

    let html = document_html(&cx_with(Some(true))).await;
    assert!(
        html.contains("<html class=\"dark\">"),
        "DarkMode(true) must set the html class, got {html}"
    );
    assert!(
        html.contains("data-theme-toggle"),
        "theme toggle must render, got {html}"
    );
    // the build-time default is only that — the pre-paint script
    // can remove the class again when the visitor has stored `light`.
    assert!(
        html.contains("classList.add") && html.contains("classList.remove"),
        "the document must carry the reconciling theme script, got {html}"
    );

    let html = document_html(&cx_with(Some(false))).await;
    assert!(
        html.contains("<html>"),
        "DarkMode(false) must not set the dark class, got {html}"
    );

    let html = document_html(&cx_with(None)).await;
    assert!(
        html.contains("<html>"),
        "no DarkMode must not set the dark class, got {html}"
    );
}

#[tokio::test]
async fn theme_cookie_overrides_the_dark_mode_default() {
    // Runtime navigation copies `<html>`'s attributes from the next page
    // and runs no script, so the server must render the stored choice.
    use topcoat::context::CxTestBuilder;

    async fn html_tag(default_dark: bool, cookie: &str) -> String {
        let (parts, ()) = http::Request::builder()
            .uri("/admin/users")
            .header(http::header::COOKIE, cookie)
            .body(())
            .unwrap()
            .into_parts();
        let cx = CxTestBuilder::new()
            .request_context(parts)
            .app_context(DarkMode(default_dark))
            .build();
        let cx_ref = &cx;
        let slot = view! { cx_ref => "hello" }.boxed().into();
        let html = Panel::layout_shell(&cx, slot)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        let start = html.find("<html").unwrap();
        html[start..=start + html[start..].find('>').unwrap()].to_string()
    }

    assert_eq!(html_tag(false, "theme=dark").await, "<html class=\"dark\">");
    assert_eq!(
        html_tag(true, "sidebar_state=collapsed; theme=light").await,
        "<html>"
    );
    assert_eq!(html_tag(true, "theme=sepia").await, "<html class=\"dark\">");
}

#[tokio::test]
async fn shell_notification_carries_dismiss_hooks() {
    // GH #97/#151: the shell toast is the shadcn/Sonner surface, carrying
    // the auto-dismiss hooks notifications.js arms (mount + 4s + close).
    use crate::notification::Notification;

    let enc = serde_json::to_string(&Notification::success("Created")).unwrap();
    let html = shell_html_with_flash(&enc).await;
    assert!(
        html.contains("data-sonner-toast") && html.contains("data-type=\"success\""),
        "shell toast must be the Sonner surface, got {html}"
    );
    assert!(
        html.contains("data-close-button"),
        "shell toast must carry the Sonner close button, got {html}"
    );
    assert!(
        html.contains("data-title") && html.contains("Created"),
        "shell toast must carry the title, got {html}"
    );
    assert!(
        !html.contains("data-description"),
        "a title-only toast renders no description, got {html}"
    );

    // Error + description: the typed toast and the supporting line. The
    // icon's colour is paint; `data-type="error"` is what selects
    // the destructive icon, and the `<svg>` proves one rendered.
    let enc =
        serde_json::to_string(&Notification::error("Boom").description("What happened")).unwrap();
    let html = shell_html_with_flash(&enc).await;
    assert!(
        html.contains("data-type=\"error\"") && html.contains("<svg"),
        "an error toast must carry its type and destructive icon, got {html}"
    );
    assert!(
        html.contains("data-description") && html.contains("What happened"),
        "the description must render, got {html}"
    );
}

/// The opening tag that starts at `start`, sliced up to the `>` closing it.
///
/// `Attributes` renders in no guaranteed order (topcoat#122), so a test
/// locates a tag by whichever attribute it can and asserts on the whole
/// tag. Quoting is honoured, so a `>` inside an attribute value (Tailwind
/// selectors and arrow-function handlers both carry them) does not end
/// the slice.
fn opening_tag_at(html: &str, start: usize) -> &str {
    let mut quoted = false;
    for (offset, byte) in html.as_bytes()[start..].iter().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b'>' if !quoted => return &html[start..start + offset],
            _ => {}
        }
    }
    panic!("unterminated tag at byte {start} in {html}");
}

/// Render the shell once with a flash cookie carrying `enc`.
async fn shell_html_with_flash(enc: &str) -> String {
    use topcoat::{context::CxTestBuilder, cookie::CookieJarCell, view::view};

    use crate::resource::{NavTarget, NavigationItem};

    let mut parts = http::Request::builder()
        .uri("/admin/users")
        .body(())
        .unwrap()
        .into_parts()
        .0;
    parts.headers.insert(
        http::header::COOKIE,
        format!("{}={enc}", crate::notification::COOKIE_NAME)
            .parse()
            .unwrap(),
    );
    let cx = CxTestBuilder::new()
        .request_context(parts)
        .request_context(CookieJarCell::new())
        .build();
    let nav_items = vec![NavigationItem {
        label: "Users".to_string(),
        target: NavTarget::Url("/admin/users".to_string()),
        order: 0,
    }];
    let cx_ref = &cx;
    let slot = view! { cx_ref => "hello" }.boxed().into();
    Panel::render_shell(&cx, &nav_items, "/admin/users", slot, None)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx)
}

#[tokio::test]
async fn sidebar_orders_custom_items_by_sort_key() {
    // `order: -1` interleaves a custom item above the
    // resources; ties keep declaration order.
    use topcoat::{context::CxTestBuilder, view::view};

    use crate::resource::{NavTarget, NavigationItem};

    let (parts, ()) = http::Request::builder()
        .uri("/admin/users")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new().request_context(parts).build();
    let nav_items = vec![
        NavigationItem {
            label: "Users".to_string(),
            target: NavTarget::Url("/admin/users".to_string()),
            order: 0,
        },
        NavigationItem {
            label: "Showcase".to_string(),
            target: NavTarget::Url("/admin/showcase".to_string()),
            order: -1,
        },
    ];
    let cx_ref = &cx;
    let slot = view! { cx_ref => "hello" }.boxed().into();
    let html = Panel::render_shell(&cx, &nav_items, "/admin/users", slot, None)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    let showcase_at = html.find("Showcase").expect("custom item renders");
    let users_at = html.find("Users").expect("resource item renders");
    assert!(
        showcase_at < users_at,
        "an order: -1 custom item must precede resources, got {html}"
    );
}

#[tokio::test]
async fn panel_shell_renders_sidebar_with_active_and_tokens() {
    // structure/aria only — pixel Token/Tailwind classes live in
    // the showcase (`admin_resource_list_page_serve_seeded_users`), so a
    // restyle does not fail core without a behavior change.
    use topcoat::{context::CxTestBuilder, view::view};

    use crate::resource::{NavTarget, NavigationItem};

    let cx = CxTestBuilder::new().build();
    let cx_ref = &cx;
    let nav_items = vec![
        NavigationItem {
            label: "Users".to_string(),
            target: NavTarget::Url("/admin/users".to_string()),
            order: 0,
        },
        NavigationItem {
            label: "Showcase".to_string(),
            target: NavTarget::Url("/admin/showcase".to_string()),
            order: 0,
        },
    ];
    let slot = view! { cx_ref => "hello" }.boxed().into();
    let html = Panel::render_shell(&cx, &nav_items, "/admin/users", slot, None)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("data-sidebar=\"sidebar\""),
        "missing sidebar data attr in {html}"
    );
    assert!(
        html.contains("data-sidebar=\"provider\""),
        "missing provider data attr in {html}"
    );
    assert!(
        html.contains("data-sidebar=\"inset\""),
        "missing inset data attr in {html}"
    );
    assert!(
        html.contains("data-sidebar=\"group\"") || html.contains("Navigation"),
        "missing sidebar group in {html}"
    );
    // Each entry navigates through the runtime; the menu button writes the
    // one `href`, so the runtime attributes add none.
    assert!(
        html.contains("href=\"/admin/showcase\"") && html.contains("data-topcoat-link="),
        "sidebar entries must carry runtime navigation in {html}"
    );
    assert!(!html.contains("href=\"\""), "no empty href in {html}");
    // Data-state for collapsible, seeded by the signal (cookie default:
    // expanded) and bound for the browser runtime.
    assert!(
        html.contains("data-state=\"expanded\"") && html.contains("data-topcoat-bind:data-state"),
        "missing bound data-state in {html}"
    );
    // No separator: the dead "Resources" placeholder group it divided
    // is gone, and a trailing rule with no following group is
    // chrome noise.
    assert!(
        !html.contains("Managed via Resource::query seam"),
        "dead placeholder must be gone, got {html}"
    );
    // The desktop/mobile trigger pair, wired to the runtime signals.
    assert_eq!(
        html.matches("data-sidebar=\"trigger\"").count(),
        2,
        "expected the desktop + mobile trigger pair in {html}"
    );
    assert!(
        html.contains("data-topcoat-on:click"),
        "missing runtime click bindings in {html}"
    );
    // Active highlight + real navigation links (the href prop, not attrs)
    assert!(
        html.contains("data-active=\"true\"") && html.contains("aria-current=\"page\""),
        "missing active highlight in {html}"
    );
    assert!(
        html.contains("<a") && html.contains("href=\"/admin/users\""),
        "navigation must render as links in {html}"
    );
    // Dark toggle
    assert!(
        html.contains("Toggle dark mode") || html.contains("data-theme-toggle"),
        "missing dark toggle in {html}"
    );
    // Ensure no ac-* remains in shell
    assert!(
        !html.contains("ac-sidebar") && !html.contains("ac-main") && !html.contains("ac-nav-item"),
        "ac-* should not remain in shell, got {html}"
    );
}

/// `sidebar_inset` renders the document's `<main>`; the slot wrapper must
/// be a plain element or the document carries two nested landmarks
/// (invalid HTML, and landmark navigation lists both).
#[tokio::test]
async fn shell_has_a_single_main_landmark() {
    use topcoat::{context::CxTestBuilder, view::view};

    let cx = CxTestBuilder::new().build();
    let cx_ref = &cx;
    let slot = view! { cx_ref => "hello" }.boxed().into();
    let html = Panel::render_shell(&cx, &[], "/admin", slot, None)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert_eq!(
        html.matches("<main").count(),
        1,
        "the shell must expose exactly one main landmark, got {html}"
    );
    let main_tag = opening_tag_at(&html, html.find("<main").expect("main landmark"));
    assert!(
        main_tag.contains("data-sidebar=\"inset\""),
        "the inset stays the main landmark, got {main_tag}"
    );
}

/// The sheet header carries a mobile-only close control (upstream
/// `examples/ui`, `sidebar`'s "Include a close button in the mobile
/// header"): below `md` the open sheet veils the inset header, so the
/// mobile trigger there is unreachable and the sheet needs its own.
#[tokio::test]
async fn sidebar_sheet_header_carries_a_mobile_close_button() {
    use topcoat::{context::CxTestBuilder, view::view};

    let cx = CxTestBuilder::new().build();
    let cx_ref = &cx;
    let slot = view! { cx_ref => "hello" }.boxed().into();
    let html = Panel::render_shell(&cx, &[], "/admin", slot, None)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    let label = html
        .find("aria-label=\"Close sidebar\"")
        .unwrap_or_else(|| panic!("missing Close sidebar control in {html}"));
    // Find the tag by its label, then assert on the whole opening tag:
    // `Attributes` renders in no guaranteed order (topcoat#122).
    let tag_start = html[..label].rfind('<').expect("close control tag start");
    let tag = opening_tag_at(&html, tag_start);
    // `md:hidden` is the responsive class that makes this control
    // mobile-only, and it is paint; what a regression would break is that
    // the close control is a real button wired to the sheet's close hook.
    assert!(
        tag.contains("<button") && tag.contains("data-topcoat-on:click"),
        "close control must be a button wired to close the sheet, got {tag}"
    );
}

#[tokio::test]
async fn toaster_renders_no_stray_span_when_empty() {
    // the toaster renders an `<ol>`, which permits only
    // `li`/`script`/`template` children — with no flash notification and
    // no live toast, neither slot may strand a `<span>` in the list.
    use topcoat::{context::CxTestBuilder, view::view};

    use crate::resource::NavigationItem;

    let cx = CxTestBuilder::new().build();
    let cx_ref = &cx;
    let nav_items: Vec<NavigationItem> = vec![];
    let slot = view! { cx_ref => "hello" }.boxed().into();
    let html = Panel::render_shell(&cx, &nav_items, "/admin/users", slot, None)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    let ol = &html[html.find("<ol").expect("toaster list")..];
    let ol = &ol[..ol.find("</ol>").expect("toaster close") + "</ol>".len()];
    assert!(
        ol.contains("data-sonner-toaster"),
        "sliced the toaster list, got {ol}"
    );
    assert!(
        !ol.contains("<span"),
        "empty toaster must not strand a span in the list, got {ol}"
    );
}

#[tokio::test]
async fn render_shell_extra_class_reaches_the_provider() {
    // `extra_class` is the custom-document extension point — a
    // passed class must reach the provider markup.
    use topcoat::context::CxTestBuilder;

    let cx = CxTestBuilder::new().build();
    let cx_ref = &cx;
    let nav_items: Vec<NavigationItem> = vec![];
    let slot = view! { cx_ref => "hello" }.boxed().into();
    let html = Panel::render_shell(
        &cx,
        &nav_items,
        "/admin/users",
        slot,
        Some("my-shell".to_string()),
    )
    .await
    .unwrap()
    .single()
    .await
    .unwrap()
    .render(&cx);
    assert!(
        html.contains("my-shell"),
        "extra_class must reach the shell markup, got {html}"
    );
}

#[tokio::test]
async fn collapsed_sidebar_cookie_seeds_the_signal() {
    // The persisted `sidebar_state` cookie seeds the runtime signal's
    // initial value, so the first paint matches the last choice; from
    // hydration on, the browser owns the state (assets/sidebar.js mirrors
    // it back).
    use topcoat::{context::CxTestBuilder, view::view};

    use crate::resource::{NavTarget, NavigationItem};

    let (parts, ()) = http::Request::builder()
        .uri("/admin/users")
        .header(http::header::COOKIE, "theme=dark; sidebar_state=collapsed")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new().request_context(parts).build();
    let cx_ref = &cx;
    let nav_items = vec![NavigationItem {
        label: "Users".to_string(),
        target: NavTarget::Url("/admin/users".to_string()),
        order: 0,
    }];
    let slot = view! { cx_ref => "hello" }.boxed().into();
    let html = Panel::render_shell(&cx, &nav_items, "/admin/users", slot, None)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("data-state=\"collapsed\"")
            && html.contains("data-collapsible=\"offcanvas\""),
        "collapsed cookie must seed the collapsed state in {html}"
    );
}
