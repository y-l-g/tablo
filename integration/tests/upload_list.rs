//! A file list over HTTP: the files a create stores, the rows an edit keeps, orders, removes and
//! adds to, the carry of a refused submission, the count and per-file rules, and the files a delete
//! lets go.

use std::path::{Path, PathBuf};

use tablo::{
    Allow, DeclarationErrorKind, FieldRuleFault, Resource, ResourceDef, Schema, Table, TextColumn,
    lens, testing::ErrorKind,
};
use toasty::Db;
use topcoat::router::Router;
use uuid::Uuid;

use crate::common::{
    TestClient, body_string, field_error, form_body, input_values, mount, panel, refusal, test_db,
};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Album {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    photos: Vec<String>,
}

#[derive(tablo::RecordForm)]
#[form(model = Album)]
struct AlbumForm {
    title: String,
    #[form(file)]
    photos: Vec<String>,
}

struct AlbumResource;

impl Resource for AlbumResource {
    type Model = Album;
    type Form = AlbumForm;

    fn declare() -> ResourceDef<Self> {
        let c = AlbumForm::controls();
        ResourceDef::new()
            .slug("albums")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Album.title))))
            .form(Schema::new((
                c.title,
                c.photos.extensions(["png"]).max_files(3),
            )))
    }
}

/// A panel storing `Album`'s uploads in a fresh directory, a client keeping one CSRF cookie
/// across its posts, as a browser does, and that directory.
async fn setup() -> (Router, Db, PathBuf) {
    let db = test_db(toasty::models!(Album)).await;
    let dir = std::env::temp_dir().join(format!("tablo-list-{}", Uuid::new_v4()));
    let router = mount(
        db.clone(),
        panel()
            .uploads_dir("/uploads", dir.clone())
            .resource::<AlbumResource>(),
    )
    .expect("panel builds");
    (router, db, dir)
}

fn browser(router: &Router) -> TestClient<'_> {
    TestClient::new(router).csrf(&Uuid::new_v4().to_string())
}

async fn albums(db: &Db) -> Vec<Album> {
    Album::all()
        .exec(&mut db.clone())
        .await
        .expect("query albums")
}

/// Whether the file a stored `/uploads/…` path names is on disk.
fn on_disk(dir: &Path, path: &str) -> bool {
    dir.join(path.strip_prefix("/uploads/").expect("an uploads path"))
        .is_file()
}

/// The basename each stored path was uploaded as, without the UUID the uploader adds.
fn uploaded_as(paths: &[String]) -> Vec<&str> {
    paths
        .iter()
        .map(|path| {
            path.rsplit_once('-')
                .map_or(path.as_str(), |(_, name)| name)
        })
        .collect()
}

/// The bare paths among a form's `keep_photos` values, one per row, without the seals a refused
/// submission carries its uploads under.
fn row_paths(values: &[String]) -> Vec<String> {
    values
        .iter()
        .filter(|value| value.starts_with('/'))
        .cloned()
        .collect()
}

/// Creates an album storing `files`, returning it.
async fn create(client: &TestClient<'_>, db: &Db, files: &[&str]) -> Album {
    let mut parts = vec![("title", None, "Sea")];
    parts.extend(files.iter().map(|name| ("photos", Some(*name), "PNG")));
    let response = client
        .submit_multipart("/admin/albums/-/create", &parts)
        .await;
    assert_eq!(response.status(), 303, "the create saves");
    albums(db).await.remove(0)
}

/// The rows the edit form of `album` keeps when it submits untouched.
async fn rows(client: &TestClient<'_>, album: &Album) -> Vec<String> {
    let html = body_string(
        client
            .get(&format!("/admin/albums/{}/edit", album.id))
            .await,
    )
    .await;
    input_values(&html, "keep_photos")
}

#[tokio::test]
async fn a_create_stores_each_chosen_file_in_order() {
    let (router, db, dir) = setup().await;
    let client = browser(&router);
    let album = create(&client, &db, &["a.png", "b.png"]).await;
    assert_eq!(uploaded_as(&album.photos), ["a.png", "b.png"]);
    assert!(album.photos.iter().all(|path| on_disk(&dir, path)));
    assert_eq!(
        rows(&client, &album).await.len(),
        2,
        "a row per stored file"
    );
}

