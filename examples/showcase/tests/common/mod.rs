//! Shared fixtures and request scaffolding for the showcase integration tests.

#![allow(dead_code)]

pub mod routers;

use showcase::models::{DEMO_ADMIN_EMAIL, seed, seed_content, seed_staff};
pub use tablo::testing::{
    SESSION_COOKIE, TestClient, body_string, filter_options, form_body, input_value,
    multipart_body, response_cookies, session_cookie_value, set_cookie_header,
};
use tablo::{Panel, RouterBuilderPanelExt, TenantId, testing::rows};
use toasty::Db;
use topcoat::router::{Body, Router, RouterBuilderDiscoverExt};

pub fn mount(db: Db, panel: Panel) -> topcoat::Result<Router> {
    Ok(Router::builder()
        .discover()
        .app_context(db)
        .panel(panel)?
        .build())
}

/// Returns a fresh in-memory `Db` with the full showcase schema and no rows.
pub async fn empty_schema_db() -> Db {
    let db = Db::builder()
        .models(toasty::models!(
            showcase::models::User,
            showcase::models::Author,
            showcase::models::Post,
            showcase::models::Comment,
            showcase::models::MediaAsset,
            showcase::models::Staff,
            showcase::models::Workspace,
            showcase::models::Seat,
            tablo::auth::AuthSession
        ))
        .connect("sqlite::memory:")
        .await
        .expect("connect");
    db.push_schema().await.expect("push_schema");
    db
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
pub async fn tenanted_db() -> (Db, uuid::Uuid, uuid::Uuid) {
    let t1 = uuid::Uuid::from_u128(1);
    let t2 = uuid::Uuid::from_u128(2);
    let mut db = empty_schema_db().await;
    seed_staff(&mut db).await.expect("seed staff");
    let a1 = toasty::create!(showcase::models::Author {
        tenant_id: TenantId::from(t1),
        name: "Alice T1",
        email: "alice.t1@example.com",
    })
    .exec(&mut db)
    .await
    .expect("create author t1");
    let a2 = toasty::create!(showcase::models::Author {
        tenant_id: TenantId::from(t2),
        name: "Bob T2",
        email: "bob.t2@example.com",
    })
    .exec(&mut db)
    .await
    .expect("create author t2");
    let p1 = toasty::create!(showcase::models::Post {
        tenant_id: TenantId::from(t1),
        title: "T1 Post",
        body: "body",
        status: "published".to_string(),
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
        tenant_id: TenantId::from(t2),
        title: "T2 Post",
        body: "body",
        status: "draft".to_string(),
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

/// Logs in through `{prefix}/login` like a browser.
pub async fn login<'a>(router: &'a Router, email: &str, password: &str) -> TestClient<'a> {
    login_next(router, email, password, "").await.0
}

/// Mints a session cookie value for the seeded admin with `email`.
pub async fn mint_session(db: &Db, email: &str) -> String {
    use std::{fmt::Write as _, time::SystemTime};

    use showcase::models::Staff;
    use tablo::auth::{AuthSession, SESSION_LIFETIME};
    use topcoat::session::Token;

    let mut db = db.clone();
    let user = Staff::filter(Staff::fields().email().eq(email.to_string()))
        .first()
        .exec(&mut db)
        .await
        .expect("look up the seeded admin")
        .unwrap_or_else(|| panic!("the seed creates the admin {email}"));
    let token = Token::random();
    let mut token_hash = String::with_capacity(64);
    for byte in token.hash().iter() {
        write!(token_hash, "{byte:02x}").expect("writing to a String cannot fail");
    }
    toasty::create!(AuthSession {
        token_hash,
        user_id: user.id.to_string(),
        panel: "/admin".to_string(),
        tenant: None,
        expires_at: jiff::Timestamp::try_from(SystemTime::now() + SESSION_LIFETIME)
            .expect("a representable session expiry"),
        created_at: jiff::Timestamp::now(),
    })
    .exec(&mut db)
    .await
    .expect("mint the session row");
    token.encode()
}

/// A client holding a freshly minted session for `email`.
pub async fn signed_in_client<'a>(router: &'a Router, db: &Db, email: &str) -> TestClient<'a> {
    let token = mint_session(db, email).await;
    TestClient::new(router).cookie(SESSION_COOKIE, &token)
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

/// Logs in with an explicit `next` destination.
pub async fn login_next<'a>(
    router: &'a Router,
    email: &str,
    password: &str,
    next: &str,
) -> (TestClient<'a>, http::Response<Body>) {
    let page = TestClient::new(router).get("/admin/login").await;
    let cookies = response_cookies(&page);
    let html = body_string(page).await;
    let csrf = input_value(&html, "csrf_token")
        .unwrap_or_else(|| panic!("login page must embed a csrf_token input: {html}"));
    let body = form_body(&[
        ("email", email),
        ("password", password),
        ("next", next),
        ("csrf_token", &csrf),
    ]);
    let response = TestClient::new(router)
        .cookies(&cookies)
        .post_form("/admin/login", body)
        .await;
    let session = response_cookies(&response);
    (
        TestClient::new(router).cookies(&cookies).cookies(&session),
        response,
    )
}

/// Reads the record key `kind` from the first row action link.
pub fn row_link_key(html: &str, kind: &str) -> Option<String> {
    let needle = format!("{kind}=");
    let mut rest = html;
    while let Some(at) = rest.find(&needle) {
        let after = &rest[at + needle.len()..];
        let end = after.find(['&', '"', '\'']).unwrap_or(after.len());
        if end > 0 {
            return Some(after[..end].to_string());
        }
        rest = &rest[at + needle.len()..];
    }
    None
}

/// Finds the first `href` containing `needle`, decoded.
pub fn find_href_with(html: &str, needle: &str) -> Option<String> {
    let mut rest = html;
    loop {
        let start = rest.find("href=\"")?;
        rest = &rest[start + "href=\"".len()..];
        let end = rest.find('"')?;
        let href = &rest[..end];
        if href.contains(needle) {
            return Some(unescape_href(href));
        }
        rest = &rest[end..];
    }
}

/// Finds the pager link carrying `needle`.
pub fn find_pager_href(html: &str, needle: &str) -> Option<String> {
    let mut rest = html;
    loop {
        let start = rest.find("href=\"")?;
        rest = &rest[start + "href=\"".len()..];
        let end = rest.find('"')?;
        let href = &rest[..end];
        if href.contains(needle) && !href.contains("delete=") {
            return Some(unescape_href(href));
        }
        rest = &rest[end..];
    }
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

/// Reads the record key of every rendered row.
pub fn row_keys(html: &str) -> Vec<String> {
    rows(html)
        .into_iter()
        .filter_map(|row| row.select_value)
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
