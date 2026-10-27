//! Shared fixtures and request scaffolding for the showcase integration tests.

pub mod routers;

use showcase::models::{DEMO_ADMIN_EMAIL, PostStatus, seed, seed_content, seed_staff};
pub use tablo::testing::{
    SESSION_COOKIE, TestClient, body_string, filter_options, find_href_with, form_body,
    input_value, mount, multipart_body, response_cookies, row_keys, session_cookie_value,
    set_cookie_header, test_db,
};
use tablo::{TenantId, testing::rows};
use toasty::Db;
use topcoat::router::{Body, Router};

/// A fresh `Db` with the full showcase schema and no rows, on the database [`test_db`] picks.
pub async fn empty_schema_db() -> Db {
    test_db(toasty::models!(
        showcase::models::User,
        showcase::models::Author,
        showcase::models::Post,
        showcase::models::Comment,
        showcase::models::Category,
        showcase::models::PostCategory,
        showcase::models::MediaAsset,
        showcase::models::Staff,
        showcase::models::Workspace,
        showcase::models::Seat,
        tablo::auth::AuthSession,
        tablo::auth::ColumnPreference,
        tablo::auth::AuthToken,
        tablo::auth::AuthFactor,
        tablo::notification::StoredNotification
    ))
    .await
}

/// [`empty_schema_db`] with the demo admin seeded and zero user rows.
pub async fn empty_users_db() -> Db {
    let mut db = empty_schema_db().await;
    seed_staff(&mut db).await.expect("seed staff");
    db
}

/// `Db` with the users seed applied.
pub async fn seeded_db() -> Db {
    let mut db = empty_schema_db().await;
    seed(&mut db).await.expect("seed");
    db
}

/// `Db` with the full seed (users, authors, posts, comments) and the
/// shipped auth models.
pub async fn full_db() -> Db {
    let mut db = empty_schema_db().await;
    seed(&mut db).await.expect("seed");
    seed_content(&mut db).await.expect("seed_content");
    db
}

/// `Db` with one author and post per tenant, for tenancy tests.
pub async fn tenanted_db() -> (Db, TenantId, TenantId) {
    let t1 = TenantId::new(uuid::Uuid::from_u128(1));
    let t2 = TenantId::new(uuid::Uuid::from_u128(2));
    let mut db = empty_schema_db().await;
    seed_staff(&mut db).await.expect("seed staff");
    let a1 = toasty::create!(showcase::models::Author {
        tenant_id: t1,
        name: "Alice T1",
        email: "alice.t1@example.com",
    })
    .exec(&mut db)
    .await
    .expect("create author t1");
    let a2 = toasty::create!(showcase::models::Author {
        tenant_id: t2,
        name: "Bob T2",
        email: "bob.t2@example.com",
    })
    .exec(&mut db)
    .await
    .expect("create author t2");
    let p1 = toasty::create!(showcase::models::Post {
        tenant_id: t1,
        title: "T1 Post",
        body: "body",
        status: showcase::models::PostStatus::Published,
        featured: true,
        created_at: "2024-01-15T09:30:00Z".parse::<jiff::Timestamp>().unwrap(),
        cover_id: None,
        tags: "t1".to_string(),
        seo: showcase::models::Seo {
            title: "T1 SEO".to_string(),
            description: String::new(),
        },
        publication: showcase::models::Publication::Published {
            published_at: Some("2024-01-15T09:30:00Z".parse::<jiff::Timestamp>().unwrap()),
            canonical_url: String::new(),
        },
        author_id: a1.id,
    })
    .exec(&mut db)
    .await
    .expect("create post t1");
    let p2 = toasty::create!(showcase::models::Post {
        tenant_id: t2,
        title: "T2 Post",
        body: "body",
        status: showcase::models::PostStatus::Draft,
        featured: false,
        created_at: "2024-06-01T12:00:00Z".parse::<jiff::Timestamp>().unwrap(),
        cover_id: None,
        tags: "t2".to_string(),
        seo: showcase::models::Seo {
            title: "T2 SEO".to_string(),
            description: String::new(),
        },
        publication: showcase::models::Publication::Scheduled {
            scheduled_at: Some("2024-07-01T09:00:00Z".parse::<jiff::Timestamp>().unwrap()),
            scheduled_for: None,
        },
        author_id: a2.id,
    })
    .exec(&mut db)
    .await
    .expect("create post t2");
    toasty::create!(showcase::models::Comment {
        body: "T1 comment",
        post_id: p1.id,
    })
    .exec(&mut db)
    .await
    .expect("create comment t1");
    toasty::create!(showcase::models::Comment {
        body: "T2 comment",
        post_id: p2.id,
    })
    .exec(&mut db)
    .await
    .expect("create comment t2");
    (db, t1, t2)
}