#[tokio::test]
async fn an_edit_keeps_its_rows_drops_a_removed_one_and_adds_the_chosen_ones() {
    let (router, db, dir) = setup().await;
    let client = browser(&router);
    let album = create(&client, &db, &["a.png", "b.png"]).await;
    let rows = rows(&client, &album).await;
    let removed = album.photos[0].as_str();

    let mut parts: Vec<(&str, Option<&str>, &str)> = vec![("title", None, "Sea")];
    parts.extend(rows.iter().map(|row| ("keep_photos", None, row.as_str())));
    parts.push(("clear_photos", None, removed));
    parts.push(("photos", Some("c.png"), "PNG"));
    let response = client
        .submit_multipart(&format!("/admin/albums/{}/edit", album.id), &parts)
        .await;
    assert_eq!(response.status(), 303);
    let edited = albums(&db).await.remove(0);
    assert_eq!(edited.photos[0], album.photos[1]);
    assert_eq!(uploaded_as(&edited.photos), ["b.png", "c.png"]);
    assert!(!on_disk(&dir, removed), "the removed file is deleted");
    assert!(edited.photos.iter().all(|path| on_disk(&dir, path)));
}

/// The rows save in the order the form posts them, which is the order the browser shows them in
/// once a row moves; a new file goes after them.
#[tokio::test]
async fn an_edit_saves_the_rows_in_the_order_it_posts_them() {
    let (router, db, _dir) = setup().await;
    let client = browser(&router);
    let album = create(&client, &db, &["a.png", "b.png", "c.png"]).await;
    let rows = rows(&client, &album).await;
    let response = client
        .submit_multipart(
            &format!("/admin/albums/{}/edit", album.id),
            &[
                ("title", None, "Sea"),
                ("keep_photos", None, &rows[2]),
                ("keep_photos", None, &rows[0]),
                ("keep_photos", None, &rows[1]),
                ("clear_photos", None, &album.photos[1]),
                ("photos", Some("d.png"), "PNG"),
            ],
        )
        .await;
    assert_eq!(response.status(), 303);
    let edited = albums(&db).await.remove(0);
    assert_eq!(
        &edited.photos[..2],
        [album.photos[2].clone(), album.photos[0].clone()]
    );
    assert_eq!(uploaded_as(&edited.photos), ["c.png", "a.png", "d.png"]);
}

/// A list posts its input whether or not a file is chosen, so a submission posting neither the
/// input nor a row leaves the stored list alone.
#[tokio::test]
async fn an_edit_posting_nothing_for_the_list_keeps_it() {
    let (router, db, _dir) = setup().await;
    let client = browser(&router);
    let album = create(&client, &db, &["a.png"]).await;
    let response = client
        .submit(
            &format!("/admin/albums/{}/edit", album.id),
            &form_body(&[("title", "Lake")]),
        )
        .await;
    assert_eq!(response.status(), 303);
    let edited = albums(&db).await.remove(0);
    assert_eq!(
        (edited.title.as_str(), edited.photos),
        ("Lake", album.photos)
    );
}

/// What `client` posts for `album`'s list beside `extra`: each `keep_photos` the edit form renders,
/// the empty input, and `extra`.
async fn edit_list(
    client: &TestClient<'_>,
    album: &Album,
    keep: &[String],
    extra: &[(&str, Option<&str>, &str)],
) -> u16 {
    let mut parts: Vec<(&str, Option<&str>, &str)> = vec![("title", None, "Sea")];
    parts.extend(keep.iter().map(|row| ("keep_photos", None, row.as_str())));
    parts.extend_from_slice(extra);
    if !extra
        .iter()
        .any(|(name, file, _)| *name == "photos" && file.is_some())
    {
        parts.push(("photos", Some(""), ""));
    }
    client
        .submit_multipart(&format!("/admin/albums/{}/edit", album.id), &parts)
        .await
        .status()
        .as_u16()
}

async fn album_of(db: &Db, album: &Album) -> Album {
    Album::get_by_id(&mut db.clone(), &album.id)
        .await
        .expect("the album")
}

/// A `keep_photos` naming no file the record stores nor one this browser's refused submission
/// carries adds nothing: another record's bare path, a carry sealed for another browser, and a
/// seal with its path changed. Nor does it drop the stored files: only a remove checkbox does.
#[tokio::test]
async fn a_forged_row_adds_nothing_and_drops_nothing() {
    let (router, db, dir) = setup().await;
    let client = browser(&router);
    let other = browser(&router);
    let theirs = create(&other, &db, &["theirs.png"]).await;
    let ours = create(&client, &db, &["a.png"]).await;
    // Another browser's refused create carries its upload under a seal minted for that browser.
    let response = other
        .submit_multipart(
            "/admin/albums/-/create",
            &[("title", None, ""), ("photos", Some("x.png"), "PNG")],
        )
        .await;
    assert_eq!(response.status(), 200);
    let sealed: Vec<String> = input_values(&body_string(response).await, "keep_photos");
    let sealed = sealed.into_iter().find(|v| !v.starts_with('/')).unwrap();
    let (tag, _) = sealed.split_once('.').unwrap();
    for forged in [
        theirs.photos[0].clone(),
        sealed.clone(),
        format!("{tag}.{}", theirs.photos[0]),
    ] {
        assert_eq!(
            edit_list(&client, &ours, std::slice::from_ref(&forged), &[]).await,
            303
        );
        assert_eq!(album_of(&db, &ours).await.photos, ours.photos, "{forged}");
    }
    assert!(on_disk(&dir, &ours.photos[0]));
    assert!(on_disk(&dir, &theirs.photos[0]));
}

