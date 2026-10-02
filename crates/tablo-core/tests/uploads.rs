//! The upload seam end to end.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use http::header::{
    CONTENT_DISPOSITION, IF_MODIFIED_SINCE, LAST_MODIFIED, LOCATION, X_CONTENT_TYPE_OPTIONS,
};
use tablo_core::{
    Ability, Auth, Field, Panel, Policy, Resource, Schema, Table, TextColumn, Uploader,
};
use toasty::Db;
use topcoat::{
    context::Cx,
    router::{Body, Router, response::Response},
};
use uuid::Uuid;

use crate::common::{
    body_bytes, body_string, csp, get, memory_db, mount, multipart_body, new_csrf, panel, post,
    post_multipart,
};

/// A document with one required and one optional upload.
#[derive(Debug, Clone, toasty::Model)]
struct Doc {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    /// Required by the form's default (the lens is a non-nullable `String`).
    cover: String,
    /// Declared `.optional()`: the app allows a record to lose its file.
    attachment: String,
}

/// What an uploader was handed: sanitized filename and bytes.
type Seen = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// An uploader that records what it was handed and answers a deterministic path.
#[derive(Clone, Default)]
struct RecordingUploader {
    seen: Seen,
}

impl RecordingUploader {
    fn seen(&self) -> Vec<(String, Vec<u8>)> {
        self.seen.lock().expect("uploader lock").clone()
    }
}

impl Uploader for RecordingUploader {
    async fn store(&self, filename: &str, bytes: &[u8]) -> Result<String, String> {
        self.seen
            .lock()
            .expect("uploader lock")
            .push((filename.to_string(), bytes.to_vec()));
        Ok(format!("/uploads/{filename}"))
    }
}

/// An uploader that always refuses, for the inline-error path.
struct FailingUploader;

impl Uploader for FailingUploader {
    async fn store(&self, _filename: &str, _bytes: &[u8]) -> Result<String, String> {
        Err("this deployment has no room left".to_string())
    }
}

struct DocResource;

impl Resource for DocResource {
    type Model = Doc;
    type Form = DocForm;
    fn form(_dx: &tablo_core::DeclCx) -> Schema {
        Schema::new((
            Field::text(Doc::fields().title()),
            Field::file(Doc::fields().cover()).label("Cover"),
            Field::file(Doc::fields().attachment())
                .label("Attachment")
                .optional(),
        ))
    }

    // Every policy hook defaults to deny.
    fn policy() -> impl Policy<Doc> {
        |_cx: &Cx, ability: Ability<'_, Doc>| {
            matches!(
                ability,
                Ability::ViewAny | Ability::View(_) | Ability::Create | Ability::Update(_)
            )
        }
    }

    fn table() -> Table<Doc> {
        Table::new(
            |doc: &Doc| doc.id.to_string(),
            TextColumn::r#for(Doc::fields().title(), |doc: &Doc| doc.title.clone()),
        )
        .paginate(25)
    }
}
#[derive(tablo_core::RecordForm)]
#[form(model = Doc)]
struct DocForm {
    title: String,
    cover: String,
    attachment: String,
}
async fn seeded_db() -> Db {
    memory_db(toasty::models!(Doc)).await
}

/// The same DB with the shipped auth models registered.
async fn auth_seeded_db() -> Db {
    memory_db(toasty::models!(
        Doc,
        tablo_core::auth::AdminUser,
        tablo_core::auth::AuthSession
    ))
    .await
}

/// A panel over `Doc`, optionally with an uploader.
fn router(db: Db, uploader: Option<impl Uploader>) -> Router {
    let panel = panel();
    let panel = match uploader {
        Some(uploader) => panel.uploads(uploader),
        None => panel,
    };
    mount(db, panel.resource::<DocResource>()).expect("panel builds")
}

/// A directory of this test's own.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tablo-uploads-{tag}-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// A GET that revalidates.
async fn get_if_modified_since(router: &Router, uri: &str, since: &str) -> Response<Body> {
    let request = http::Request::builder()
        .uri(uri)
        .header(IF_MODIFIED_SINCE, since)
        .body(Body::empty())
        .expect("request builds");
    router.handle(request).await
}

