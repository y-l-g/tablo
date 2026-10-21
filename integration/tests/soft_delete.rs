//! A resource that soft-deletes: a delete stamps the column and hides the record, the trash
//! control lists it, Restore and Delete permanently act on it under the tenant filter, and the
//! edit page, the unique probe and the mount treat it as the declaration says.

use tablo::{
    Ability, Committed, DeclarationErrorKind, Mutation, Resource, ResourceDef, Schema, Site, Table,
    Tenancy, TenantId, TextColumn, Trashed, lens, scoped_query, scoped_query_in,
    testing::{ErrorKind, confirms_first, field_error, refusal, row_actions, row_keys},
};
use toasty::Db;
use topcoat::router::Router;
use uuid::Uuid;

use crate::common::{TestClient, body_string, mount, panel, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Post {
    #[key]
    #[auto]
    id: Uuid,
    tenant_id: TenantId,
    #[unique]
    title: String,
    deleted_at: Option<jiff::Timestamp>,
}

#[derive(tablo::RecordForm)]
#[form(model = Post)]
struct PostForm {
    title: String,
}

/// The committed mutations `after_commit` saw, in order.
static COMMITTED: std::sync::Mutex<Vec<(Mutation, usize)>> = std::sync::Mutex::new(Vec::new());

struct PostResource;

impl Resource for PostResource {
    type Model = Post;
    type Form = PostForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("posts")
            .policy_fn(|_cx, _ability: Ability<'_, Post>| true)
            .tenancy(Tenancy::column(Post::fields().tenant_id()))
            .soft_delete(lens!(Post.deleted_at))
            .table(Table::new(TextColumn::new(lens!(Post.title))))
            .form(Schema::new(PostForm::controls().title.unique()))
    }

    async fn after_commit(
        _cx: &topcoat::context::Cx,
        committed: Committed<Post>,
    ) -> topcoat::Result<()> {
        COMMITTED
            .lock()
            .unwrap()
            .push((committed.mutation(), committed.records().len()));
        Ok(())
    }
}

const ACME: Uuid = Uuid::from_u128(1);
const GLOBEX: Uuid = Uuid::from_u128(2);

async fn db() -> Db {
    test_db(toasty::models!(Post)).await
}

fn router(db: &Db) -> Router {
    mount(db.clone(), panel().resource::<PostResource>()).expect("panel builds")
}

async fn post(db: &Db, tenant: Uuid, title: &str) -> Post {
    toasty::create!(Post {
        tenant_id: TenantId::from(tenant),
        title: title.to_string(),
        deleted_at: None,
    })
    .exec(&mut db.clone())
    .await
    .unwrap()
}

async fn stored(db: &Db, id: Uuid) -> Option<Post> {
    Post::filter_by_id(id)
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
}

async fn listed(client: &TestClient<'_>, query: &str) -> Vec<String> {
    let html = body_string(client.get(&format!("/admin/posts{query}")).await).await;
    row_keys(&html)
}

/// A delete stamps the column and keeps the row; the list hides the record until the trash
/// control asks for it, and a restore brings it back.
#[tokio::test]
async fn a_delete_trashes_the_record_and_a_restore_brings_it_back() {
    let db = db().await;
    let router = router(&db);
    let client = TestClient::new(&router).tenant(ACME);
    let kept = post(&db, ACME, "kept").await;
    let gone = post(&db, ACME, "gone").await;
    let (kept_key, gone_key) = (kept.id.to_string(), gone.id.to_string());

    let response = client
        .submit(&format!("/admin/posts/{gone_key}/delete"), "confirm=1")
        .await;
    assert!(response.status().is_redirection(), "{}", response.status());
    let trashed = stored(&db, gone.id).await.expect("the row stays");
    assert!(trashed.deleted_at.is_some());

    assert_eq!(listed(&client, "").await, std::slice::from_ref(&kept_key));
    assert_eq!(
        listed(&client, "?f.trashed=only").await,
        std::slice::from_ref(&gone_key)
    );
    let mut both = listed(&client, "?f.trashed=with").await;
    both.sort();
    let mut expected = vec![kept_key.clone(), gone_key.clone()];
    expected.sort();
    assert_eq!(both, expected);

    // The trashed row offers Restore, which needs no confirmation, and Delete permanently, which
    // does; it is no longer edited or deleted.
    let html = body_string(client.get("/admin/posts?f.trashed=only").await).await;
    let back = "?return=%2Fadmin%2Fposts%3Ff.trashed%3Donly";
    let restore = format!("/admin/posts/{gone_key}/-/actions/restore");
    assert_eq!(
        confirms_first(&html, &format!("{restore}{back}")),
        Some(false),
        "{html}"
    );
    let force = format!("/admin/posts/{gone_key}/-/actions/force-delete{back}");
    assert_eq!(confirms_first(&html, &force), Some(true), "{html}");
    let actions = row_actions(&html, &gone_key).expect("the trashed row renders");
    assert_eq!((actions.edit, actions.delete_action), (None, None));
    assert_eq!(
        client
            .get(&format!("/admin/posts/{gone_key}"))
            .await
            .status(),
        200,
        "the trashed record's detail page opens"
    );
    assert_eq!(
        client
            .get(&format!("/admin/posts/{gone_key}/edit"))
            .await
            .status(),
        404,
        "a trashed record is restored before it is edited"
    );

    let response = client.submit(&restore, "").await;
    assert!(response.status().is_redirection(), "{}", response.status());
    assert!(stored(&db, gone.id).await.unwrap().deleted_at.is_none());
    assert_eq!(
        listed(&client, "?f.trashed=only").await,
        Vec::<String>::new()
    );
}

/// Delete permanently removes a trashed record's row, and refuses a live one.
#[tokio::test]
async fn a_force_delete_removes_a_trashed_row_only() {
    let db = db().await;
    let router = router(&db);
    let client = TestClient::new(&router).tenant(ACME);
    let live = post(&db, ACME, "live").await;
    let response = client
        .submit(
            &format!("/admin/posts/{}/-/actions/force-delete", live.id),
            "confirm=1",
        )
        .await;
    assert_eq!(response.status(), 404);
    assert!(stored(&db, live.id).await.is_some());

    client
        .submit(&format!("/admin/posts/{}/delete", live.id), "confirm=1")
        .await;
    let response = client
        .submit(
            &format!("/admin/posts/{}/-/actions/force-delete", live.id),
            "confirm=1",
        )
        .await;
    assert!(response.status().is_redirection(), "{}", response.status());
    assert!(stored(&db, live.id).await.is_none());
}

/// Restore and Delete permanently load through the tenant filter: another tenant's trashed record
/// is not found, and stays as it was.
#[tokio::test]
async fn another_tenants_trashed_record_is_not_found() {
    let db = db().await;
    let router = router(&db);
    let theirs = post(&db, GLOBEX, "theirs").await;
    TestClient::new(&router)
        .tenant(GLOBEX)
        .submit(&format!("/admin/posts/{}/delete", theirs.id), "confirm=1")
        .await;
    let client = TestClient::new(&router).tenant(ACME);
    for action in ["restore", "force-delete"] {
        let response = client
            .submit(
                &format!("/admin/posts/{}/-/actions/{action}", theirs.id),
                "confirm=1",
            )
            .await;
        assert_eq!(response.status(), 404, "{action}");
    }
    assert!(stored(&db, theirs.id).await.unwrap().deleted_at.is_some());
    assert_eq!(
        listed(&client, "?f.trashed=only").await,
        Vec::<String>::new()
    );
}

/// A bulk restore over a selection holding a live record restores the trashed one and skips the
/// live one; the hook names what was restored.
#[tokio::test]
async fn a_bulk_restore_skips_the_live_records() {
    let db = db().await;
    let router = router(&db);
    let client = TestClient::new(&router).tenant(ACME);
    let live = post(&db, ACME, "live").await;
    let trashed = post(&db, ACME, "trashed").await;
    client
        .submit(&format!("/admin/posts/{}/delete", trashed.id), "confirm=1")
        .await;
    COMMITTED.lock().unwrap().clear();
    let ids = format!("{},{}", live.id, trashed.id);
    let response = client
        .submit("/admin/posts/-/actions/restore", &format!("ids={ids}"))
        .await;
    assert!(response.status().is_redirection(), "{}", response.status());
    assert!(stored(&db, trashed.id).await.unwrap().deleted_at.is_none());
    assert!(stored(&db, live.id).await.unwrap().deleted_at.is_none());
    assert!(COMMITTED.lock().unwrap().contains(&(Mutation::Restore, 1)));
}

/// The list's bulk bar offers what the rows it shows take: Delete over the live records, Restore
/// over the trashed ones.
#[tokio::test]
async fn the_bulk_bar_follows_the_trash_control() {
    let db = db().await;
    let router = router(&db);
    let client = TestClient::new(&router).tenant(ACME);
    let live = body_string(client.get("/admin/posts").await).await;
    assert!(!live.contains("/-/actions/restore"), "{live}");
    let trash = body_string(client.get("/admin/posts?f.trashed=only").await).await;
    assert!(trash.contains("/admin/posts/-/actions/restore"), "{trash}");
    assert!(!trash.contains("/admin/posts/-/bulk-delete"), "{trash}");
}

/// A trashed record still holds its unique value: the probe sees it and re-renders the form
/// instead of the insert failing on the index.
#[tokio::test]
async fn the_unique_probe_sees_trashed_records() {
    let db = db().await;
    let router = router(&db);
    let client = TestClient::new(&router).tenant(ACME);
    let taken = post(&db, ACME, "taken").await;
    client
        .submit(&format!("/admin/posts/{}/delete", taken.id), "confirm=1")
        .await;
    let response = client.submit("/admin/posts/-/create", "title=taken").await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert_eq!(
        field_error(&html, "title"),
        Some(ErrorKind::Invalid),
        "{html}"
    );
}

/// `scoped_query` admits the live records and `scoped_query_in` the ones its scope names, each
/// under the tenant filter.
#[tokio::test]
async fn app_code_reads_trashed_records_through_scoped_query_in() {
    let db = db().await;
    let live = post(&db, ACME, "live").await;
    let trashed = post(&db, ACME, "trashed").await;
    post(&db, GLOBEX, "theirs").await;
    let router = router(&db);
    TestClient::new(&router)
        .tenant(ACME)
        .submit(&format!("/admin/posts/{}/delete", trashed.id), "confirm=1")
        .await;
    let handle = panel()
        .resource::<PostResource>()
        .handle(&db)
        .expect("panel builds");
    let cx = handle.context().with(tablo::Tenant(ACME));
    let ids = |posts: Vec<Post>| posts.into_iter().map(|post| post.id).collect::<Vec<_>>();
    let mut ex = db.clone();
    let query = scoped_query::<PostResource>(&cx).unwrap();
    assert_eq!(ids(query.exec(&mut ex).await.unwrap()), [live.id]);
    let query = scoped_query_in::<PostResource>(&cx, Trashed::Only).unwrap();
    assert_eq!(ids(query.exec(&mut ex).await.unwrap()), [trashed.id]);
    let query = scoped_query_in::<PostResource>(&cx, Trashed::With).unwrap();
    assert_eq!(query.exec(&mut ex).await.unwrap().len(), 2);
}

/// The mount refuses a record form that writes the soft-delete column.
#[tokio::test]
async fn a_form_writing_the_soft_delete_column_is_refused() {
    #[derive(tablo::RecordForm)]
    #[form(model = Post)]
    struct StampingForm {
        title: String,
        deleted_at: Option<jiff::Timestamp>,
    }

    struct Stamping;

    impl Resource for Stamping {
        type Model = Post;
        type Form = StampingForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .tenancy(Tenancy::column(Post::fields().tenant_id()))
                .soft_delete(lens!(Post.deleted_at))
        }
    }

    let errors = refusal(mount(db().await, panel().resource::<Stamping>()));
    assert!(
        errors.iter().any(|error| error.site == Site::Form
            && matches!(
                &error.kind,
                DeclarationErrorKind::FormClaimsSoftDeleteColumn { column, .. }
                    if column == "deleted_at"
            )),
        "{errors:?}"
    );
}
