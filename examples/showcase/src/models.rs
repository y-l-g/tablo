use jiff::Timestamp;
use tablo_core::TenantId;
use toasty::Deferred;

pub use crate::{
    seed::{
        BLOCKED_TENANT, DEMO_ADMIN_EMAIL, DEMO_ADMIN_PASSWORD, DEMO_TENANT, REMOVED_COMMENT_BODY,
        SIDE_TENANT, TENANTLESS_ADMIN_EMAIL, seed, seed_content, seed_staff,
    },
    staff::{Seat, SignedStaff, Staff, Workspace, create_staff},
};

/// A user's role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, tablo_core::Options)]
pub enum Role {
    Admin,
    Member,
}

/// A post's lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, tablo_core::Options)]
pub enum PostStatus {
    Draft,
    Published,
}

#[derive(Debug, Clone, toasty::Model)]
pub struct User {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    #[index]
    pub name: String,
    #[unique]
    pub email: String,
    pub role: String,
    pub active: bool,
    pub age: i64,
    #[default(jiff::Timestamp::now())]
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, toasty::Model)]
// Scoped unique index avoids a 500 on cross-tenant emails.
#[unique(tenant_id, email)]
pub struct Author {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    #[index]
    pub tenant_id: TenantId,
    pub name: String,
    pub email: String,
    #[has_many]
    pub posts: Deferred<Vec<Post>>,
}

/// SEO metadata for a post.
#[derive(Debug, Clone, toasty::Embed, tablo_core::EmbeddedForm)]
pub struct Seo {
    pub title: String,
    #[form(multiline = 3)]
    pub description: String,
}

/// A post's lifecycle.
#[derive(Debug, Clone, PartialEq, toasty::Embed, tablo_core::EmbeddedForm)]
pub enum Publication {
    #[column(variant = 1)]
    Scheduled {
        #[shared(timestamp)]
        #[form(label = "Publication timestamp")]
        scheduled_at: Option<Timestamp>,
        scheduled_for: Option<Timestamp>,
    },
    #[column(variant = 2)]
    Published {
        #[shared(timestamp)]
        published_at: Option<Timestamp>,
        #[form(label = "Canonical URL")]
        canonical_url: String,
    },
    #[column(variant = 3)]
    Archived {
        #[shared(timestamp)]
        archived_at: Option<Timestamp>,
        #[form(label = "Archive reason")]
        reason: String,
    },
}

#[derive(Debug, Clone, toasty::Model)]
pub struct Post {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    #[index]
    pub tenant_id: TenantId,
    #[index]
    pub title: String,
    pub body: String,
    #[index]
    pub status: String,
    pub featured: bool,
    #[default(jiff::Timestamp::now())]
    pub created_at: Timestamp,
    pub cover_id: Option<uuid::Uuid>,
    pub tags: String,
    pub seo: Seo,
    pub publication: Publication,
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

/// One stored file in the media library.
#[derive(Debug, Clone, toasty::Model)]
#[table = "medias"]
pub struct MediaAsset {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    #[index]
    pub tenant_id: TenantId,
    pub path: String,
    pub filename: String,
    pub kind: String,
    pub created_at: Timestamp,
}