/// The exact directive a served file carries.
const SERVED_FILE_POLICY: &str = "default-src 'none'; img-src 'self'; media-src 'self'; \
     style-src 'unsafe-inline'; sandbox; frame-ancestors 'self'";

async fn seed_doc(db: &Db, title: &str, cover: &str, attachment: &str) -> Doc {
    let mut db = db.clone();
    toasty::create!(Doc {
        title: title.to_string(),
        cover: cover.to_string(),
        attachment: attachment.to_string(),
    })
    .exec(&mut db)
    .await
    .expect("seed doc")
}

async fn docs(db: &Db) -> Vec<Doc> {
    let mut db = db.clone();
    Doc::all().exec(&mut db).await.expect("query docs")
}

#[tokio::test]
async fn an_installed_uploader_stores_the_bytes_and_the_path_reaches_the_record() {
    let db = seeded_db().await;
    let uploader = RecordingUploader::default();
    let router = router(db.clone(), Some(uploader.clone()));
    let csrf = new_csrf();
    let body = multipart_body(
        "B",
        &[
            ("title", None, "Notes"),
            ("cover", Some("cover.png"), "PNG-BYTES"),
            ("attachment", Some("spec.pdf"), "PDF-BYTES"),
            ("csrf_token", None, &csrf),
        ],
    );

    let response = post_multipart(&router, "/admin/docs/create", &csrf, "B", body).await;
    assert_eq!(response.status(), 303, "a valid create redirects");

    // The record stores what the uploader returned.
    let created = docs(&db).await;
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].cover, "/uploads/cover.png");
    assert_eq!(created[0].attachment, "/uploads/spec.pdf");

    // The uploader saw the bytes under sanitized names.
    let mut seen = uploader.seen();
    seen.sort();
    assert_eq!(
        seen,
        vec![
            ("cover.png".to_string(), b"PNG-BYTES".to_vec()),
            ("spec.pdf".to_string(), b"PDF-BYTES".to_vec()),
        ],
        "the uploader receives each file part's sanitized name and content"
    );
}

#[tokio::test]
async fn without_an_uploader_the_sanitized_basename_is_still_stored() {
    let db = seeded_db().await;
    let router = router(db.clone(), None::<RecordingUploader>);
    let csrf = new_csrf();
    let body = multipart_body(
        "B",
        &[
            ("title", None, "Notes"),
            // A path-carrying client name is sanitized to its basename.
            ("cover", Some("../../etc/cover.png"), "PNG-BYTES"),
            ("csrf_token", None, &csrf),
        ],
    );

    let response = post_multipart(&router, "/admin/docs/create", &csrf, "B", body).await;
    assert_eq!(response.status(), 303);

    let created = docs(&db).await;
    assert_eq!(created[0].cover, "cover.png");
    assert_eq!(created[0].attachment, "");
}

#[tokio::test]
async fn a_refused_upload_is_an_inline_field_error_and_writes_nothing() {
    let db = seeded_db().await;
    let router = router(db.clone(), Some(FailingUploader));
    let csrf = new_csrf();
    let body = multipart_body(
        "B",
        &[
            ("title", None, "Notes"),
            ("cover", Some("cover.png"), "PNG-BYTES"),
            ("csrf_token", None, &csrf),
        ],
    );

    let response = post_multipart(&router, "/admin/docs/create", &csrf, "B", body).await;
    assert_eq!(response.status(), 200, "the form re-renders");
    let html = body_string(response).await;
    assert!(
        html.contains("Cover could not be uploaded: this deployment has no room left"),
        "the uploader's reason must reach the field's inline error: {html}"
    );
    assert!(
        !html.contains("Cover is required"),
        "'required' would restate the symptom and hide the reason: {html}"
    );
    assert!(
        !html.contains("data-file-current"),
        "a create must not present the refused filename as a stored file: {html}"
    );
    assert!(
        docs(&db).await.is_empty(),
        "a refused upload must not create the record"
    );
}

