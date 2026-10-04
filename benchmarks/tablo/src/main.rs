use std::time::Instant;

use jiff::Timestamp;
use tablo_core::{
    Ability, ComputedColumn, Field, Panel, Policy, Resource, RouterBuilderPanelExt, Schema, Table,
    TablePage, TableState, Tenancy, Tenant, TextColumn, lens,
};
use toasty::{Db, Deferred};
use topcoat::{
    Result,
    context::{Cx, CxTestBuilder},
    router::{Body, Next, Router, RouterBuilderDiscoverExt, layer, response::Response},
    view::ViewExt,
};

#[derive(Debug, Clone, toasty::Model)]
pub struct Author {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    #[index]
    pub tenant_id: uuid::Uuid,
    pub name: String,
    #[unique]
    pub email: String,
    #[has_many]
    pub posts: Deferred<Vec<Post>>,
}

#[derive(Debug, Clone, toasty::Model)]
pub struct Post {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    #[index]
    pub tenant_id: uuid::Uuid,
    #[index]
    pub title: String,
    pub body: String,
    #[index]
    pub status: String,
    pub featured: bool,
    pub created_at: Timestamp,
    pub image_path: String,
    pub tags: String,
    #[index]
    pub author_id: uuid::Uuid,
    #[belongs_to(key = author_id, references = id)]
    pub author: Deferred<Author>,
    #[has_many]
    pub comments: Deferred<Vec<Comment>>,
}

#[derive(Debug, Clone, toasty::Model)]
pub struct Comment {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub body: String,
    #[index]
    pub post_id: uuid::Uuid,
    #[belongs_to(key = post_id, references = id)]
    pub post: Deferred<Post>,
}

pub struct AuthorResource;
impl Resource for AuthorResource {
    type Model = Author;
    type Form = tablo_core::NoForm<Self::Model>;
    fn tenancy() -> Tenancy<Author> {
        Tenancy::column(Author::fields().tenant_id())
    }
    fn table() -> Table<Author> {
        Table::new((
            TextColumn::new(lens!(Author.name)).searchable().sortable(),
            TextColumn::new(lens!(Author.email)),
        ))
        .paginate(2)
    }
}

pub struct PostResource;
impl Resource for PostResource {
    type Model = Post;
    type Form = PostForm;
    fn form(_dx: &tablo_core::DeclCx) -> Schema {
        Schema::new(Field::text(Post::fields().title()).required())
    }

    fn policy() -> impl Policy<Post> {
        |_cx: &Cx, ability: Ability<'_, Post>| !matches!(ability, Ability::Create)
    }
    fn tenancy() -> Tenancy<Post> {
        Tenancy::column(Post::fields().tenant_id())
    }
    fn table() -> Table<Post> {
        Table::new((
            TextColumn::new(lens!(Post.title)).searchable().sortable(),
            ComputedColumn::new("Author", |p: &Post| {
                if p.author.is_unloaded() {
                    "-".to_string()
                } else {
                    p.author.get().name.clone()
                }
            })
            .include(Post::fields().author()),
            ComputedColumn::new("Comments", |p: &Post| {
                if p.comments.is_unloaded() {
                    "0".to_string()
                } else {
                    p.comments.get().len().to_string()
                }
            })
            .include(Post::fields().comments()),
        ))
        .paginate(50)
    }
}

#[derive(tablo_core::RecordForm)]
#[form(model = Post)]
pub struct PostForm {
    pub title: String,
}

async fn seed_50(db: &mut Db, tenant: uuid::Uuid) {
    let mut author_ids = Vec::new();
    for i in 0..5 {
        let a = toasty::create!(Author {
            tenant_id: tenant,
            name: format!("Author {i}"),
            email: format!("author{i}@example.com"),
        })
        .exec(db)
        .await
        .expect("create author");
        author_ids.push(a.id);
    }
    for i in 0..50 {
        let aid = author_ids[i % author_ids.len()];
        let post = toasty::create!(Post {
            tenant_id: tenant,
            title: format!("Post {i:02}"),
            body: format!("Body {i}"),
            status: if i % 2 == 0 {
                "published".to_string()
            } else {
                "draft".to_string()
            },
            featured: i % 3 == 0,
            created_at: Timestamp::now(),
            image_path: "/images/x.jpg".to_string(),
            tags: "bench".to_string(),
            author_id: aid,
        })
        .exec(db)
        .await
        .expect("create post");
        toasty::create!(Comment {
            body: format!("Comment for {i}"),
            post_id: post.id,
        })
        .exec(db)
        .await
        .expect("create comment");
    }
}