/// The seeded member of staff with `email`, as the panel signs them in.
async fn signed_staff(db: &Db, email: &str) -> showcase::staff::SignedStaff {
    use showcase::{models::Staff, staff::SignedStaff};

    let staff = Staff::filter(Staff::fields().email().eq(email.to_string()))
        .first()
        .exec(&mut db.clone())
        .await
        .expect("look up the seeded admin")
        .unwrap_or_else(|| panic!("the seed creates the admin {email}"));
    // Only `user_id` reaches the session row; each request loads the workspaces itself.
    SignedStaff {
        staff,
        workspaces: Vec::new(),
    }
}

/// Mints a session cookie value for the seeded admin with `email`.
pub async fn mint_session(db: &Db, email: &str) -> String {
    let user = signed_staff(db, email).await;
    tablo::auth::mint_session(&mut db.clone(), "admin", &user)
        .await
        .expect("mint the session row")
}

/// A client holding a freshly minted session for `email`.
pub async fn signed_in_client<'a>(router: &'a Router, db: &Db, email: &str) -> TestClient<'a> {
    let user = signed_staff(db, email).await;
    TestClient::new(router).sign_in(db, "admin", &user).await
}

/// A client holding a freshly minted session for the seeded demo admin.
pub async fn demo_client<'a>(router: &'a Router, db: &Db) -> TestClient<'a> {
    signed_in_client(router, db, DEMO_ADMIN_EMAIL).await
}

/// A client holding a freshly minted session for the tenantless admin, for the
/// tenancy fail-closed tests.
pub async fn tenantless_client<'a>(router: &'a Router, db: &Db) -> TestClient<'a> {
    signed_in_client(router, db, showcase::models::TENANTLESS_ADMIN_EMAIL).await
}

/// The decoded `href` of the pager's `rel` link, `"next"` or `"prev"`; `None` when the pager has
/// no such link.
pub fn pager_link(html: &str, rel: &str) -> Option<String> {
    let marker = format!("rel=\"{rel}\"");
    // Each anchor up to its `</a>`: the tag holds the click handler, whose `=>` is a raw `>`, so
    // the tag's own end is no boundary to search by, and its attributes come in no fixed order.
    html.match_indices("<a ").find_map(|(start, _)| {
        let anchor = &html[start..start + html[start..].find("</a>")?];
        anchor.contains(&marker).then_some(())?;
        let href = anchor.find("href=\"")? + "href=\"".len();
        let len = anchor[href..].find('"')?;
        Some(unescape_href(&anchor[href..href + len]))
    })
}

fn unescape_href(href: &str) -> String {
    href.replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Reads the row titles in document order.
pub fn row_titles(html: &str) -> Vec<String> {
    rows(html)
        .into_iter()
        .filter_map(|row| row.cells.into_iter().find(|cell| !cell.is_empty()))
        .collect()
}

/// Counts `Post` rows.
pub async fn post_count(db: &Db) -> usize {
    let mut db = db.clone();
    showcase::models::Post::all()
        .exec(&mut db)
        .await
        .unwrap()
        .len()
}

/// Counts `User` rows.
pub async fn user_count(db: &Db) -> usize {
    let mut db = db.clone();
    showcase::models::User::all()
        .exec(&mut db)
        .await
        .unwrap()
        .len()
}

/// Counts `Comment` rows.
pub async fn comment_count(db: &Db) -> usize {
    let mut db = db.clone();
    showcase::models::Comment::all()
        .exec(&mut db)
        .await
        .unwrap()
        .len()
}

/// A runtime (page re-run) POST, optionally carrying a session cookie.
///
/// Page re-runs are same-URL POSTs carrying Topcoat's runtime marker: the
/// runtime layer rewrites them into a GET for the page's own URL.
pub async fn runtime_post(
    router: &topcoat::router::Router,
    session: Option<&str>,
) -> http::Response<Body> {
    let mut request = http::Request::builder()
        .method(http::Method::POST)
        .uri("/admin/users")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(&topcoat::runtime::RUNTIME_HEADER, "true");
    if let Some(session) = session {
        request = request.header(http::header::COOKIE, format!("{SESSION_COOKIE}={session}"));
    }
    router.handle(request.body(Body::from("{}")).unwrap()).await
}

/// How an English page reads `status`: its derived label's fallback.
pub fn status_label(status: PostStatus) -> &'static str {
    match status {
        PostStatus::Draft => "Draft",
        PostStatus::Published => "Published",
    }
}
