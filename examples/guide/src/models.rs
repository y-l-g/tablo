//! The guide's demo world: one small blog plus the supporting models the
//! chapters reference.

#[derive(Debug, Clone, toasty::Model)]
pub struct User {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub name: String,
    pub email: String,
    pub role: String,
    pub age: i64,
    pub active: bool,
    pub created_at: jiff::Timestamp,
    pub sso_managed: bool,
}

// ANCHOR: role-options
#[derive(tablo::Options)]
pub enum Role {
    Admin,
    Member,
}
// ANCHOR_END: role-options

#[derive(Debug, Clone, Copy, PartialEq, Eq, tablo_core::Options)]
pub enum PostStatus {
    Draft,
    Published,
}

#[derive(Debug, Clone, toasty::Embed, tablo_core::EmbeddedForm)]
// ANCHOR: seo-struct
pub struct Seo {
    pub title: String,
    #[form(multiline = 3)]
    pub description: String,
}
// ANCHOR_END: seo-struct

#[derive(Debug, Clone, toasty::Model)]
pub struct Author {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub name: String,
}

#[derive(Debug, Clone, toasty::Model)]
pub struct Post {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub title: String,
    pub body: String,
    pub status: String,
    pub featured: bool,
    pub created_at: jiff::Timestamp,
    pub author_id: uuid::Uuid,
    #[belongs_to(key = author_id, references = id)]
    pub author: toasty::Deferred<Author>,
    pub seo: Seo,
    pub deleted_at: Option<jiff::Timestamp>,
}

#[derive(Debug, Clone, toasty::Model)]
pub struct Comment {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub post_id: uuid::Uuid,
    #[belongs_to(key = post_id, references = id)]
    pub post: toasty::Deferred<Post>,
    pub body: String,
}

#[derive(Debug, Clone, toasty::Model)]
pub struct Audit {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub action: String,
    pub created_at: String,
}

#[derive(Debug, Clone, toasty::Model)]
pub struct Theme {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub accent: String,
}

#[derive(Debug, Clone, toasty::Model)]
pub struct Order {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
}

#[derive(Debug, Clone)]
pub struct Staff {
    pub id: uuid::Uuid,
    pub name: String,
    pub active: bool,
    pub editor: bool,
    pub password_hash: String,
}

impl tablo_core::PanelUser for Staff {
    fn user_id(&self) -> String {
        self.id.to_string()
    }

    fn display_name(&self) -> &str {
        &self.name
    }

    fn can_access_panel(&self) -> bool {
        self.active
    }
}
