//! What becomes of a stored upload once a committed write lets it go: an edit that replaces or
//! clears it, and a delete or force delete that removes its record, delete the file; a refused
//! write, a soft delete and a file the uploader does not hold keep it.

use std::path::{Path, PathBuf};

use tablo::{Ability, Resource, ResourceDef, Table, TextColumn, lens};
use toasty::Db;
use topcoat::router::Router;
use uuid::Uuid;

use crate::common::{TestClient, mount, panel, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Sheet {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    cover: String,
    attachment: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Sheet)]
struct SheetForm {
    title: String,
    #[form(file)]
    cover: String,
    #[form(file, optional)]
    attachment: String,
}

struct SheetResource;

impl Resource for SheetResource {
    type Model = Sheet;
    type Form = SheetForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy_fn(|_cx, ability| !matches!(ability, Ability::ForceDelete(_)))
            .table(Table::new(TextColumn::new(lens!(Sheet.title))))
    }
}

/// A panel storing `Sheet`'s uploads in a fresh directory, and that directory.
async fn setup() -> (Router, Db, PathBuf) {
    let db = test_db(toasty::models!(Sheet)).await;
    let dir = std::env::temp_dir().join(format!("tablo-discard-{}", Uuid::new_v4()));
    let router = mount(
        db.clone(),
        panel()
            .uploads_dir("/uploads", dir.clone())
            .resource::<SheetResource>(),
    )
    .expect("panel builds");
    (router, db, dir)
}

async fn sheets(db: &Db) -> Vec<Sheet> {
    let mut db = db.clone();
    Sheet::all().exec(&mut db).await.expect("query sheets")
}

/// Whether the file a stored `/uploads/…` path names is on disk.
fn on_disk(dir: &Path, path: &str) -> bool {
    dir.join(path.strip_prefix("/uploads/").expect("an uploads path"))
        .is_file()
}

async fn create(router: &Router, db: &Db) -> Sheet {
    let response = TestClient::new(router)
        .submit_multipart(
            "/admin/sheets/-/create",
            &[
                ("title", None, "Notes"),
                ("cover", Some("cover.png"), "COVER"),
                ("attachment", Some("spec.pdf"), "SPEC"),
            ],
        )
        .await;
    assert_eq!(response.status(), 303, "the create saves");
    sheets(db).await.remove(0)
}

#[tokio::test]
async fn an_edit_deletes_the_file_it_replaces_or_clears() {
    let (router, db, dir) = setup().await;
    let sheet = create(&router, &db).await;
    assert!(on_disk(&dir, &sheet.cover) && on_disk(&dir, &sheet.attachment));

    let response = TestClient::new(&router)
        .submit_multipart(
            &format!("/admin/sheets/{}/edit", sheet.id),
            &[
                ("title", None, "Notes"),
                ("cover", Some("new.png"), "NEW"),
                ("attachment", Some(""), ""),
                ("clear_attachment", None, "1"),
            ],
        )
        .await;
    assert_eq!(response.status(), 303);
    let edited = sheets(&db).await.remove(0);
    assert!(on_disk(&dir, &edited.cover), "the new file stays");
    assert!(!on_disk(&dir, &sheet.cover), "the replaced file is deleted");
    assert!(
        !on_disk(&dir, &sheet.attachment),
        "the cleared file is deleted"
    );
}

#[tokio::test]
async fn a_refused_edit_keeps_the_stored_files() {
    let (router, db, dir) = setup().await;
    let sheet = create(&router, &db).await;

    let response = TestClient::new(&router)
        .submit_multipart(
            &format!("/admin/sheets/{}/edit", sheet.id),
            &[
                ("title", None, ""),
                ("cover", Some(""), ""),
                ("attachment", Some(""), ""),
                ("clear_attachment", None, "1"),
            ],
        )
        .await;
    assert_eq!(response.status(), 200, "a blank title re-renders the form");
    assert_eq!(
        sheets(&db).await[0].attachment,
        sheet.attachment,
        "nothing is written"
    );
    assert!(on_disk(&dir, &sheet.cover) && on_disk(&dir, &sheet.attachment));
}

#[tokio::test]
async fn a_delete_deletes_the_records_files_and_leaves_others() {
    let (router, db, dir) = setup().await;
    let sheet = create(&router, &db).await;
    // A file the uploader did not name: a path an app wrote by hand.
    std::fs::write(dir.join("hand-written.png"), b"KEEP").unwrap();
    let mut handle = db.clone();
    let other = toasty::create!(Sheet {
        title: "Other".to_string(),
        cover: "/uploads/hand-written.png".to_string(),
        attachment: String::new(),
    })
    .exec(&mut handle)
    .await
    .unwrap();

    for id in [sheet.id, other.id] {
        let response = TestClient::new(&router)
            .submit(&format!("/admin/sheets/{id}/delete"), "confirm=1")
            .await;
        assert_eq!(response.status(), 303, "the delete commits");
    }
    assert!(sheets(&db).await.is_empty());
    assert!(!on_disk(&dir, &sheet.cover) && !on_disk(&dir, &sheet.attachment));
    assert!(
        on_disk(&dir, "/uploads/hand-written.png"),
        "a file the uploader does not hold is never deleted"
    );
}
