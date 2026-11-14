//! A panel bound to one host with `Panel::host`: its pages, its sign-in, its runtime endpoint and
//! its served directory answer there and 404 on every other host of the same router.

use tablo::{
    Auth, DeclarationErrorKind, NoForm, Panel, Resource, ResourceDef, RouterBuilderPanelExt,
    auth::{AdminUser, AuthSession, hash_password},
    lens,
    testing::search_results,
};
use toasty::Db;
use topcoat::router::{Router, RouterBuilderDiscoverExt};
use uuid::Uuid;

use crate::common::{TestClient, body_string, mount, refusal, test_db};

const RESULTS_SHARD: &str = "/_topcoat/runtime/shards/tablo-search";

#[derive(Debug, Clone, toasty::Model, tablo::PanelModel)]
struct Book {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
}

struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = NoForm<Book>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("books")
            .policy(tablo::ReadOnly)
            .record_title(lens!(Book.title))
            .detail(tablo::Detail::new(tablo::TextColumn::new(lens!(
                Book.title
            ))))
    }
}

/// A panel of books without sign-in at `prefix`, bound to `host`.
fn open_panel(prefix: &str, host: &str) -> Panel {
    Panel::new(prefix)
        .auth(Auth::disabled())
        .host(host)
        .resource::<BookResource>()
}

async fn db_with_dune() -> (Db, Book) {
    let mut db = test_db(toasty::models!(Book, AdminUser, AuthSession)).await;
    let dune = toasty::create!(Book {
        title: "Dune".to_string(),
    })
    .exec(&mut db)
    .await
    .expect("seed the book");
    (db, dune)
}

async fn status(client: TestClient<'_>) -> u16 {
    client.get("/admin/books").await.status().as_u16()
}

/// The search shard's status and the URLs its results link, asked for `prefix`'s page.
async fn shard(client: &TestClient<'_>, prefix: &str) -> (u16, Vec<String>) {
    let args = format!(r#"{{"args":["{prefix}","dune"],"signals":{{}}}}"#);
    let response = client.call_procedure(RESULTS_SHARD, &args).await;
    let status = response.status().as_u16();
    let urls = search_results(&body_string(response).await)
        .into_iter()
        .flat_map(|group| group.results)
        .map(|result| result.href)
        .collect();
    (status, urls)
}

#[tokio::test]
async fn a_bound_panel_answers_on_its_host_alone() {
    let (db, _) = db_with_dune().await;
    let router = mount(db, open_panel("admin", "admin.test")).expect("the panel mounts");
    let client = TestClient::new(&router);

    assert_eq!(status(client.host("admin.test")).await, 200);
    // Neither the port nor the case of the name counts.
    assert_eq!(status(client.host("Admin.Test:3000")).await, 200);
    assert_eq!(status(client.host("app.test")).await, 404);
    assert_eq!(status(client.host("admin.test.evil.test")).await, 404);
    assert_eq!(
        status(client.clone()).await,
        404,
        "a request naming no host"
    );
}

/// The sign-in page is under the prefix: on another host nothing offers to start a session.
#[tokio::test]
async fn a_bound_panel_signs_no_one_in_on_another_host() {
    let (db, _) = db_with_dune().await;
    let panel = Panel::new("admin")
        .host("admin.test")
        .resource::<BookResource>();
    let router = mount(db, panel).expect("the panel mounts");
    let client = TestClient::new(&router);

    let here = client.host("admin.test").get("/admin/login").await;
    assert_eq!(here.status(), 200);
    let elsewhere = client.host("app.test").get("/admin/login").await;
    assert_eq!(elsewhere.status(), 404);
    let posted = client
        .host("app.test")
        .submit(
            "/admin/login",
            "email=ada%40example.com&password=opensesame",
        )
        .await;
    assert_eq!(posted.status(), 404);
}

/// A session of the panel, were its cookie ever sent to another host, opens nothing there: not a
/// page, and not the runtime endpoint the panel declares.
#[tokio::test]
async fn a_session_of_a_bound_panel_opens_nothing_on_another_host() {
    let (mut db, dune) = db_with_dune().await;
    let ada = toasty::create!(AdminUser {
        email: "ada@example.com".to_string(),
        password_hash: hash_password("opensesame").await.expect("hash"),
        display_name: "Ada".to_string(),
        active: true,
        created_at: jiff::Timestamp::now(),
    })
    .exec(&mut db)
    .await
    .expect("seed the admin");
    let panel = Panel::new("admin")
        .host("admin.test")
        .resource::<BookResource>();
    let router = mount(db.clone(), panel).expect("the panel mounts");
    let signed = TestClient::new(&router).sign_in(&db, "admin", &ada).await;

    let here = signed.host("admin.test");
    assert_eq!(here.get("/admin/books").await.status(), 200);
    assert_eq!(
        shard(&here, "/admin").await,
        (200, vec![format!("/admin/books/{}", dune.id)])
    );

    let elsewhere = signed.host("app.test");
    assert_eq!(elsewhere.get("/admin/books").await.status(), 404);
    assert_eq!(shard(&elsewhere, "/admin").await, (404, Vec::new()));
}

/// Each host sees its own panel alone, so the endpoint two panels declare runs as the panel of
/// the request's host, and never lists the other's records.
#[tokio::test]
async fn a_shared_endpoint_runs_as_the_panel_of_the_request_host() {
    let (db, dune) = db_with_dune().await;
    let mut builder = Router::builder().discover().with(db);
    builder
        .panel(open_panel("admin", "admin.test"))
        .expect("the first panel mounts");
    builder
        .panel(open_panel("app", "app.test"))
        .expect("the second panel mounts");
    let router = builder.build();
    let client = TestClient::new(&router);

    let app = client.host("app.test");
    assert_eq!(
        shard(&app, "/app").await,
        (200, vec![format!("/app/books/{}", dune.id)])
    );
    assert_eq!(shard(&app, "/admin").await, (200, Vec::new()));
    assert_eq!(app.get("/admin/books").await.status(), 404);
    assert_eq!(shard(&client.host("other.test"), "/app").await.0, 404);
}

#[tokio::test]
async fn a_bound_panel_serves_its_directory_on_its_host_alone() {
    let (db, _) = db_with_dune().await;
    let dir = std::env::temp_dir().join(format!("tablo-panel-host-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("create the directory");
    std::fs::write(dir.join("cat.png"), b"PNG-FILE").expect("write the file");
    let panel = open_panel("admin", "admin.test").serve_dir("/files/{*file}", dir);
    let router = mount(db, panel).expect("the panel mounts");
    let client = TestClient::new(&router);

    let here = client.host("admin.test").get("/files/cat.png").await;
    assert_eq!(here.status(), 200);
    let elsewhere = client.host("app.test").get("/files/cat.png").await;
    assert_eq!(elsewhere.status(), 404);
}

/// A host is a name alone: anything that could name another origin refuses the panel.
#[tokio::test]
async fn a_host_that_is_not_a_name_alone_refuses_the_panel() {
    for host in [
        "https://admin.test",
        "admin.test:3000",
        "admin.test/admin",
        "*.test",
        "",
    ] {
        let db = test_db(toasty::models!(Book)).await;
        let errors = refusal(mount(db, open_panel("admin", host)));
        assert!(
            errors.iter().any(|error| error.kind
                == DeclarationErrorKind::InvalidHost {
                    host: host.to_string()
                }),
            "{host}: {errors:?}"
        );
    }
}
