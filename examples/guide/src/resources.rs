//! The guide's canonical resources. Each method is anchored into its chapter;
//! required items the guide elides sit outside the anchors.

use tablo::prelude::*;
use topcoat::{Result, context::Cx};

use crate::{
    actions::{ChangeStatus, Publish, PublishDrafts},
    detail_pages::SearchPreviewPage,
    models::{Audit, Author, Comment, Order, Post, PostStatus, Role, User},
    policy_tenancy::editors_only,
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
            .policy_fn(|cx, ability: Ability<'_, User>| match ability {
                Ability::ViewAny | Ability::View(_) => true,
                Ability::Update(user) => !user.sso_managed,
                Ability::Create
                | Ability::DeleteAny
                | Ability::Delete(_)
                | Ability::RestoreAny
                | Ability::Restore(_)
                | Ability::ForceDeleteAny
                | Ability::ForceDelete(_)
                | Ability::RunAny { .. }
                | Ability::Run { .. }
                | Ability::RunHeader { .. } => is_admin(cx),
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
            // ANCHOR: user-field-access
            .field_access(lens!(User.age), FieldAccess::new().view(is_admin))
            .field_access(lens!(User.role), FieldAccess::new().edit(is_admin))
            // ANCHOR_END: user-field-access
            // ANCHOR: user-navigation
            .navigation_order(-1)
            .icon(tablo::ui::icons::USERS)
            // ANCHOR_END: user-navigation
            // ANCHOR: user-global-search
            .record_title(lens!(User.name))
            .global_search(GlobalSearch::new().detail("Email", lens!(User.email)))
        // ANCHOR_END: user-global-search
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
            // Posts are written on the blog, with their author and SEO; the panel edits them.
            .without_page(ResourcePage::Create)
            // ANCHOR: post-tenancy
            .tenancy(Tenancy::column(lens!(Post.tenant_id)))
            // ANCHOR_END: post-tenancy
            // ANCHOR: post-soft-delete
            .soft_delete(lens!(Post.deleted_at))
            // ANCHOR_END: post-soft-delete
            // ANCHOR: post-view
            .detail(Detail::new(Section::new("Post").columns((
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
            .relation(Relation::has_many::<CommentResource>(lens!(
                Comment.post_id
            )))
            // ANCHOR_END: post-relations
            // ANCHOR: post-record-page
            .record_page::<SearchPreviewPage>()
            // ANCHOR_END: post-record-page
            // ANCHOR: post-actions
            .action::<Publish>()
            .action::<ChangeStatus>()
            .header_action::<PublishDrafts>()
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
    ) -> Result<Comment, WriteError<CommentFormField>> {
        if !posted.named(CommentFormField::PostId) || posted.post_id == record.post_id {
            return tablo::write_update::<Self>(cx, record, posted, ex).await;
        }
        let post = Post::filter_by_id(posted.post_id)
            .first()
            .exec(&mut *ex)
            .await?;
        if !post.is_some_and(|post| post.status == PostStatus::Published) {
            return Err(WriteError::field(
                CommentFormField::PostId,
                "Comments go on published posts",
            ));
        }
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
            // Authors write for every tenant.
            .tenancy(Tenancy::none())
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

// ANCHOR: published-query
pub fn blog() -> Panel {
    Panel::new("blog").resource_with::<PostResource>(|def| {
        def.query(|_cx, query| query.filter(Post::fields().status().eq(PostStatus::Published)))
    })
}
// ANCHOR_END: published-query

// ANCHOR: audit-before-commit
pub async fn before_commit(
    _cx: &Cx,
    change: &Change<Post>,
    ex: &mut dyn toasty::Executor,
) -> Result<()> {
    let action = match change {
        Change::Create(post) => format!("created {}", post.title),
        Change::Update { before, after } => {
            format!("updated {} (was {})", after.title, before.title)
        }
        Change::Delete(posts) => format!("deleted {} posts", posts.len()),
        Change::Restore(posts) => format!("restored {} posts", posts.len()),
        Change::ForceDelete(posts) => format!("deleted {} posts for good", posts.len()),
        Change::Action { name, records } => format!("ran {name} on {} posts", records.len()),
        Change::HeaderAction { name } => format!("ran {name}"),
        Change::Attach {
            owner,
            relation,
            key,
        } => {
            format!("linked {} to {relation} {key}", owner.title)
        }
        Change::Detach {
            owner,
            relation,
            keys,
        } => {
            format!(
                "unlinked {} from {relation} {}",
                owner.title,
                keys.join(", ")
            )
        }
    };
    toasty::create!(Audit {
        action,
        created_at: jiff::Timestamp::now().to_string(),
    })
    .exec(ex)
    .await?;
    Ok(())
}
// ANCHOR_END: audit-before-commit

// ANCHOR: notify-after-commit
pub async fn after_commit(cx: &Cx, change: Change<Post>) -> Result<()> {
    if let Change::Create(post) = &change {
        notify_subscribers(cx, post).await?;
    }
    Ok(())
}
// ANCHOR_END: notify-after-commit

pub async fn notify_subscribers(_cx: &Cx, _post: &Post) -> Result<()> {
    Ok(())
}