#[tokio::test]
async fn an_untouched_file_input_keeps_the_stored_path_and_a_chosen_one_replaces_it() {
    let db = seeded_db().await;
    let uploader = RecordingUploader::default();
    let router = router(db.clone(), Some(uploader.clone()));
    let doc = seed_doc(&db, "Original", "cover.png", "spec.pdf").await;

    // A browser submits every file input.
    let csrf = new_csrf();
    let body = multipart_body(
        "B",
        &[
            ("title", None, "Renamed"),
            ("cover", Some(""), ""),
            ("attachment", Some(""), ""),
            ("csrf_token", None, &csrf),
        ],
    );
    let response = post_multipart(
        &router,
        &format!("/admin/docs/{}/edit", doc.id),
        &csrf,
        "B",
        body,
    )
    .await;
    assert_eq!(response.status(), 303, "an untouched upload saves");
    let updated = docs(&db).await;
    assert_eq!(updated[0].title, "Renamed");
    assert_eq!(updated[0].cover, "cover.png", "the stored path is kept");
    assert_eq!(updated[0].attachment, "spec.pdf");
    assert!(
        uploader.seen().is_empty(),
        "an untouched file input must not reach the uploader"
    );

    // Choosing a file replaces the stored one: the new path is what is stored.
    let csrf = new_csrf();
    let body = multipart_body(
        "B",
        &[
            ("title", None, "Renamed"),
            ("cover", Some("new.png"), "NEW-BYTES"),
            ("attachment", Some(""), ""),
            ("csrf_token", None, &csrf),
        ],
    );
    let response = post_multipart(
        &router,
        &format!("/admin/docs/{}/edit", doc.id),
        &csrf,
        "B",
        body,
    )
    .await;
    assert_eq!(response.status(), 303);
    assert_eq!(docs(&db).await[0].cover, "/uploads/new.png");
}

/// A url-encoded pair under a declared file field's name is dropped before validation.
#[tokio::test]
async fn a_text_value_for_a_file_upload_is_not_stored_on_create() {
    let db = seeded_db().await;
    let router = router(db.clone(), Some(RecordingUploader::default()));
    let csrf = new_csrf();
    let body = format!("title=Notes&cover=javascript%3Aalert%281%29&csrf_token={csrf}");

    let response = post(
        &router,
        "/admin/docs/create",
        &csrf,
        "application/x-www-form-urlencoded".to_string(),
        body,
    )
    .await;
    assert_eq!(
        response.status(),
        200,
        "the form re-renders with the required error"
    );
    let html = body_string(response).await;
    assert!(
        html.contains("cover-error"),
        "the typed value leaves the required field empty: {html}"
    );
    assert!(
        !html.contains("javascript"),
        "the typed value must not survive into the re-rendered form: {html}"
    );
    assert!(
        docs(&db).await.is_empty(),
        "a client-typed upload value must not create the record"
    );
}

/// A multipart text part under a declared file field's name is dropped too.
#[tokio::test]
async fn a_text_value_for_a_file_upload_keeps_the_stored_file_on_edit() {
    let db = seeded_db().await;
    let router = router(db.clone(), Some(RecordingUploader::default()));
    let doc = seed_doc(&db, "Original", "/uploads/old.png", "spec.pdf").await;
    let csrf = new_csrf();
    let body = multipart_body(
        "B",
        &[
            ("title", None, "Renamed"),
            ("cover", None, "javascript:alert(1)"),
            ("csrf_token", None, &csrf),
        ],
    );

    let response = post_multipart(
        &router,
        &format!("/admin/docs/{}/edit", doc.id),
        &csrf,
        "B",
        body,
    )
    .await;
    assert_eq!(
        response.status(),
        303,
        "the edit saves with the stored file kept"
    );
    let updated = docs(&db).await;
    assert_eq!(
        updated[0].title, "Renamed",
        "the rest of the edit still applies"
    );
    assert_eq!(
        updated[0].cover, "/uploads/old.png",
        "a client-typed value must not replace the stored file"
    );
}

