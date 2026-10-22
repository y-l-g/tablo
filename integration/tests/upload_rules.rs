//! A file field's extensions and size over HTTP: the refusals before storing, the carried path
//! a field's extensions drop, and the declarations that refuse the mount.

use std::sync::{Arc, Mutex};

use tablo::{
    Allow, DeclarationErrorKind, FieldRuleFault, Resource, ResourceDef, Schema, Table, TextColumn,
    Uploader, lens,
};
use toasty::Db;
use topcoat::router::Router;
use uuid::Uuid;

use crate::common::{
    TestClient, body_string, control, field_error_message, get, mount, panel, refusal, test_db,
};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Photo {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    image: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Photo)]
struct PhotoForm {
    title: String,
    #[form(file)]
    image: String,
}

struct PhotoResource;

impl Resource for PhotoResource {
    type Model = Photo;
    type Form = PhotoForm;

    fn declare() -> ResourceDef<Self> {
        let c = PhotoForm::controls();
        let image = c.image.extensions([".PNG", "jpg"]).max_bytes(8);
        ResourceDef::new()
            .slug("photos")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Photo.title))))
            .form(Schema::new((c.title, image)))
    }
}

/// An uploader that records each name it stores, and holds every path under `/uploads/`.
#[derive(Clone, Default)]
struct HoldingUploader {
    stored: Arc<Mutex<Vec<String>>>,
}

impl Uploader for HoldingUploader {
    async fn store(&self, filename: &str, _bytes: &[u8]) -> Result<String, String> {
        self.stored.lock().expect("lock").push(filename.to_string());
        Ok(format!("/uploads/{filename}"))
    }

    async fn holds(&self, path: &str) -> bool {
        path.starts_with("/uploads/")
    }
}

async fn photos() -> (Db, Router, HoldingUploader) {
    let db = test_db(toasty::models!(Photo)).await;
    let uploader = HoldingUploader::default();
    let router = mount(
        db.clone(),
        panel()
            .uploads(uploader.clone())
            .resource::<PhotoResource>(),
    )
    .expect("panel builds");
    (db, router, uploader)
}

async fn photo_count(db: &Db) -> usize {
    Photo::all().exec(&mut db.clone()).await.unwrap().len()
}

/// A file of an extension the field does not take, or larger than its most bytes, is refused on
/// the field and never reaches the uploader; a taken one, of any case, is stored.
#[tokio::test]
async fn a_refused_file_is_never_stored() {
    let (db, router, uploader) = photos().await;
    let client = TestClient::new(&router);
    let html = body_string(get(&router, "/admin/photos/-/create").await).await;
    assert!(
        control(&html, "image").contains("accept=\".png,.jpg\""),
        "the picker is hinted the extensions: {html}"
    );
    for (filename, bytes, words) in [
        (
            "page.html",
            "PNG",
            "Image must be a file ending in .png or .jpg",
        ),
        ("png", "PNG", "Image must be a file ending in .png or .jpg"),
        (
            "shot.png.html",
            "PNG",
            "Image must be a file ending in .png or .jpg",
        ),
        ("shot.png", "123456789", "Image must be at most 8 bytes"),
    ] {
        let response = client
            .submit_multipart(
                "/admin/photos/-/create",
                &[("title", None, "Sea"), ("image", Some(filename), bytes)],
            )
            .await;
        assert_eq!(response.status(), 200, "{filename}");
        let html = body_string(response).await;
        assert_eq!(
            field_error_message(&html, "image").as_deref(),
            Some(words),
            "{filename}"
        );
    }
    assert!(uploader.stored.lock().unwrap().is_empty());
    assert_eq!(photo_count(&db).await, 0);

    let response = client
        .submit_multipart(
            "/admin/photos/-/create",
            &[
                ("title", None, "Sea"),
                ("image", Some("Shot.JPG"), "12345678"),
            ],
        )
        .await;
    assert_eq!(response.status(), 303);
    assert_eq!(*uploader.stored.lock().unwrap(), ["Shot.JPG"]);
}

/// A carried path whose extension the field does not take is dropped, though the uploader holds
/// it, so a forged carry cannot point the record at another field's file; a taken one is kept.
#[tokio::test]
async fn a_carried_path_the_extensions_refuse_is_dropped() {
    let (db, router, _) = photos().await;
    let client = TestClient::new(&router);
    for forged in ["/uploads/page.html", "/uploads/page.html?x.png"] {
        let body = format!("title=Sea&keep_image={}", forged.replace('?', "%3F"));
        let response = client.submit("/admin/photos/-/create", &body).await;
        assert_eq!(response.status(), 200, "{forged}");
    }
    assert_eq!(photo_count(&db).await, 0);
    let response = client
        .submit(
            "/admin/photos/-/create",
            "title=Sea&keep_image=/uploads/a.png",
        )
        .await;
    assert_eq!(response.status(), 303);
    let stored = Photo::all().exec(&mut db.clone()).await.unwrap();
    assert_eq!(stored[0].image, "/uploads/a.png");
}

/// An empty extension list, an extension no file name ends in, and a size the form-body cap
/// refuses first, each refuse the mount.
#[tokio::test]
async fn a_misdeclared_file_rule_refuses_the_mount() {
    struct Misdeclared;

    impl Resource for Misdeclared {
        type Model = Photo;
        type Form = PhotoForm;

        fn declare() -> ResourceDef<Self> {
            let c = PhotoForm::controls();
            ResourceDef::new()
                .slug("photos")
                .policy(Allow)
                .form(Schema::new((
                    c.title,
                    c.image
                        .extensions(["tar.gz", "png"])
                        .max_bytes(10 * 1024 * 1024),
                )))
        }
    }

    struct Empty;

    impl Resource for Empty {
        type Model = Photo;
        type Form = PhotoForm;

        fn declare() -> ResourceDef<Self> {
            let c = PhotoForm::controls();
            let none: [&str; 0] = [];
            ResourceDef::new()
                .slug("empty")
                .policy(Allow)
                .form(Schema::new((c.title, c.image.extensions(none))))
        }
    }

    let db = test_db(toasty::models!(Photo)).await;
    let errors = refusal(mount(
        db,
        panel()
            .uploads(HoldingUploader::default())
            .resource::<Misdeclared>()
            .resource::<Empty>(),
    ));
    let kinds: Vec<_> = errors.into_iter().map(|error| error.kind).collect();
    let rule = |fault| DeclarationErrorKind::FieldRule {
        field: "image".to_string(),
        fault,
    };
    for expected in [
        rule(FieldRuleFault::InvalidExtension("tar.gz".to_string())),
        rule(FieldRuleFault::AboveFormCap(10 * 1024 * 1024)),
        rule(FieldRuleFault::NoExtension),
    ] {
        assert!(kinds.contains(&expected), "{expected:?} in {kinds:?}");
    }
}