fn bench_cx(db: &Db, tenant: uuid::Uuid) -> Cx {
    let parts = http::Request::builder()
        .uri("/admin/posts")
        .body(())
        .expect("bench request parts")
        .into_parts()
        .0;
    CxTestBuilder::new()
        .app_context(db.clone())
        .request_context(parts)
        .request_context(Tenant(tenant))
        .build()
}

fn summarize(mut times: Vec<f64>) -> (f64, f64, f64, f64, f64) {
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = times.len();
    (
        times[n / 2],
        times[(n as f64 * 0.9) as usize % n],
        times[(n as f64 * 0.99) as usize % n],
        times[0],
        times[n - 1],
    )
}

async fn bench_list_path(db: &Db, tenant: uuid::Uuid, iterations: usize) -> Vec<f64> {
    let mut times_ms = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let cx = bench_cx(db, tenant);
        // The list's gate: the shipped page answers 403 without a tenant and
        // `ViewAny`, so a bench that skipped them would not be the list path.
        assert!(
            tablo_core::can_list::<PostResource>(&cx),
            "the bench request must be allowed to open the list"
        );
        let start = Instant::now();
        let state = TableState::from_cx(&cx);
        let table = tablo_core::panel::wired_table::<PostResource>(&cx);
        assert_eq!(
            table.page_size(),
            50,
            "declared .paginate(50) must reach the loader"
        );
        let page = TablePage::load(
            &cx,
            &table,
            tablo_core::scoped_query::<PostResource>(&cx).expect("tenant scope"),
            &state,
        )
        .await
        .expect("table load");
        assert_eq!(page.rows.len(), 50, "expected first page of 50 rows");
        let html = table
            .render_with_state(&cx, page, &state, "/admin/posts")
            .await
            .expect("table render")
            .single()
            .await
            .expect("resolve view")
            .render(&cx);
        assert!(
            html.contains("Post 00") && html.contains("Post 49"),
            "rendered HTML must carry the 50 rows"
        );
        times_ms.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times_ms
}

async fn bench_query_only(db: &Db, tenant: uuid::Uuid, iterations: usize) -> Vec<f64> {
    let mut times_ms = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let cx = bench_cx(db, tenant);
        let start = Instant::now();
        let mut db2 = tablo_core::db::db(&cx);
        let inc_author: toasty::stmt::Include<Post, Author> = Post::fields().author().into();
        let inc_comments: toasty::stmt::Include<Post, toasty::stmt::List<Comment>> =
            Post::fields().comments().into();
        let rows: Vec<Post> = tablo_core::scoped_query::<PostResource>(&cx)
            .expect("tenant scope")
            .include(inc_author)
            .include(inc_comments)
            .exec(&mut db2)
            .await
            .expect("query");
        assert_eq!(rows.len(), 50, "expected 50 rows");
        // Touch includes to ensure they were loaded (no N+1)
        for p in &rows {
            let _ = p.author.get().name.clone();
            let _ = p.comments.get().len();
        }
        times_ms.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times_ms
}

async fn run_leg(db: Db, label: &str, iterations: usize) {
    let tenant = uuid::Uuid::new_v4();
    let mut seed_db = db.clone();
    seed_50(&mut seed_db, tenant).await;
    let (p50, p90, p99, min, max) = summarize(bench_list_path(&db, tenant, iterations).await);
    let (q50, _, _, _, _) = summarize(bench_query_only(&db, tenant, 20).await);
    println!("--- {label} ---");
    println!("tenant: {tenant} (fresh per run; 5 authors + 50 posts + 50 comments)");
    println!(
        "list path (from_cx -> load -> render_with_state -> HTML), {iterations} iters — p50: {p50:.2}ms p90: {p90:.2}ms p99: {p99:.2}ms min: {min:.2}ms max: {max:.2}ms"
    );
    println!("query-only diagnostic (cold Cx, no load/render), 20 iters — p50: {q50:.2}ms");
}