/// Duplicate part names are last-write-wins.
#[tokio::test]
async fn a_text_part_after_a_file_part_does_not_forge_a_value_on_create() {
    let db = seeded_db().await;
    let router = router(db.clone(), None::<RecordingUploader>);
    let csrf = new_csrf();
    let body = multipart_body(
        "B",
        &[
            ("title", None, "Notes"),
            ("cover", Some("cover.png"), "PNG-BYTES"),
            ("cover", None, "javascript:alert(1)"),
            ("csrf_token", None, &csrf),
        ],
    );

    let response = post_multipart(&router, "/admin/docs/create", &csrf, "B", body).await;
    assert_eq!(
        response.status(),
        200,
        "the form re-renders with the required error"
    );
    let html = body_string(response).await;
    assert!(
        html.contains("cover-error"),
        "the later text part leaves the field empty: {html}"
    );
    assert!(
        !html.contains("javascript"),
        "the typed value must not survive into the re-rendered form: {html}"
    );
    assert!(
        docs(&db).await.is_empty(),
        "the typed value must not create the record"
    );
}

/// The same duplicate-name bypass on edit, with an uploader installed.
#[tokio::test]
async fn a_text_part_after_a_file_part_keeps_the_stored_file_on_edit() {
    let db = seeded_db().await;
    let uploader = RecordingUploader::default();
    let router = router(db.clone(), Some(uploader.clone()));
    let doc = seed_doc(&db, "Original", "/uploads/old.png", "spec.pdf").await;
    let csrf = new_csrf();
    let body = multipart_body(
        "B",
        &[
            ("title", None, "Renamed"),
            ("cover", Some("new.png"), "NEW-BYTES"),
            ("cover", None, "javascript:alert(1)"),
            ("csrf_token", None, &csrf),
        ],
    );

    let response = post_multipart(
        &router,
        &format!("/admin/docs/{}/edit", doc.id),
        &csrf,
        "B",
        body,
    )
    .await;
    assert_eq!(response.status(), 303, "the edit saves");
    let updated = docs(&db).await;
    assert_eq!(
        updated[0].title, "Renamed",
        "the rest of the edit still applies"
    );
    assert_eq!(
        updated[0].cover, "/uploads/old.png",
        "the stored file must survive the duplicate name"
    );
    assert!(
        uploader.seen().is_empty(),
        "the discarded file part must not reach the uploader"
    );
}

/// The duplicate-name bypass on edit with no uploader.
#[tokio::test]
async fn a_text_part_after_a_file_part_keeps_the_stored_file_without_an_uploader() {
    let db = seeded_db().await;
    let router = router(db.clone(), None::<RecordingUploader>);
    let doc = seed_doc(&db, "Original", "/uploads/old.png", "spec.pdf").await;
    let csrf = new_csrf();
    let body = multipart_body(
        "B",
        &[
            ("title", None, "Renamed"),
            ("cover", Some("new.png"), "NEW-BYTES"),
            ("cover", None, "javascript:alert(1)"),
            ("csrf_token", None, &csrf),
        ],
    );

    let response = post_multipart(
        &router,
        &format!("/admin/docs/{}/edit", doc.id),
        &csrf,
        "B",
        body,
    )
    .await;
    assert_eq!(response.status(), 303, "the edit saves");
    assert_eq!(
        docs(&db).await[0].cover,
        "/uploads/old.png",
        "the stored file must survive the duplicate name"
    );
}

/// The last part wins in the other order too.
#[tokio::test]
async fn a_file_part_after_a_text_part_wins_on_create() {
    let db = seeded_db().await;
    let router = router(db.clone(), Some(RecordingUploader::default()));
    let csrf = new_csrf();
    let body = multipart_body(
        "B",
        &[
            ("title", None, "Notes"),
            ("cover", None, "javascript:alert(1)"),
            ("cover", Some("cover.png"), "PNG-BYTES"),
            ("csrf_token", None, &csrf),
        ],
    );

    let response = post_multipart(&router, "/admin/docs/create", &csrf, "B", body).await;
    assert_eq!(response.status(), 303, "the last part is a file and wins");
    let created = docs(&db).await;
    assert_eq!(
        created[0].cover, "/uploads/cover.png",
        "the file part's value is what the record stores"
    );
}