/// A form opened before another save added a file leaves that file in place.
#[tokio::test]
async fn an_edit_keeps_a_file_another_save_added_since_it_opened() {
    let (router, db, dir) = setup().await;
    let client = browser(&router);
    let album = create(&client, &db, &["a.png"]).await;
    let opened = rows(&client, &album).await;
    let other = browser(&router);
    let theirs = rows(&other, &album).await;
    assert_eq!(
        edit_list(&other, &album, &theirs, &[("photos", Some("b.png"), "PNG")]).await,
        303
    );
    assert_eq!(edit_list(&client, &album, &opened, &[]).await, 303);
    let edited = album_of(&db, &album).await;
    assert_eq!(uploaded_as(&edited.photos), ["a.png", "b.png"]);
    assert!(edited.photos.iter().all(|path| on_disk(&dir, path)));
}

/// Checking every row's remove checkbox empties the list and deletes its files.
#[tokio::test]
async fn removing_every_row_empties_the_list() {
    let (router, db, dir) = setup().await;
    let client = browser(&router);
    let album = create(&client, &db, &["a.png", "b.png"]).await;
    let rows = rows(&client, &album).await;
    let clears: Vec<(&str, Option<&str>, &str)> = album
        .photos
        .iter()
        .map(|path| ("clear_photos", None, path.as_str()))
        .collect();
    assert_eq!(edit_list(&client, &album, &rows, &clears).await, 303);
    assert!(album_of(&db, &album).await.photos.is_empty());
    assert!(album.photos.iter().all(|path| !on_disk(&dir, path)));
}

/// A refused submission carries its rows and the files it just stored, and the next submission
/// posting them back saves them.
#[tokio::test]
async fn a_refused_edit_carries_its_rows_and_new_files() {
    let (router, db, _dir) = setup().await;
    let client = browser(&router);
    let album = create(&client, &db, &["a.png"]).await;
    let edit = format!("/admin/albums/{}/edit", album.id);
    let mut parts: Vec<(&str, Option<&str>, &str)> = vec![("title", None, "")];
    let stored = rows(&client, &album).await;
    parts.extend(stored.iter().map(|row| ("keep_photos", None, row.as_str())));
    parts.push(("photos", Some("b.png"), "PNG"));
    let response = client.submit_multipart(&edit, &parts).await;
    assert_eq!(response.status(), 200, "a blank title re-renders the form");
    let carried = input_values(&body_string(response).await, "keep_photos");
    assert_eq!(
        albums(&db).await[0].photos,
        album.photos,
        "nothing is written"
    );

    assert_eq!(edit_list(&client, &album, &carried, &[]).await, 303);
    let edited = albums(&db).await.remove(0);
    assert_eq!(edited.photos[0], album.photos[0]);
    assert_eq!(uploaded_as(&edited.photos), ["a.png", "b.png"]);
}

/// Choosing more files than `max_files` is refused before any is stored.
#[tokio::test]
async fn choosing_more_files_than_the_most_stores_none() {
    let (router, db, dir) = setup().await;
    let client = browser(&router);
    let mut parts = vec![("title", None, "Sea")];
    for name in ["a.png", "b.png", "c.png", "d.png"] {
        parts.push(("photos", Some(name), "PNG"));
    }
    let response = client
        .submit_multipart("/admin/albums/-/create", &parts)
        .await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert_eq!(field_error(&html, "photos"), Some(ErrorKind::Invalid));
    assert!(input_values(&html, "keep_photos").is_empty());
    assert!(albums(&db).await.is_empty());
    assert!(
        !dir.exists()
            || std::fs::read_dir(&dir).unwrap().all(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with('.')
            })
    );
}

