//! The showcase's demo rows: the users the panel lists, the two blogs and the
//! staff who sign in to them, and the authors, posts and comments.

use jiff::Timestamp;
use tablo_core::{TenantId, auth::hash_password};
use toasty::Db;

use crate::{
    models::{Author, Comment, Post, Publication, Seo, User},
    staff::{create_staff, ensure_workspace},
};

/// Seeds the users the panel lists.
pub async fn seed(db: &mut Db) -> toasty::Result<()> {
    toasty::create!(User::[
        {
            name: "Ada Lovelace",
            email: "ada@example.com",
            role: "admin",
            active: true,
            age: 36,
            created_at: "2024-01-15T09:30:00Z"
                .parse::<Timestamp>()
                .expect("timestamp"),
        },
        {
            name: "Alan Turing",
            email: "alan@example.com",
            role: "member",
            active: false,
            age: 41,
            created_at: "2024-06-01T12:00:00Z"
                .parse::<Timestamp>()
                .expect("timestamp"),
        },
        {
            name: "Grace Hopper",
            email: "grace@example.com",
            role: "member",
            active: true,
            age: 78,
            created_at: "2023-11-20T18:45:00Z"
                .parse::<Timestamp>()
                .expect("timestamp"),
        },
        {
            name: "Claude Shannon",
            email: "claude@example.com",
            role: "member",
            active: true,
            age: 57,
            created_at: "2024-02-10T10:00:00Z"
                .parse::<Timestamp>()
                .expect("timestamp"),
        },
        {
            name: "Dorothy Vaughan",
            email: "dorothy@example.com",
            role: "member",
            active: true,
            age: 62,
            created_at: "2024-02-18T14:00:00Z"
                .parse::<Timestamp>()
                .expect("timestamp"),
        },
        {
            name: "Edsger Dijkstra",
            email: "edsger@example.com",
            role: "member",
            active: false,
            age: 51,
            created_at: "2024-03-05T09:00:00Z"
                .parse::<Timestamp>()
                .expect("timestamp"),
        },
        {
            name: "Frances Allen",
            email: "frances@example.com",
            role: "admin",
            active: true,
            age: 59,
            created_at: "2024-03-12T16:30:00Z"
                .parse::<Timestamp>()
                .expect("timestamp"),
        },
        {
            name: "Ken Thompson",
            email: "ken@example.com",
            role: "member",
            active: true,
            age: 82,
            created_at: "2024-04-02T11:15:00Z"
                .parse::<Timestamp>()
                .expect("timestamp"),
        },
    ])
    .exec(db)
    .await?;
    seed_staff(db).await
}

/// Seeds the two blogs and the staff who sign in.
pub async fn seed_staff(db: &mut Db) -> toasty::Result<()> {
    ensure_workspace(db, DEMO_TENANT, "Main Blog").await?;
    ensure_workspace(db, SIDE_TENANT, "Side Project").await?;
    create_staff(
        db,
        DEMO_ADMIN_EMAIL,
        "Demo Admin",
        DEMO_ADMIN_PASSWORD,
        &[DEMO_TENANT, SIDE_TENANT],
    )
    .await?;
    create_staff(
        db,
        TENANTLESS_ADMIN_EMAIL,
        "No Tenant",
        DEMO_ADMIN_PASSWORD,
        &[],
    )
    .await?;
    Ok(())
}

/// The tenant owning all showcase seed rows.
pub const DEMO_TENANT: uuid::Uuid = uuid::Uuid::from_u128(100);

/// A second, empty blog the demo admin also holds a seat in.
pub const SIDE_TENANT: uuid::Uuid = uuid::Uuid::from_u128(101);

/// Returns the deterministic id for the `index`th seeded post.
fn seeded_post_id(index: usize) -> uuid::Uuid {
    uuid::Uuid::from_u128(index as u128)
}

/// The first id handed to a pagination filler row.
const FILLER_ID_BASE: u128 = 0x8000_0000_0000_0000_0000_0000_0000_0000;

