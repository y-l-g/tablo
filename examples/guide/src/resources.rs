//! The guide's canonical resources. Each method is anchored into its chapter;
//! required items the guide elides sit outside the anchors.

use tablo::prelude::*;
use toasty::stmt::{List, Query};
use topcoat::{Result, context::Cx};

use crate::{
    models::{Audit, Author, Comment, Order, Post, PostStatus, Role, User},
    policy_tenancy::editors_only,
    tables::{ChangeStatus, Publish},
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
                Ability::Update(user) => !user.sso_managed,
                Ability::Create
                | Ability::DeleteAny
                | Ability::Delete(_)
                | Ability::RunAny { .. }
                | Ability::Run { .. } => is_admin(cx),
            })
            // ANCHOR_END: user-policy
            // ANCHOR: user-form
            .form(Schema::new(Section::new("Profile").schema((
                c.name.placeholder("Ada Lovelace"),
                c.email.email(),
                c.role,
                c.age,
            ))))
            // ANCHOR_END: user-form
            // ANCHOR: user-navigation
            .navigation_order(-1)
            .icon(tablo::ui::icons::USERS)
        // ANCHOR_END: user-navigation
    }

    // ANCHOR: user-validate
    fn validate_record(_cx: &Cx, form: &UserForm) -> FieldErrors<UserFormField> {
        let mut errors = FieldErrors::new();
        if form.age < 0 {
            errors.add(UserFormField::Age, "Age must be zero or more");
        }
        errors
    }
    // ANCHOR_END: user-validate
}

// ANCHOR: user-record-form
#[derive(Debug, Clone, tablo::RecordForm)]
#[form(model = User)]
pub struct UserForm {
    pub name: String,
    pub email: String,
    #[form(options, blank = Role::Member)]
    pub role: Role,
    #[form(blank = 0)]
    pub age: i64,
}
// ANCHOR_END: user-record-form

pub struct PostResource;

impl Resource for PostResource {
    type Model = Post;
    type Form = PostForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .table(Table::new(TextColumn::new(lens!(Post.title))))
            // ANCHOR: post-policy-editors
            .policy(ReadOnly.or(when(editors_only)))
            // ANCHOR_END: post-policy-editors
            // ANCHOR: post-tenancy
            .tenancy(Tenancy::column(lens!(Post.tenant_id)))
            // ANCHOR_END: post-tenancy
            // ANCHOR: post-view
            .view(Detail::new(Section::new("Post").columns((
                TextColumn::new(lens!(Post.title)),
                TextColumn::new(lens!(Post.body)),
                TextColumn::new(lens!(Post.status)),
                RelationColumn::of::<AuthorResource>(relation!(Post.author)),
            ))))
            // ANCHOR_END: post-view
            // ANCHOR: post-record-title
            .record_title(lens!(Post.title))
            // ANCHOR_END: post-record-title
            // ANCHOR: post-public-link
            .public_link(|_cx: &Cx, post: &Post| {
                (post.status == PostStatus::Published).then(|| PublicLink {
                    url: format!("/blog/{}", post.id),
                    label: "View on the blog",
                })
            })
            // ANCHOR_END: post-public-link
            // ANCHOR: post-relations
            // The related model's foreign key, which holds the post's primary key.
            .relation(Relation::has_many::<CommentResource>(
                Comment::fields().post_id(),
            ))
            // ANCHOR_END: post-relations
            // ANCHOR: post-actions
            .action::<Publish>()
            .action::<ChangeStatus>()
        // ANCHOR_END: post-actions
    }
}

// ANCHOR: post-record-form
#[derive(Debug, Clone, tablo::RecordForm)]
#[form(model = Post)]
pub struct PostForm {
    pub title: String,
    pub body: String,
    #[form(options)]
    pub status: PostStatus,
}
// ANCHOR_END: post-record-form

pub struct CommentResource;

impl Resource for CommentResource {
    type Model = Comment;
    type Form = CommentForm;

    fn declare() -> ResourceDef<Self> {
        let c = CommentForm::controls();
        ResourceDef::new()
            .table(Table::new(TextColumn::new(lens!(Comment.body))))
            // ANCHOR: comment-tenancy-via
            .tenancy(Tenancy::via(Comment::fields().post().tenant_id()))
            // ANCHOR_END: comment-tenancy-via
            .form(Schema::new(
                Section::new("Comment").schema((c.body, c.post_id)),
            ))
    }

    // ANCHOR: comment-update-record
    async fn update_record(
        cx: &Cx,
        record: Comment,
        posted: Posted<CommentForm>,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Comment> {
        tablo::write_update::<Self>(cx, record, posted, ex).await
    }
    // ANCHOR_END: comment-update-record
}

#[derive(Debug, Clone, tablo::RecordForm)]
#[form(model = Comment)]
pub struct CommentForm {
    #[form(relationship = PostResource)]
    pub post_id: uuid::Uuid,
    pub body: String,
}

pub struct AuthorResource;

impl Resource for AuthorResource {
    type Model = Author;
    type Form = AuthorForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .table(Table::new(TextColumn::new(lens!(Author.name))))
            // Titles the detail page, the post form's author options and the post's author column.
            .record_title(lens!(Author.name))
            // ANCHOR: author-policy-editors
            .policy(when(editors_only))
        // ANCHOR_END: author-policy-editors
    }
}

#[derive(Debug, Clone, tablo::RecordForm)]
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