/// Kept and added files over `max_files` together are refused on the field; the files it stored
/// are carried, so removing a row saves the rest.
#[tokio::test]
async fn kept_and_added_files_over_the_most_are_refused_and_carried() {
    let (router, db, _dir) = setup().await;
    let client = browser(&router);
    let album = create(&client, &db, &["a.png", "b.png"]).await;
    let edit = format!("/admin/albums/{}/edit", album.id);
    let mut parts: Vec<(&str, Option<&str>, &str)> = vec![("title", None, "Sea")];
    let stored = rows(&client, &album).await;
    parts.extend(stored.iter().map(|row| ("keep_photos", None, row.as_str())));
    parts.push(("photos", Some("c.png"), "PNG"));
    parts.push(("photos", Some("d.png"), "PNG"));
    let response = client.submit_multipart(&edit, &parts).await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert_eq!(field_error(&html, "photos"), Some(ErrorKind::Invalid));
    let carried = input_values(&html, "keep_photos");

    let clear = [("clear_photos", None, album.photos[0].as_str())];
    assert_eq!(edit_list(&client, &album, &carried, &clear).await, 303);
    assert_eq!(
        uploaded_as(&album_of(&db, &album).await.photos),
        ["b.png", "c.png", "d.png"]
    );
}

/// A file whose name sanitizes to nothing, as a Windows device name does, errs on the field
/// rather than dropping the files chosen beside it.
#[tokio::test]
async fn a_file_named_like_a_device_errs_and_the_rest_carry() {
    let (router, db, _dir) = setup().await;
    let client = browser(&router);
    let response = client
        .submit_multipart(
            "/admin/albums/-/create",
            &[
                ("title", None, "Sea"),
                ("photos", Some("a.png"), "PNG"),
                ("photos", Some("aux.png"), "PNG"),
                ("photos", Some("b.png"), "PNG"),
            ],
        )
        .await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert_eq!(field_error(&html, "photos"), Some(ErrorKind::Invalid));
    let carried = input_values(&html, "keep_photos");
    assert_eq!(uploaded_as(&row_paths(&carried)), ["a.png", "b.png"]);
    assert!(albums(&db).await.is_empty());
}

/// The extensions apply to each file: a refused one is never stored and errs on the field, and
/// the others are carried.
#[tokio::test]
async fn a_refused_file_among_several_errs_and_the_rest_carry() {
    let (router, db, dir) = setup().await;
    let client = browser(&router);
    let response = client
        .submit_multipart(
            "/admin/albums/-/create",
            &[
                ("title", None, "Sea"),
                ("photos", Some("a.png"), "PNG"),
                ("photos", Some("page.html"), "HTML"),
            ],
        )
        .await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert_eq!(field_error(&html, "photos"), Some(ErrorKind::Invalid));
    assert_eq!(row_paths(&input_values(&html, "keep_photos")).len(), 1);
    let stored: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|entry| entry.unwrap().file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect();
    assert_eq!(stored.len(), 1, "only the taken file is stored: {stored:?}");
    assert!(stored[0].ends_with("-a.png"));
    assert!(albums(&db).await.is_empty());
}

#[tokio::test]
async fn a_delete_deletes_every_file_of_the_list() {
    let (router, db, dir) = setup().await;
    let client = browser(&router);
    let album = create(&client, &db, &["a.png", "b.png"]).await;
    let response = client
        .submit(&format!("/admin/albums/{}/delete", album.id), "confirm=1")
        .await;
    assert_eq!(response.status(), 303);
    assert!(albums(&db).await.is_empty());
    assert!(album.photos.iter().all(|path| !on_disk(&dir, path)));
}

#[tokio::test]
async fn max_files_on_a_single_file_refuses_the_mount() {
    #[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
    struct Cover {
        #[key]
        #[auto]
        id: Uuid,
        image: String,
    }

    #[derive(tablo::RecordForm)]
    #[form(model = Cover)]
    struct CoverForm {
        #[form(file)]
        image: String,
    }

    struct CoverResource;

    impl Resource for CoverResource {
        type Model = Cover;
        type Form = CoverForm;

        fn declare() -> ResourceDef<Self> {
            let c = CoverForm::controls();
            ResourceDef::new()
                .policy(Allow)
                .form(Schema::new(c.image.max_files(2)))
        }
    }

    let db = test_db(toasty::models!(Cover)).await;
    let kinds: Vec<_> = refusal(mount(
        db,
        panel()
            .uploads_dir("/uploads", std::env::temp_dir())
            .resource::<CoverResource>(),
    ))
    .into_iter()
    .map(|error| error.kind)
    .collect();
    let expected = DeclarationErrorKind::FieldRule {
        field: "image".to_string(),
        fault: FieldRuleFault::SingleFile,
    };
    assert!(kinds.contains(&expected), "{kinds:?}");
}