/// Backlog titles for the pagination fixture.
const PAGINATION_FILLER_TITLES: [&str; 60] = [
    "Cursor Pagination, Explained Slowly",
    "What We Learned From a 500-Row Admin Table",
    "Indexing the Columns Editors Actually Sort By",
    "A Draft Is Not a Todo",
    "Why Our Search Matches Substrings",
    "Escaping User Input in a LIKE Predicate",
    "The Case Against Infinite Scroll in Admin Tools",
    "Reading Query Plans Without Panicking",
    "Tenant Scoping Is a Query, Not a Filter",
    "One Round-Trip: Preloading Relations",
    "When a Cache Hides a Fresh Write",
    "Modelling Drafts, Scheduled Posts, and Retractions",
    "Why the Panel Owns Its Stylesheet",
    "Server-Rendered Forms Without a Client Framework",
    "Morphing a Table In Place",
    "Keeping Focus Across a Live Re-render",
    "The Cost of a Second Database Round-Trip",
    "Bulk Actions Need Confirmation, Not Optimism",
    "Naming Things in an Admin Panel",
    "Why Deletes Are Record Functions",
    "Authorization Belongs in the Handler",
    "Policy Checks Are Not Middleware",
    "What a Row Key Is For",
    "Display Keys Are Not Primary Keys",
    "Grouping Counts Are Page-Local",
    "Exporting a Filtered View",
    "RFC 4180 and the Humble CSV",
    "File Uploads Without a Bucket",
    "Storing a Filename Is Not Storing a File",
    "Required Fields and the Empty Submit",
    "Validation Errors Belong Next to the Field",
    "Accessible Forms for an Internal Tool",
    "Keyboard-First Admin Work",
    "Dark Mode Without a Flash of Wrong Theme",
    "Tokens Over Hard-Coded Colors",
    "Owning Your Primitives",
    "Syncing Components Without Forking Them",
    "A Sidebar That Remembers Itself",
    "Toasts That Do Not Steal Focus",
    "Empty States Are Not Errors",
    "Failed Loads Need a Retry",
    "Streaming a Skeleton Before the Rows",
    "Cursor Pagination Beats Offset at Scale",
    "Stable Sort Orders for Stable Pages",
    "What Happens When the Cursor Goes Stale",
    "Escaping the Search Term",
    "Trimming Before Validating",
    "Absent Keys Mean Unchanged",
    "Repeaters and Partial Groups",
    "Relationships in a Select",
    "Too Many Options to Load",
    "Searching Options on the Server",
    "Selecting a Variant",
    "Three Ways to Filter a List",
    "Filter State Lives in the URL",
    "Deep Links Into a Filtered Table",
    "Testing an Admin Panel Over HTTP",
    "Fixtures That Do Not Lie",
    "Benchmarking What Users Feel",
    "Writing Down the Decisions",
];
/// A tenant whose policy denies everything.
pub const BLOCKED_TENANT: uuid::Uuid = uuid::Uuid::from_u128(9999);

/// Demo administrator credentials.
pub const DEMO_ADMIN_EMAIL: &str = "admin@example.com";
pub const DEMO_ADMIN_PASSWORD: &str = "password";

/// A seeded administrator with no tenant.
pub const TENANTLESS_ADMIN_EMAIL: &str = "root@example.com";

/// The body of a removed comment.
pub const REMOVED_COMMENT_BODY: &str = "[removed]";

/// Returns the Argon2id PHC hash of a demo password, memoized per process.
pub(crate) fn memoized_password_hash(password: &str) -> String {
    use std::{
        collections::HashMap,
        sync::{Mutex, OnceLock},
    };

    static HASHES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    HASHES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("the demo hash cache is never poisoned")
        .entry(password.to_string())
        .or_insert_with(|| hash_password(password).expect("hash a demo password"))
        .clone()
}
/// Returns the embedded shapes a filler row carries.
fn filler_embedded() -> (Seo, Publication) {
    (
        Seo {
            title: String::new(),
            description: String::new(),
        },
        Publication::Scheduled {
            scheduled_at: Some("2024-07-01T09:00:00Z".parse::<Timestamp>().unwrap()),
            scheduled_for: None,
        },
    )
}

