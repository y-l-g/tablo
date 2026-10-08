use std::collections::HashMap;

use topcoat::context::Cx;

use super::{
    super::test_support::{cx, opening_tag_at, tag_with},
    *,
};
use crate::{
    form::FieldErrors,
    schema::{Schema, Source},
};

/// The opening `<input …>` tag around the file control, so assertions do
/// not have to care about attribute order (topcoat#122).
fn file_input_tag(html: &str) -> String {
    let at = html.find("type=\"file\"").expect("a file input");
    let start = html[..at].rfind("<input").expect("its opening tag");
    opening_tag_at(html, start).to_string()
}

fn cx_and_doc_schema() -> (Cx, Schema) {
    #[derive(Debug, toasty::Model)]
    struct Upload {
        #[key]
        #[auto]
        id: uuid::Uuid,
        path: String,
    }
    (
        cx(),
        Schema::new(Field::file(Upload::fields().path()).required()),
    )
}

async fn render_upload(schema: &Schema, cx: &Cx, value: Option<&str>) -> String {
    let mut values = HashMap::new();
    if let Some(value) = value {
        values.insert("path".to_string(), value.to_string());
    }
    schema
        .render(cx, Source::form(&values, &FieldErrors::new()))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(cx)
}

/// The same stored path as a detail page's [`FileColumn`](crate::FileColumn) shows it.
async fn render_readonly_upload(cx: &Cx, value: Option<&str>) -> String {
    stored_upload(cx, value.unwrap_or_default())
        .single()
        .await
        .unwrap()
        .render(cx)
}

/// Nothing stored (a create) keeps the required contract — the
/// browser blocks an empty submit and the server reports it inline.
#[tokio::test]
async fn file_upload_is_required_on_create() {
    let (cx, schema) = cx_and_doc_schema();
    let html = render_upload(&schema, &cx, None).await;
    let tag = file_input_tag(&html);
    assert!(
        tag.contains("required"),
        "a create must keep the required file control, got {tag}"
    );
    assert!(
        !html.contains("data-file-current"),
        "a create has no stored path to show, got {html}"
    );
    assert!(
        !html.contains("Leave empty"),
        "the keep-current hint is an edit affordance, got {html}"
    );
}

/// A stored path (an edit) makes the control optional and shows
/// what is stored, because a file input cannot be pre-filled — otherwise
/// the browser blocks every save and the server's untouched-value backfill
/// never gets a request to act on.
#[tokio::test]
async fn file_upload_surfaces_the_stored_path_and_drops_required() {
    let (cx, schema) = cx_and_doc_schema();
    let html = render_upload(&schema, &cx, Some("/uploads/cover.jpg")).await;
    let tag = file_input_tag(&html);
    assert!(
        !tag.contains("required"),
        "an edit must not block on the empty file control, got {tag}"
    );
    assert!(
        tag.contains("aria-describedby=\"path-hint\""),
        "the control must describe itself with the hint, got {tag}"
    );
    assert!(
        html.contains("data-file-current=\"/uploads/cover.jpg\""),
        "the stored path must be surfaced, got {html}"
    );
    assert!(
        html.contains("Leave empty to keep the current file."),
        "the edit must say an empty control keeps the file, got {html}"
    );
}

/// A stored path that is only whitespace is not a file: it must behave as
/// a create, not as an edit with a blank "Current:" line.
#[tokio::test]
async fn file_upload_treats_a_blank_stored_path_as_empty() {
    let (cx, schema) = cx_and_doc_schema();
    let html = render_upload(&schema, &cx, Some("   ")).await;
    let tag = file_input_tag(&html);
    assert!(
        tag.contains("required"),
        "a blank stored path must stay required, got {tag}"
    );
    assert!(
        !html.contains("data-file-current"),
        "a blank stored path must not render a Current line, got {html}"
    );
}