/// A later file part whose name sanitizes to empty discards the staged bytes.
#[tokio::test]
async fn a_rejected_filename_after_a_file_part_discards_the_staged_bytes() {
    let db = seeded_db().await;
    let uploader = RecordingUploader::default();
    let router = router(db.clone(), Some(uploader.clone()));
    let csrf = new_csrf();
    let body = multipart_body(
        "B",
        &[
            ("title", None, "Notes"),
            ("cover", Some("first.png"), "FIRST-BYTES"),
            ("cover", Some(".."), "SECOND-BYTES"),
            ("csrf_token", None, &csrf),
        ],
    );

    let response = post_multipart(&router, "/admin/docs/create", &csrf, "B", body).await;
    assert_eq!(
        response.status(),
        200,
        "a rejected name leaves the required field empty"
    );
    assert!(
        uploader.seen().is_empty(),
        "the discarded file part must not reach the uploader"
    );
    assert!(
        docs(&db).await.is_empty(),
        "a rejected name must not create the record"
    );
}

#[tokio::test]
async fn clearing_an_optional_upload_empties_the_stored_path() {
    let db = seeded_db().await;
    let router = router(db.clone(), Some(RecordingUploader::default()));
    let doc = seed_doc(&db, "Original", "cover.png", "spec.pdf").await;

    let csrf = new_csrf();
    let body = multipart_body(
        "B",
        &[
            ("title", None, "Original"),
            ("cover", Some(""), ""),
            ("attachment", Some(""), ""),
            // The framework's own control posts this.
            ("clear_attachment", None, "1"),
            ("csrf_token", None, &csrf),
        ],
    );
    let response = post_multipart(
        &router,
        &format!("/admin/docs/{}/edit", doc.id),
        &csrf,
        "B",
        body,
    )
    .await;
    assert_eq!(response.status(), 303, "clearing an optional upload saves");

    let updated = docs(&db).await;
    assert_eq!(updated[0].attachment, "", "the cleared field is emptied");
    assert_eq!(updated[0].cover, "cover.png", "the untouched one is kept");
}

#[tokio::test]
async fn clearing_a_required_upload_is_refused_inline() {
    let db = seeded_db().await;
    let router = router(db.clone(), Some(RecordingUploader::default()));
    let doc = seed_doc(&db, "Original", "cover.png", "spec.pdf").await;

    let csrf = new_csrf();
    let body = multipart_body(
        "B",
        &[
            ("title", None, "Original"),
            ("cover", Some(""), ""),
            ("attachment", Some(""), ""),
            ("clear_cover", None, "1"),
            ("csrf_token", None, &csrf),
        ],
    );
    let response = post_multipart(
        &router,
        &format!("/admin/docs/{}/edit", doc.id),
        &csrf,
        "B",
        body,
    )
    .await;
    assert_eq!(response.status(), 200, "the form re-renders with the error");
    let html = body_string(response).await;
    assert!(
        html.contains("Cover is required"),
        "a required upload refuses the clear inline: {html}"
    );
    assert_eq!(
        docs(&db).await[0].cover,
        "cover.png",
        "the refused clear writes nothing"
    );
}

#[tokio::test]
async fn a_refused_edit_upload_keeps_showing_the_stored_file() {
    let db = seeded_db().await;
    let router = router(db.clone(), Some(FailingUploader));
    let doc = seed_doc(&db, "Notes", "/uploads/old.png", "spec.pdf").await;

    let csrf = new_csrf();
    let body = multipart_body(
        "B",
        &[
            ("title", None, "Renamed"),
            ("cover", Some("new.png"), "NEW-BYTES"),
            ("attachment", Some(""), ""),
            ("csrf_token", None, &csrf),
        ],
    );
    let response = post_multipart(
        &router,
        &format!("/admin/docs/{}/edit", doc.id),
        &csrf,
        "B",
        body,
    )
    .await;
    assert_eq!(response.status(), 200, "the form re-renders");
    let html = body_string(response).await;
    assert!(
        html.contains("Cover could not be uploaded: this deployment has no room left"),
        "the reason must reach the field: {html}"
    );
    assert!(
        html.contains("data-file-current=\"/uploads/old.png\""),
        "the stored file is still there and must still be shown: {html}"
    );
    assert_eq!(
        docs(&db).await[0].cover,
        "/uploads/old.png",
        "a refused upload writes nothing"
    );
}