/// Refuses a Postgres URL whose database does not look disposable.
fn assert_bench_database(url: &str) {
    let dbname = url
        .rsplit_once('/')
        .map(|(_, tail)| tail.split('?').next().unwrap_or(tail))
        .unwrap_or("")
        .to_ascii_lowercase();
    assert!(
        dbname.contains("bench") || dbname.contains("test"),
        "refusing to reset Postgres database {dbname:?}: name a disposable bench/test database"
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn bench_database_guard_accepts_bench_names_and_refuses_the_rest() {
        for ok in [
            "postgresql://localhost/tablo_bench",
            "postgresql://u:p@host:5432/my-test-db?sslmode=require",
            "postgresql://localhost/BENCH",
        ] {
            super::assert_bench_database(ok);
        }
        for bad in [
            "postgresql://localhost/production",
            "postgresql://localhost/app",
            "postgresql://localhost/",
            "not-a-url",
        ] {
            let refused = std::panic::catch_unwind(|| super::assert_bench_database(bad));
            assert!(refused.is_err(), "{bad:?} must be refused");
        }
    }
}

async fn run_bench(iterations: usize) {
    println!("=== Tablo bench (GH #171, UNGATED): real list path ===");
    println!("workload: 50 rows, 2 includes (author + comments), tenancy set, policy enforced");
    println!(
        "path: TableState::from_cx -> TablePage::load (scoped_query + .paginate(50)) -> render_with_state -> HTML"
    );
    println!(
        "budget: <40ms p50 (50 rows, 2 includes) — reference only; UNGATED while GH #171 collects numbers, gating follows in a follow-up"
    );

    let sqlite = Db::builder()
        .models(toasty::models!(Author, Post, Comment))
        .connect("sqlite::memory:")
        .await
        .expect("connect sqlite");
    sqlite.push_schema().await.expect("push_schema sqlite");
    run_leg(sqlite, "sqlite (sqlite::memory:)", iterations).await;

    // Postgres leg runs when pointed at a server.
    // The URL must name a disposable bench database.
    match std::env::var("TABLO_BENCH_POSTGRES_URL") {
        Err(_) => println!(
            "--- postgres --- skipped (set TABLO_BENCH_POSTGRES_URL=postgresql://... to run)"
        ),
        Ok(url) => {
            // Refuses anything that does not look disposable.
            assert_bench_database(&url);
            // The bench database is disposable by contract: drop it first so
            // reruns start empty.
            let pg = Db::builder()
                .models(toasty::models!(Author, Post, Comment))
                .connect(&url)
                .await
                .expect("connect postgres");
            pg.reset_db().await.expect("reset_db postgres");
            drop(pg);
            let pg = Db::builder()
                .models(toasty::models!(Author, Post, Comment))
                .connect(&url)
                .await
                .expect("reconnect postgres");
            pg.push_schema().await.expect("push_schema postgres");
            run_leg(pg, "postgres (TABLO_BENCH_POSTGRES_URL)", iterations).await;
        }
    }
    println!("done (ungated — no PASS/FAIL; the p50 gate follows in a follow-up per GH #171)");
}

const SERVER_TENANT: uuid::Uuid = uuid::Uuid::nil();

/// Supplies the HTTP mode's tenant.
///
/// `enforce_tenant` refuses a tenant-scoped resource with 403 when the request
/// carries no `Tenant` scoped value, so the HTTP mode needs this layer to serve
/// `/admin`. The bench path scopes its own requests instead, through `bench_cx`.
#[layer("/admin")]
async fn inject_server_tenant(cx: &Cx, body: Body, next: Next<'_>) -> Result<Response> {
    next.run(&cx.with(Tenant(SERVER_TENANT)), body).await
}

fn router(db: Db) -> Router {
    Router::builder()
        .discover()
        .app_context(db)
        .panel(
            Panel::new("admin")
                .auth(tablo_core::Auth::disabled())
                .resource::<AuthorResource>()
                .resource::<PostResource>(),
        )
        .expect("panel mounts")
        .build()
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let is_bench = args.iter().any(|a| a == "--bench");
    if is_bench {
        let iterations = args
            .iter()
            .position(|a| a == "--iterations")
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(100);
        run_bench(iterations).await;
        return;
    }

    let mut db = Db::builder()
        .models(toasty::models!(Author, Post, Comment))
        .connect("sqlite::memory:")
        .await
        .expect("connect");
    db.push_schema().await.expect("push_schema");
    seed_50(&mut db, SERVER_TENANT).await;
    let router = router(db);
    println!("storefront-tablo listening on http://localhost:3000/ (try /admin/posts)");
    topcoat::start(router).await.unwrap();
}