/// The stored path is a link to the file it names, whatever the
/// extension — the framework renders what the app stored, invents no URL
/// convention, and keeps no image pipeline.
#[tokio::test]
async fn file_upload_links_the_stored_file() {
    let (cx, schema) = cx_and_doc_schema();
    let image = render_upload(&schema, &cx, Some("/uploads/cover.png")).await;
    assert!(
        !image.contains("<img"),
        "a stored path is never rendered as an image, got {image}"
    );
    assert!(
        tag_with(&image, "href=\"/uploads/cover.png\"").starts_with("<a"),
        "the stored path must be a link to the file, got {image}"
    );
    assert!(
        image.contains("data-file-current=\"/uploads/cover.png\""),
        "the path stays readable beside the link, got {image}"
    );

    // The stored value is opaque: a query string is part of the path the
    // app stored and reaches the link unchanged.
    let signed = render_upload(&schema, &cx, Some("/media/photo.JPG?token=abc")).await;
    assert!(
        tag_with(&signed, "href=\"/media/photo.JPG?token=abc\"").starts_with("<a"),
        "the stored path is rendered verbatim, got {signed}"
    );
}

/// A stored value becomes an `href` only when it is a rooted path
/// or an absolute `http(s)` URL. Every other spelling — a scheme such as
/// `javascript:` or `data:`, the scheme-relative `//host`, a bare basename
/// — renders as text in both the edit row and the detail value: the
/// framework stores what it is handed, so the render is where a scheme is
/// refused.
#[tokio::test]
async fn file_upload_links_only_a_rooted_or_http_url() {
    let (cx, schema) = cx_and_doc_schema();
    for refused in [
        "javascript:alert(1)",
        "JavaScript:alert(1)",
        "data:text/html,x",
        "//evil.example/x.png",
        "report.pdf",
    ] {
        let edit = render_upload(&schema, &cx, Some(refused)).await;
        assert!(
            !edit.contains("href="),
            "{refused} must not become a link on the form, got {edit}"
        );
        assert!(
            edit.contains(&format!("data-file-current=\"{refused}\"")),
            "{refused} must stay visible on the form, got {edit}"
        );
        let view = render_readonly_upload(&cx, Some(refused)).await;
        assert!(
            !view.contains("href="),
            "{refused} must not become a link on the detail page, got {view}"
        );
        assert!(
            view.contains(refused),
            "{refused} must still render as text on the detail page, got {view}"
        );
    }

    for linkable in ["/uploads/a.png", "https://cdn.example/a.png"] {
        let edit = render_upload(&schema, &cx, Some(linkable)).await;
        assert!(
            tag_with(&edit, &format!("href=\"{linkable}\"")).starts_with("<a"),
            "{linkable} must stay a link on the form, got {edit}"
        );
        let view = render_readonly_upload(&cx, Some(linkable)).await;
        assert!(
            tag_with(&view, &format!("href=\"{linkable}\"")).starts_with("<a"),
            "{linkable} must stay a link on the detail page, got {view}"
        );
    }
}

/// The clear control belongs to a stored value — it is the only
/// way to say "remove the file" rather than "leave it alone".
#[tokio::test]
async fn file_upload_offers_the_clear_control_only_for_a_stored_value() {
    let (cx, schema) = cx_and_doc_schema();
    let edit = render_upload(&schema, &cx, Some("/uploads/cover.png")).await;
    assert!(
        edit.contains("name=\"clear_path\""),
        "an edit must post the declared transport key, got {edit}"
    );
    assert!(
        edit.contains("value=\"1\""),
        "the control must carry the truthy value the handlers read, got {edit}"
    );
    assert!(
        edit.contains("Remove the current file"),
        "the label must say what ticking it does, got {edit}"
    );

    let create = render_upload(&schema, &cx, None).await;
    assert!(
        !create.contains("name=\"clear_path\""),
        "a create has nothing to clear, got {create}"
    );
}
