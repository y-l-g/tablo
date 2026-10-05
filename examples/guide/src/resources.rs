//! The guide's canonical resources. Each method is anchored into its chapter;
//! required items the guide elides sit outside the anchors.

use tablo::prelude::*;
use toasty::stmt::{Include, List, Query};
use topcoat::{Result, context::Cx};

use crate::{
    models::{Audit, Author, Comment, Order, Post, Role, User},
    policy_tenancy::editors_only,
    tables::Publish,
};

fn is_admin(_cx: &Cx) -> bool {
    false
}

pub struct UserResource;

impl Resource for UserResource {
    type Model = User;
    type Form = UserForm;

    fn declare() -> ResourceDef<Self> {
        let c = UserForm::controls();
        ResourceDef::new()
            // ANCHOR: user-table
            .table(
                Table::new((
                    TextColumn::new(lens!(User.name)).searchable().sortable(),
                    TextColumn::new(lens!(User.email)).searchable(),
                    TextColumn::new(lens!(User.age)).sortable(),
                    BooleanColumn::new(lens!(User.active)),
                ))
                .paginate(20),
            )
            // ANCHOR_END: user-table
            // ANCHOR: user-policy
            .policy(|cx: &Cx, ability: Ability<'_, User>| match ability {
                Ability::ViewAny | Ability::View(_) => true,
                Ability::Create | Ability::DeleteAny | Ability::Delete(_) => is_admin(cx),
                Ability::Update(user) => !user.sso_managed,
            })
            // ANCHOR_END: user-policy
            // ANCHOR: user-form
            .form(Schema::new(Section::new("Profile").schema((
                c.name.placeholder("Ada Lovelace"),
                c.email.email(),
                c.role.optional(),
                c.age.optional(),
            ))))
            // ANCHOR_END: user-form
            // ANCHOR: user-navigation
            .navigation_order(-1)
            .icon(tablo_ui::icons::USERS)
        // ANCHOR_END: user-navigation
    }

    // ANCHOR: user-validate
    fn validate_record(_cx: &Cx, form: &UserForm) -> FieldErrors {
        let mut errors = FieldErrors::new();
        if form.age < 0 {
            errors.add("age", "Age must be zero or more");
        }
        errors
    }
    // ANCHOR_END: user-validate
}

// ANCHOR: user-record-form
#[derive(Debug, Clone, tablo_core::RecordForm)]
#[form(model = User)]
pub struct UserForm {
    pub name: String,
    pub email: String,
    #[form(options = Role, blank = Role::Member.value())]
    pub role: String,
    #[form(blank = 0)]
    pub age: i64,
}
// ANCHOR_END: user-record-form

pub struct PostResource;

impl Resource for PostResource {
    type Model = Post;
    type Form = PostForm;

    fn declare() -> ResourceDef<Self> {
        let c = PostForm::controls();
        ResourceDef::new()
            .table(Table::new(TextColumn::new(lens!(Post.title))))
            // ANCHOR: post-policy-editors
            .policy(ReadOnly.or(when(editors_only)))
            // ANCHOR_END: post-policy-editors
            // ANCHOR: post-tenancy
            .tenancy(Tenancy::column(Post::fields().tenant_id()))
            // ANCHOR_END: post-tenancy
            // ANCHOR: post-view
            .view(Schema::new(Section::new("Post").schema((
                c.title,
                c.body.multiline(6),
                c.status,
            ))))
            // ANCHOR_END: post-view
            // ANCHOR: post-relations
            // The related model's foreign key, which holds the post's primary key.
            .relation(Relation::has_many::<CommentResource>(
                Comment::fields().post_id(),
            ))
            // ANCHOR_END: post-relations
            // ANCHOR: post-actions
            .action::<Publish>()
        // ANCHOR_END: post-actions
    }

    // ANCHOR: post-view-query
    fn view_query(cx: &Cx) -> Query<List<Post>> {
        let author: Include<Post, Author> = Post::fields().author().into();
        Self::query(cx).include(author)
    }
    // ANCHOR_END: post-view-query
}

// ANCHOR: post-record-form
#[derive(Debug, Clone, tablo_core::RecordForm)]
#[form(model = Post)]
pub struct PostForm {
    pub title: String,
    pub body: String,
    pub status: String,
}
// ANCHOR_END: post-record-form

pub struct CommentResource;

impl Resource for CommentResource {
    type Model = Comment;
    type Form = CommentForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .table(Table::new(TextColumn::new(lens!(Comment.body))))
            // ANCHOR: comment-tenancy-via
            .tenancy(Tenancy::via(Comment::fields().post().tenant_id()))
        // ANCHOR_END: comment-tenancy-via
    }

    // ANCHOR: comment-update-record
    async fn update_record(
        cx: &Cx,
        record: Comment,
        posted: Posted<CommentForm>,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Comment> {
        // `Posted` derefs to the form.
        ensure_post_in_tenant(cx, posted.post_id, ex).await?;
        tablo_core::write_update::<Self>(cx, record, posted, ex).await
    }
    // ANCHOR_END: comment-update-record
}

#[derive(Debug, Clone, tablo_core::RecordForm)]
#[form(model = Comment)]
pub struct CommentForm {
    pub post_id: uuid::Uuid,
    pub body: String,
}

async fn ensure_post_in_tenant(
    _cx: &Cx,
    _post_id: uuid::Uuid,
    _ex: &mut dyn toasty::Executor,
) -> Result<()> {
    Ok(())
}

pub struct AuthorResource;

impl Resource for AuthorResource {
    type Model = Author;
    type Form = AuthorForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .table(Table::new(TextColumn::new(lens!(Author.name))))
            // ANCHOR: author-policy-editors
            .policy(when(editors_only))
        // ANCHOR_END: author-policy-editors
    }
}

#[derive(Debug, Clone, tablo_core::RecordForm)]
#[form(model = Author)]
pub struct AuthorForm {
    pub name: String,
}

// ANCHOR: audit-resource
pub struct AuditResource;

impl Resource for AuditResource {
    type Model = Audit;
    type Form = NoForm<Audit>; // list-only: no create or edit pages

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Audit.action))))
    }
}
// ANCHOR_END: audit-resource

pub struct OrderResource;

impl Resource for OrderResource {
    type Model = Order;
    type Form = NoForm<Order>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().table(Table::new(TextColumn::new(lens!(Order.id))))
    }
}

// ANCHOR: soft-deleted-query
pub fn query(_cx: &Cx) -> Query<List<Post>> {
    Query::<List<Post>>::all().filter(Post::fields().deleted_at().is_none())
}
// ANCHOR_END: soft-deleted-query

// ANCHOR: notify-after-commit
pub async fn after_commit(cx: &Cx, committed: Committed<Post>) -> Result<()> {
    for post in committed.records() {
        notify_subscribers(cx, post).await?;
    }
    Ok(())
}
// ANCHOR_END: notify-after-commit

pub async fn notify_subscribers(_cx: &Cx, _post: &Post) -> Result<()> {
    Ok(())
}