#[tokio::test]
async fn an_over_cap_body_still_413s_with_an_uploader_installed() {
    let db = seeded_db().await;
    let router = router(db.clone(), Some(RecordingUploader::default()));
    let csrf = new_csrf();
    let huge = "a".repeat(11 * 1024 * 1024);
    let body = multipart_body(
        "B",
        &[
            ("title", None, "Too big"),
            ("cover", Some("huge.png"), &huge),
            ("csrf_token", None, &csrf),
        ],
    );

    let response = post_multipart(&router, "/admin/docs/create", &csrf, "B", body).await;
    assert_eq!(
        response.status(),
        413,
        "an over-cap upload must 413 whether or not the bytes are buffered"
    );
    assert!(
        docs(&db).await.is_empty(),
        "an over-cap body must not create the record"
    );
}

#[tokio::test]
async fn the_edit_form_links_the_stored_files() {
    let db = seeded_db().await;
    let router = router(db.clone(), Some(RecordingUploader::default()));
    let doc = seed_doc(&db, "Notes", "/uploads/photo.png", "/files/spec.pdf").await;

    let response = get(&router, &format!("/admin/docs/{}/edit", doc.id)).await;
    assert!(response.status().is_success());
    let html = body_string(response).await;
    assert!(
        !html.contains("src=\"/uploads/photo.png\""),
        "no stored path is rendered as an image: {html}"
    );
    assert!(
        html.contains("href=\"/uploads/photo.png\""),
        "a stored image path is a link to the file: {html}"
    );
    assert!(
        html.contains("href=\"/files/spec.pdf\""),
        "a stored non-image path is a link to the file: {html}"
    );
    // Both stored values offer the clear control, labelled with what it does.
    assert!(html.contains("name=\"clear_cover\""), "{html}");
    assert!(html.contains("name=\"clear_attachment\""), "{html}");
    assert!(html.contains("Remove the current file"), "{html}");
}

#[tokio::test]
async fn a_create_form_offers_no_stored_value_and_no_clear_control() {
    let db = seeded_db().await;
    let router = router(db.clone(), Some(RecordingUploader::default()));

    let response = get(&router, "/admin/docs/create").await;
    assert!(response.status().is_success());
    let html = body_string(response).await;
    assert!(!html.contains("<img"), "{html}");
    assert!(!html.contains("data-file-current"), "{html}");
    assert!(!html.contains("name=\"clear_"), "{html}");
    assert!(
        html.contains("enctype=\"multipart/form-data\""),
        "a form with a file input posts multipart: {html}"
    );
}

#[tokio::test]
async fn serve_dir_serves_the_upload_directory_through_the_panel() {
    let db = seeded_db().await;
    let dir = temp_dir("serve");
    std::fs::write(dir.join("cat.png"), b"PNG-FILE").expect("write upload");

    let router = mount(
        db,
        Panel::new("admin")
            .auth(Auth::disabled())
            .serve_dir("/uploads/{*file}", dir.clone())
            .resource::<DocResource>(),
    )
    .expect("panel builds");

    let response = get(&router, "/uploads/cat.png").await;
    assert_eq!(response.status(), 200, "the stored path is fetchable");
    assert_eq!(body_bytes(response).await, b"PNG-FILE");

    // The directory route's own rules hold through the panel.
    let escaped = get(&router, "/uploads/%2e%2e/Cargo.toml").await;
    assert_eq!(
        escaped.status(),
        404,
        "a path out of the served directory is not served"
    );
    let missing = get(&router, "/uploads/absent.png").await;
    assert_eq!(missing.status(), 404);
}