/// Seeds authors, posts, and comments.
pub async fn seed_content(db: &mut Db) -> toasty::Result<()> {
    if Author::all().exec(db).await?.is_empty() {
        let tenant = DEMO_TENANT;
        let ada_author = toasty::create!(Author {
            tenant_id: TenantId::from(tenant),
            name: "Ada Author",
            email: "ada.author@example.com",
        })
        .exec(db)
        .await?;
        let alan_author = toasty::create!(Author {
            tenant_id: TenantId::from(tenant),
            name: "Alan Author",
            email: "alan.author@example.com",
        })
        .exec(db)
        .await?;
        let june_writer = toasty::create!(Author {
            tenant_id: TenantId::from(tenant),
            name: "June Writer",
            email: "june.writer@example.com",
        })
        .exec(db)
        .await?;
        let rosa_editor = toasty::create!(Author {
            tenant_id: TenantId::from(tenant),
            name: "Rosa Editor",
            email: "rosa.editor@example.com",
        })
        .exec(db)
        .await?;
        toasty::create!(Post {
            id: seeded_post_id(0),
            tenant_id: TenantId::from(tenant),
            title: "Hello Toasty",
            body: "How we render admin tables over Toasty queries without an N+1.",
            status: "published".to_string(),
            featured: true,
            created_at: "2024-01-15T09:30:00Z".parse::<Timestamp>().unwrap(),
            cover_id: None,
            tags: "rust,async".to_string(),
            seo: Seo {
                title: "Hello Toasty — the admin panel".to_string(),
                description: "How we render admin tables over Toasty.".to_string(),
            },
            publication: Publication::Published {
                published_at: Some("2024-01-15T09:30:00Z".parse::<Timestamp>().unwrap()),
                canonical_url: "https://example.com/hello-toasty".to_string(),
            },
            author_id: ada_author.id,
        })
        .exec(db)
        .await?;
        toasty::create!(Post {
            id: seeded_post_id(1),
            tenant_id: TenantId::from(tenant),
            title: "Second Post",
            body: "Draft notes on cursor pagination edge cases.",
            status: "draft".to_string(),
            featured: false,
            created_at: "2024-06-01T12:00:00Z".parse::<Timestamp>().unwrap(),
            cover_id: None,
            tags: "draft".to_string(),
            seo: Seo {
                title: "Second Post".to_string(),
                description: "Draft notes on cursor pagination.".to_string(),
            },
            publication: Publication::Scheduled {
                scheduled_at: Some("2024-07-01T09:00:00Z".parse::<Timestamp>().unwrap()),
                scheduled_for: Some("2024-08-01T09:00:00Z".parse::<Timestamp>().unwrap()),
            },
            author_id: alan_author.id,
        })
        .exec(db)
        .await?;
        for (index, (title, body, tags, created_at, author_id)) in [
            (
                "Row-Level Caching Notes",
                "When the list cache helps and when it hides fresh writes.",
                "performance,caching",
                "2024-02-08T10:00:00Z",
                june_writer.id,
            ),
            (
                "Async Rust Patterns",
                "A field guide to the executors and channels we actually use.",
                "rust,async",
                "2024-02-20T15:30:00Z",
                rosa_editor.id,
            ),
            (
                "Reviewing Query Plans",
                "Reading EXPLAIN output before reaching for an index.",
                "database,sql",
                "2024-03-14T09:45:00Z",
                june_writer.id,
            ),
            (
                "Onboarding Runbook",
                "Checklist for bringing a new editor onto the panel.",
                "process,docs",
                "2024-04-09T13:20:00Z",
                rosa_editor.id,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let (seo, publication) = filler_embedded();
            toasty::create!(Post {
                id: seeded_post_id(index + 2),
                tenant_id: TenantId::from(tenant),
                title: title,
                body: body,
                status: "draft".to_string(),
                featured: false,
                created_at: created_at.parse::<Timestamp>().unwrap(),
                cover_id: None,
                tags: tags,
                seo: seo,
                publication: publication,
                author_id: author_id,
            })
            .exec(db)
            .await?;
        }
        for (index, title) in PAGINATION_FILLER_TITLES.iter().enumerate() {
            let created_at = jiff::civil::date(2024, 7, 1)
                .at(9, 0, 0, 0)
                .to_zoned(jiff::tz::TimeZone::UTC)
                .expect("a valid zoned time")
                .checked_add(jiff::Span::new().hours(index as i64 * 6))
                .expect("a representable timestamp")
                .timestamp();
            let author_id = match index % 4 {
                0 => ada_author.id,
                1 => alan_author.id,
                2 => june_writer.id,
                _ => rosa_editor.id,
            };
            let (seo, publication) = filler_embedded();
            toasty::create!(Post {
                id: uuid::Uuid::from_u128(FILLER_ID_BASE + index as u128),
                tenant_id: TenantId::from(tenant),
                title: *title,
                body: "Backlog draft kept for pagination coverage.",
                status: "draft".to_string(),
                featured: false,
                created_at: created_at,
                cover_id: None,
                tags: "backlog,draft".to_string(),
                seo: seo,
                publication: publication,
                author_id: author_id,
            })
            .exec(db)
            .await?;
        }
        let first_post = Post::filter(Post::fields().title().eq("Hello Toasty".to_string()))
            .first()
            .exec(db)
            .await?
            .expect("post");
        for body in [
            "Clear write-up — the include strategy finally clicked.",
            "Tried this on our staging data, pagination stayed stable.",
            "Small nit: the CSV export section deserves its own post.",
        ] {
            toasty::create!(Comment {
                body: body,
                post_id: first_post.id,
            })
            .exec(db)
            .await?;
        }
        toasty::create!(Comment {
            body: REMOVED_COMMENT_BODY,
            post_id: first_post.id,
        })
        .exec(db)
        .await?;
    }
    Ok(())
}