#[tokio::test]
async fn a_served_directory_is_reachable_without_a_session() {
    let db = auth_seeded_db().await;
    let dir = temp_dir("serve-anonymous");
    std::fs::write(dir.join("cat.png"), b"PNG-FILE").expect("write upload");

    let router = mount(
        db,
        Panel::new("admin")
            // No `.auth(..)` call.
            .serve_dir("/uploads/{*file}", dir.clone())
            .resource::<DocResource>(),
    )
    .expect("panel builds");

    // The gate is live.
    let page = get(&router, "/admin/docs").await;
    assert_eq!(
        page.status(),
        307,
        "the panel must still gate anonymous page requests"
    );
    assert_eq!(
        page.headers().get(LOCATION).unwrap().to_str().unwrap(),
        "/admin/login?next=%2Fadmin%2Fdocs",
        "the anonymous panel page must be sent to login"
    );

    // The served directory is not behind that gate: no cookie, no session.
    let response = get(&router, "/uploads/cat.png").await;
    assert_eq!(
        response.status(),
        200,
        "a served file needs no session (ADR-0017)"
    );
    assert_eq!(body_bytes(response).await, b"PNG-FILE");
}

#[tokio::test]
async fn served_active_content_is_inert() {
    let db = seeded_db().await;
    let dir = temp_dir("serve-inert");
    std::fs::write(dir.join("cat.png"), b"PNG-FILE").expect("write upload");
    std::fs::write(
        dir.join("evil.svg"),
        br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#,
    )
    .expect("write upload");
    std::fs::write(dir.join("evil.html"), b"<p>x</p>").expect("write upload");

    let router = mount(
        db,
        Panel::new("admin")
            .auth(Auth::disabled())
            .serve_dir("/uploads/{*file}", dir.clone())
            .resource::<DocResource>(),
    )
    .expect("panel builds");

    let png = get(&router, "/uploads/cat.png").await;
    assert_eq!(png.status(), 200);
    assert_eq!(
        png.headers().get(X_CONTENT_TYPE_OPTIONS).unwrap(),
        "nosniff",
        "a served file is never sniffed"
    );
    assert_eq!(
        csp(&png),
        SERVED_FILE_POLICY,
        "a served file carries the fixed sandboxing policy"
    );
    assert!(
        png.headers().get(CONTENT_DISPOSITION).is_none(),
        "a raster image opens inline"
    );

    // A revalidated file is still hardened, and the missing `Content-Type` of a
    // 304 must not turn an inline image into a download.
    let last_modified = png
        .headers()
        .get(LAST_MODIFIED)
        .expect("a served file is dated")
        .to_str()
        .unwrap()
        .to_string();
    let revalidated = get_if_modified_since(&router, "/uploads/cat.png", &last_modified).await;
    assert_eq!(revalidated.status(), 304, "the file did not change");
    assert_eq!(
        revalidated.headers().get(X_CONTENT_TYPE_OPTIONS).unwrap(),
        "nosniff"
    );
    assert_eq!(csp(&revalidated), SERVED_FILE_POLICY);
    assert!(
        revalidated.headers().get(CONTENT_DISPOSITION).is_none(),
        "a 304 has nothing to download"
    );

    for path in ["/uploads/evil.svg", "/uploads/evil.html"] {
        let response = get(&router, path).await;
        assert_eq!(response.status(), 200, "{path} is served");
        assert_eq!(
            response.headers().get(X_CONTENT_TYPE_OPTIONS).unwrap(),
            "nosniff",
            "{path} is never sniffed"
        );
        assert_eq!(
            csp(&response),
            SERVED_FILE_POLICY,
            "{path} carries the fixed sandboxing policy"
        );
        let disposition = response
            .headers()
            .get(CONTENT_DISPOSITION)
            .unwrap_or_else(|| panic!("{path} must download"))
            .to_str()
            .unwrap();
        assert!(
            disposition.starts_with("attachment"),
            "{path} must download, got {disposition}"
        );
    }

    // The layer is scoped to the served path.
    let page = get(&router, "/admin/docs").await;
    assert_eq!(page.status(), 200);
    assert_eq!(
        csp(&page),
        "frame-ancestors 'self'",
        "a panel page keeps its own policy"
    );
}

#[tokio::test]
async fn a_serve_dir_path_without_a_catch_all_fails_the_build() {
    let db = seeded_db().await;
    let Err(error) = mount(
        db,
        Panel::new("admin")
            .serve_dir("/uploads", temp_dir("bad-path"))
            .resource::<DocResource>(),
    ) else {
        panic!("a serve_dir pattern with no catch-all must fail the build");
    };
    assert!(
        error.to_string().contains("catch-all"),
        "the error must name the fix: {error}"
    );
}
