use std::path::PathBuf;

use tablo_core::{
    Action, Actions, BooleanColumn, Brand, ColumnWidth, Committed, DateFilter, DeclCx, Field,
    FieldErrors, Grid, Group, NavigationItem, Options, Panel, Posted, Relation, Repeater,
    ResolvedLens, Resource, Schema, Section, SelectFilter, Table, TernaryFilter, TextColumn,
    Uploader, VariantFilter, scoped_query, tenant_id, write_create, write_update,
};
use toasty::Db;
use topcoat::{
    Result,
    asset::AssetBundle,
    context::Cx,
    font::{Font, fontsource::fontsource_font},
    router::{Router, Slot, layout},
    tailwind,
    view::{View, ViewExt, view},
};

use crate::{
    dashboard::Dashboard,
    live::LiveActivityPage,
    media::{MediaLibrary, MediaLibraryPage},
    models::{
        Author, BLOCKED_TENANT, Comment, MediaAsset, Post, PostStatus, Publication,
        REMOVED_COMMENT_BODY, Role, Seo, User,
    },
};

/// The theme's sans font, pulled from Fontsource and self-hosted as a Topcoat
/// asset.
///
/// Registered as the panel's shell font, so the admin shell and every document
/// `Panel::document` renders link the same typeface.
const GEIST: Font = fontsource_font!(GEIST, host: Asset);

/// How many words `body` holds: whitespace-separated tokens.
///
/// Empty bodies hold none.
pub fn word_count(body: &str) -> usize {
    body.split_whitespace().count()
}

/// How many minutes `words` take to read at 200 words per minute, rounded up.
///
/// Empty bodies read in no minutes.
pub fn read_minutes(words: usize) -> i64 {
    if words == 0 {
        0
    } else {
        words.div_ceil(200) as i64
    }
}

// ---------------------------------------------------------------------------
// Resource — single Model → Resource, see CONTEXT.md
// ---------------------------------------------------------------------------

/// Admin resource for `User`.
///
/// A hand-written `Resource` impl: this one declares a custom `Table`, and the
/// hooks are the declaration, so there is nothing for a derive to fill in.
pub struct UserResource;

impl Resource for UserResource {
    type Model = User;
    type Form = UserForm;

    fn navigation() -> NavigationItem {
        NavigationItem::for_resource::<Self>().icon(tablo_ui::icons::USERS)
    }

    /// The derived controls, arranged into one section: each binding, label
    /// and control kind comes from `UserForm`, so this only adds what the
    /// fields cannot say — placeholders, the email rule, and which controls may
    /// be left empty.
    fn form(dx: &DeclCx) -> Schema {
        let c = UserForm::controls(dx);
        Schema::new(Section::new("Profile").schema((
            c.name.placeholder("Ada Lovelace"),
            c.email.email().unique().placeholder("ada@example.com"),
            c.role.optional(),
            c.active,
            // A stored integer: optional, zero or more.
            c.age.optional(),
        )))
    }

    /// A stored integer holds zero or more: a non-number is the typed rule's
    /// error, and a negative is this rule's. Both render inline with a 200 and
    /// write nothing, never a 500 from a record fn.
    fn validate_record(_cx: &Cx, form: &UserForm) -> FieldErrors {
        let mut errors = FieldErrors::new();
        if form.age < 0 {
            errors.add("age", "Age must be zero or more");
        }
        errors
    }

    fn can_view_any(_cx: &Cx) -> bool {
        true
    }
    fn can_view(_cx: &Cx, _record: &User) -> bool {
        true
    }
    fn can_create(_cx: &Cx) -> bool {
        true
    }
    fn can_update(_cx: &Cx, record: &User) -> bool {
        // Row-level rule: Ken's account is SSO-managed outside the panel,
        // so the panel never writes it (reads still flow).
        record.name != "Ken Thompson"
    }
    fn can_delete_any(_cx: &Cx) -> bool {
        true
    }
    fn can_delete(_cx: &Cx, record: &User) -> bool {
        // Same SSO guard on the delete path: per-row Policy proven over HTTP.
        record.name != "Ken Thompson"
    }

    fn table() -> Table<User> {
        Table::new(
            |u: &User| u.id.to_string(),
            (
                TextColumn::r#for(User::fields().name(), |u: &User| u.name.clone())
                    .searchable()
                    .sortable(),
                TextColumn::r#for(User::fields().email(), |u: &User| u.email.clone()).searchable(),
                TextColumn::r#for(User::fields().role(), |u: &User| u.role.clone()),
                TextColumn::computed("Status", |u: &User| {
                    if u.active { "Active" } else { "Inactive" }.to_string()
                }),
                // A date's length is known: the column declares it rather
                // than take a narrow share that clips it on a smaller window.
                TextColumn::computed("Created", |u: &User| {
                    u.created_at.strftime("%Y-%m-%d").to_string()
                })
                .width(ColumnWidth::Rem(8)),
            ),
        )
        .live_search()
    }

    /// The form's controls, read-only: a choice shows its option's label and
    /// the toggle reads Yes or No.
    fn view(dx: &DeclCx) -> Schema {
        let c = UserForm::controls(dx);
        Schema::new(Section::new("Profile").schema((c.name, c.email, c.role, c.active, c.age)))
    }

    /// Wake the live feed after a committed write, so an open page re-reads the
    /// users it shows.
    async fn after_commit(_cx: &Cx, _committed: Committed<User>) -> Result<()> {
        crate::live::notify();
        Ok(())
    }
}

/// What the user form writes, and the controls it renders: `role` is a choice
/// over [`Role`], `active` a toggle, the rest text. `role`, `active`, and `age`
/// are optional controls, so each declares what an emptied control stores: the
/// create defaults, and zero for a stored integer.
#[derive(tablo_core::RecordForm)]
#[form(model = User)]
pub struct UserForm {
    pub name: String,
    pub email: String,
    #[form(options = Role, blank = Role::Member.value())]
    pub role: String,
    #[form(blank = true)]
    pub active: bool,
    #[form(blank = 0)]
    pub age: i64,
}

pub struct AuthorResource;

impl Resource for AuthorResource {
    type Model = Author;
    type Form = AuthorForm;

    fn navigation() -> NavigationItem {
        NavigationItem::for_resource::<Self>().icon(tablo_ui::icons::PEN_LINE)
    }

    fn form(dx: &DeclCx) -> Schema {
        let c = AuthorForm::controls(dx);
        Schema::new((c.name, c.email.email()))
    }

    fn label() -> String {
        "Writer".to_string()
    }

    // No `query` override: the framework ANDs `tenant_id = <tenant>`
    // onto the default query for a `requires_tenant` resource, derived from
    // `Author`'s own schema, so the filter cannot be forgotten here.
    fn can_view_any(cx: &Cx) -> bool {
        if tenant_id(cx).is_some_and(|tid| tid == BLOCKED_TENANT) {
            return false;
        }
        true
    }
    fn can_view(cx: &Cx, _record: &Author) -> bool {
        Self::can_view_any(cx)
    }
    fn can_create(cx: &Cx) -> bool {
        Self::can_view_any(cx)
    }
    fn can_update(cx: &Cx, _record: &Author) -> bool {
        Self::can_view_any(cx)
    }
    fn can_delete_any(cx: &Cx) -> bool {
        Self::can_view_any(cx)
    }

    // Tenant-scoped model: every handler fails closed without a
    // tenant instead of leaking unscoped rows or minting nil-tenant orphans.
    fn requires_tenant() -> bool {
        true
    }

    fn table() -> Table<Author> {
        Table::new(
            |a: &Author| a.id.to_string(),
            (
                TextColumn::r#for(Author::fields().name(), |a: &Author| a.name.clone())
                    .searchable()
                    .sortable(),
                TextColumn::r#for(Author::fields().email(), |a: &Author| a.email.clone())
                    .searchable(),
            ),
        )
        .live_search()
    }
}

/// What the author form writes. The tenant is the framework's to stamp on
/// create, so it is not a field.
#[derive(tablo_core::RecordForm)]
#[form(model = Author)]
pub struct AuthorForm {
    pub name: String,
    pub email: String,
}

pub struct PostResource;

impl Resource for PostResource {
    type Model = Post;
    type Form = PostForm;

    fn navigation() -> NavigationItem {
        NavigationItem::for_resource::<Self>().icon(tablo_ui::icons::FILE_TEXT)
    }

    fn form(dx: &DeclCx) -> Schema {
        let c = PostForm::controls(dx);
        Schema::new((
            Section::new("Content").schema((
                c.title.placeholder("A title editors click"),
                // Prose, so a textarea rather than a one-line input.
                // Optional so quick draft stubs submit; full stories fill it.
                c.body
                    .multiline(6)
                    .placeholder("The full story…")
                    .optional(),
            )),
            // Grouped metadata: lifecycle selects beside the author picker,
            // titled like the detail page's "Details" panel.
            Group::new().schema((
                Section::new("Details").schema((
                    Grid::new(2).schema((c.status.optional(), c.featured)),
                    c.author_id
                        .relationship::<AuthorResource>(
                            AuthorResource::query,
                            |a: &Author| a.id,
                            |a: &Author| a.name.clone(),
                        )
                        .searchable()
                        .label("Author"),
                    // One media source: the cover is a picked library row, not an
                    // upload. Optional and single: empty clears the cover.
                    c.cover_id
                        .relationship::<MediaLibrary>(
                            |_cx| toasty::stmt::Query::<toasty::stmt::List<MediaAsset>>::all(),
                            |m: &MediaAsset| m.id,
                            |m: &MediaAsset| m.filename.clone(),
                        )
                        .searchable()
                        .label("Cover")
                        .optional(),
                )),
                Repeater::new("Tags").schema(c.tags.label("Tag")),
            )),
            // Embedded **values**: the controls, their flattened names, and the
            // enum's discriminant all come from the app schema and the type's
            // own shape — nothing here spells `seo_title`, and no variant is
            // recovered from which payload columns happen to be filled in.
            Group::new().schema((
                Section::new("SEO").schema(c.seo),
                Section::new("Publication").schema(c.publication),
            )),
        ))
    }

    async fn create_record(cx: &Cx, form: PostForm, ex: &mut dyn toasty::Executor) -> Result<Post> {
        ensure_author_in_tenant(cx, form.author_id, ex).await?;
        write_create::<Self>(cx, form, ex).await
    }

    async fn update_record(
        cx: &Cx,
        record: Post,
        posted: Posted<PostForm>,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Post> {
        // An unposted `author_id` reads as the stored one, which is re-checked
        // too: the author may have left the tenant since.
        ensure_author_in_tenant(cx, posted.author_id, ex).await?;
        write_update::<Self>(cx, record, posted, ex).await
    }

    fn label() -> String {
        "Blog Post".to_string()
    }

    /// The post's title, so the detail heading names the post rather than its
    /// record key.
    fn record_label(_cx: &Cx, record: &Post) -> Option<String> {
        Some(record.title.clone())
    }

    /// The post's public page, linked from the detail and edit headers.
    ///
    /// The blog serves published posts only, so an unpublished record links
    /// nothing: the URL would answer not-found.
    fn public_url(_cx: &Cx, record: &Post) -> Option<String> {
        if record.status == crate::blog::PUBLISHED {
            Some(format!("/blog/{}", record.id))
        } else {
            None
        }
    }

    /// One post, read-only, from the form's own controls, so a field means
    /// the same thing on both pages.
    ///
    /// What is absent, and why: the **author key** (`Uuid`) and the **cover key**
    /// (`Option<Uuid>`), because a key renders as an id rather than a name —
    /// the cover's key renders through [`Self::view_content`] — and the
    /// **comments**, which render as a relation ([`Self::relations`]). The
    /// publication shows its one date rather than every variant's payload.
    fn view(dx: &DeclCx) -> Schema {
        let c = PostForm::controls(dx);
        Schema::new((
            Section::new("Post").schema((c.title, c.body.multiline(6))),
            Section::new("Details")
                .schema(Group::new().schema((Grid::new(2).schema((c.status, c.featured)), c.tags))),
            Section::new("SEO").schema(c.seo),
            Section::new("Publication").schema(
                Field::text(ResolvedLens::new(
                    dx,
                    Post::fields().publication().published().published_at(),
                ))
                .label("Published at")
                .optional(),
            ),
        ))
    }

    /// The post's computed reading stats and its cover, above its comments.
    fn view_content<'a>(cx: &'a Cx, record: &Post) -> Option<topcoat::view::BoxView<'a>> {
        let words = word_count(&record.body);
        let minutes = read_minutes(words);
        let cover_id = record.cover_id;
        Some(
            view! {
                cx =>
                <div class="flex flex-col gap-1">
                    <p class="text-sm text-muted-foreground">
                        (format!("{words} words · {minutes} min read"))
                    </p>
                    if let Some(cover_id) = cover_id {
                        <p class="text-xs text-muted-foreground">
                            (format!("Cover: {cover_id}"))
                        </p>
                    }
                </div>
            }
            .boxed(),
        )
    }

    /// Publish drafts from a row or for the selection.
    fn actions() -> Actions<Self> {
        Actions::new().add::<PublishPosts>()
    }

    /// The post's comments: the comments list's own table, narrowed to this
    /// post, on its detail and edit pages, with a create link that opens the
    /// comment form with this post chosen.
    fn relations() -> Vec<Relation<Post>> {
        vec![Relation::has_many::<CommentResource, _>(
            Comment::fields().post_id(),
            |post: &Post| post.id,
        )]
    }

    fn can_view_any(cx: &Cx) -> bool {
        if tenant_id(cx).is_some_and(|tid| tid == BLOCKED_TENANT) {
            return false;
        }
        true
    }
    fn can_view(cx: &Cx, _record: &Post) -> bool {
        Self::can_view_any(cx)
    }
    fn can_create(cx: &Cx) -> bool {
        Self::can_view_any(cx)
    }
    fn can_update(cx: &Cx, _record: &Post) -> bool {
        Self::can_view_any(cx)
    }
    fn can_delete_any(cx: &Cx) -> bool {
        Self::can_view_any(cx)
    }

    // Tenant-scoped model: every handler fails closed without a
    // tenant instead of leaking unscoped rows or minting nil-tenant orphans.
    fn requires_tenant() -> bool {
        true
    }

    fn table() -> Table<Post> {
        Table::new(
            |p: &Post| p.id.to_string(),
            (
                TextColumn::r#for(Post::fields().title(), |p: &Post| p.title.clone())
                    .searchable()
                    .sortable(),
                // a status is narrow by content, not by kind — `r#for`
                // binds a `String` field, which the framework cannot tell from
                // a title. Featured and Comments below keep their narrow
                // default.
                TextColumn::r#for(Post::fields().status(), |p: &Post| {
                    PostStatus::label_of(&p.status)
                })
                .width(ColumnWidth::Narrow),
                BooleanColumn::r#for(Post::fields().featured(), |p: &Post| p.featured),
                // The other override direction: a computed column that holds a
                // name is body text, so it takes a share of the free width.
                TextColumn::computed("Author", |p: &Post| {
                    // Loud on missing includes: a silent "-" reads as data.
                    // The list and the export load author because this column
                    // includes it, so this fires only if that `.include` goes.
                    debug_assert!(
                        !p.author.is_unloaded(),
                        "the Author column declares `.include(Post::fields().author())`"
                    );
                    if p.author.is_unloaded() {
                        "(unloaded)".to_string()
                    } else {
                        p.author.get().name.clone()
                    }
                })
                .width(ColumnWidth::Wide)
                .include(Post::fields().author()),
                TextColumn::computed("Comments", |p: &Post| {
                    debug_assert!(
                        !p.comments.is_unloaded(),
                        "the Comments column declares `.include(Post::fields().comments())`"
                    );
                    if p.comments.is_unloaded() {
                        "(unloaded)".to_string()
                    } else {
                        p.comments.get().len().to_string()
                    }
                })
                .include(Post::fields().comments()),
            ),
        )
        .filters((
            SelectFilter::r#for(Post::fields().status(), PostStatus::options()),
            TernaryFilter::r#for(Post::fields().featured()),
            DateFilter::r#for(Post::fields().created_at()),
            // Prebuilt-expression VariantFilter (no embedded enum needed):
            // the editorial facet, where each option pairs the featured
            // flag with the lifecycle status — `Promoted` is featured and
            // published, `Backlog` is a draft that is not featured.
            VariantFilter::r#for(
                "promoted",
                "Promoted",
                vec![
                    (
                        "Promoted".to_string(),
                        Post::fields().featured().eq(true).and(
                            Post::fields()
                                .status()
                                .eq(PostStatus::Published.value().to_string()),
                        ),
                    ),
                    (
                        "Backlog".to_string(),
                        Post::fields().featured().eq(false).and(
                            Post::fields()
                                .status()
                                .eq(PostStatus::Draft.value().to_string()),
                        ),
                    ),
                ],
            ),
        ))
        .group_by("status", |p: &Post| p.status.clone())
        .live_search()
    }
}

/// Publish draft posts, from a row or for the selection.
///
/// The framework loads the posts through the tenant-scoped query inside its
/// transaction and checks `can_view` and `can_run` on each before `run`
/// writes through the same transaction.
pub struct PublishPosts;

impl Action<PostResource> for PublishPosts {
    const NAME: &'static str = "publish";

    fn label() -> String {
        "Publish".to_string()
    }

    fn can_run(_cx: &Cx, post: &Post) -> bool {
        post.status != crate::blog::PUBLISHED
    }

    async fn run(_cx: &Cx, posts: &[Post], ex: &mut dyn toasty::Executor) -> Result<()> {
        for post in posts {
            Post::filter(Post::fields().id().eq(post.id))
                .update()
                .status(crate::blog::PUBLISHED.to_string())
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }
}

/// What the post form writes: every column but the tenant (stamped by the
/// framework), `created_at` (a model default), and the relations. The embedded
/// values are written whole.
#[derive(tablo_core::RecordForm)]
#[form(model = Post)]
pub struct PostForm {
    pub title: String,
    pub body: String,
    #[form(options = PostStatus, blank = PostStatus::Draft.value())]
    pub status: String,
    pub featured: bool,
    #[form(choice)]
    pub author_id: uuid::Uuid,
    #[form(choice)]
    pub cover_id: Option<uuid::Uuid>,
    pub tags: String,
    #[form(embed)]
    pub seo: Seo,
    #[form(embed)]
    pub publication: Publication,
}

/// The author must exist *in this tenant* at write time: `validate_async`
/// checked the option set before the transaction opened, but the author may
/// be cross-tenant or deleted since, so the check re-runs through the
/// tenant-scoped query inside the transaction. A miss is a 500.
async fn ensure_author_in_tenant(
    cx: &Cx,
    author_id: uuid::Uuid,
    ex: &mut dyn toasty::Executor,
) -> Result<()> {
    let author_exists = scoped_query::<AuthorResource>(cx)?
        .filter(Author::fields().id().eq(author_id))
        .first()
        .exec(&mut *ex)
        .await
        .map_err(|e| -> topcoat::Error { e.into() })?
        .is_some();
    if !author_exists {
        return Err(topcoat::Error::from(std::io::Error::other(
            "author not found",
        )));
    }
    Ok(())
}

/// Comments resource over `Comment`: the moderation queue.
///
/// Comments carry no tenant of their own — they inherit visibility from their
/// post — so the request tenant is required like any other gated
/// resource, and the scope is the parent post's tenant, declared in
/// [`tenant_scope`](Resource::tenant_scope).
///
/// That declaration is what the framework's default cannot supply: the default
/// derives the filter from a `tenant_id` column on the model, and `Comment` has
/// none. Leaving `requires_tenant` false and writing the filter inside `query`
/// was the GH #223 review's leak — a tenantless request silently *skipped* the
/// filter instead of being refused, and with `can_view_any`/`can_view` true the
/// caller could read and moderate every tenant's comments. Gated + declared is
/// the shape that fails closed: no tenant is a 403 everywhere, and the
/// predicate is the framework's to apply.
///
/// The queue moderates: `can_delete_any` enables row and bulk delete, and
/// `delete_record` writes them — bulk delete rides
/// the framework default that loops `delete_record`. A resource that wants a
/// read-only queue leaves [`can_delete_any`](Resource::can_delete_any) at its default instead.
pub struct CommentResource;

/// Re-resolve a comment's parent post through the tenant-scoped
/// [`scoped_query::<PostResource>`] inside the caller's open transaction.
/// `Schema::validate_async` already rejects a `post_id` outside the
/// tenant-scoped option set before the tx opens, but that
/// is a pre-write check in a different window: a policy or tenant change between
/// the two would slip through, and a direct `create_record` / `update_record`
/// caller never ran it at all. Mirroring `PostResource`'s author double-check,
/// this is the defense-in-depth half — one query on the seam that owns tenancy.
///
/// The miss is a 404, not the 500 `PostResource` uses for a missing author: a
/// parent in another tenant is an authorization boundary, and "wrong tenant
/// looks exactly like unknown id" is this panel's contract everywhere else
/// (#169).
async fn ensure_post_in_tenant(
    cx: &Cx,
    post_id: uuid::Uuid,
    ex: &mut dyn toasty::Executor,
) -> Result<()> {
    let in_tenant = scoped_query::<PostResource>(cx)?
        .filter(Post::fields().id().eq(post_id))
        .first()
        .exec(&mut *ex)
        .await
        .map_err(|e| -> topcoat::Error { e.into() })?
        .is_some();
    if !in_tenant {
        return Err(topcoat::router::error::not_found().into());
    }
    Ok(())
}

impl Resource for CommentResource {
    type Model = Comment;
    type Form = CommentForm;

    fn navigation() -> NavigationItem {
        NavigationItem::for_resource::<Self>().icon(tablo_ui::icons::MESSAGE_SQUARE)
    }

    fn form(dx: &DeclCx) -> Schema {
        let c = CommentForm::controls(dx);
        Schema::new((
            // Prose, so a textarea rather than a one-line input — the same
            // shape the post body uses.
            c.body.multiline(4).placeholder("Write a reply…"),
            c.post_id
                .relationship::<PostResource>(
                    PostResource::query,
                    |p: &Post| p.id,
                    |p: &Post| p.title.clone(),
                )
                .searchable()
                .label("Post"),
        ))
    }

    async fn create_record(
        cx: &Cx,
        form: CommentForm,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Comment> {
        // Tenancy double-check inside the tx: the pre-tx option-set
        // validation is not a write-time guarantee.
        ensure_post_in_tenant(cx, form.post_id, ex).await?;
        write_create::<Self>(cx, form, ex).await
    }

    async fn update_record(
        cx: &Cx,
        record: Comment,
        posted: Posted<CommentForm>,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Comment> {
        // An update can re-point the comment at another post, which is exactly
        // the move the pre-tx check cannot be trusted to catch.
        ensure_post_in_tenant(cx, posted.post_id, ex).await?;
        write_update::<Self>(cx, record, posted, ex).await
    }

    fn navigation_label() -> String {
        // "Comments", not "Discussion": the entity is a comment, the
        // route and model say so, and a discussion — if it means anything here
        // — would be the set of comments on one post, which is not a record the
        // panel can list or moderate.
        "Comments".to_string()
    }

    /// Tenant-scoped from the parent post, and gated like every other
    /// tenant-owned resource.
    ///
    /// `true` is what makes a tenantless request a 403 here instead of a read
    /// that quietly dropped the filter; the predicate below replaces the
    /// framework's name-based derivation, which finds no `tenant_id` on
    /// `Comment`.
    fn requires_tenant() -> bool {
        true
    }

    /// Inherit-through-the-relation: scope through the parent post's
    /// tenant. Toasty rewrites the relation-path comparison into a foreign-key
    /// subquery, and the framework ANDs the result onto `query`/`view_query`
    /// exactly as it ANDs the derived `tenant_id` filter elsewhere.
    fn tenant_scope(tenant: uuid::Uuid) -> Option<toasty::stmt::Expr<bool>> {
        Some(Comment::fields().post().tenant_id().eq(tenant))
    }

    fn can_view_any(_cx: &Cx) -> bool {
        true
    }
    /// A removed comment keeps its row as a placeholder: there is no
    /// content to read, so this refuses it and the post's relation omits it.
    /// The framework withholds the queue's row actions from a record this
    /// refuses, which is the same answer — a placeholder has nothing
    /// to edit or delete.
    fn can_view(_cx: &Cx, record: &Comment) -> bool {
        record.body != REMOVED_COMMENT_BODY
    }
    fn can_create(_cx: &Cx) -> bool {
        true
    }
    fn can_update(_cx: &Cx, _record: &Comment) -> bool {
        true
    }
    fn can_delete_any(_cx: &Cx) -> bool {
        true
    }

    fn table() -> Table<Comment> {
        Table::new(
            |c: &Comment| c.id.to_string(),
            (
                TextColumn::r#for(Comment::fields().body(), |c: &Comment| c.body.clone())
                    .searchable()
                    .sortable(),
                TextColumn::computed("Post", |c: &Comment| {
                    debug_assert!(
                        !c.post.is_unloaded(),
                        "the Post column declares `.include(Comment::fields().post())`"
                    );
                    if c.post.is_unloaded() {
                        "(unloaded)".to_string()
                    } else {
                        c.post.get().title.clone()
                    }
                })
                // a post title is body text, not the narrow badge a
                // computed column defaults to.
                .width(ColumnWidth::Wide)
                .include(Comment::fields().post()),
            ),
        )
        .live_search()
    }
}

/// What the comment form writes.
#[derive(tablo_core::RecordForm)]
#[form(model = Comment)]
pub struct CommentForm {
    pub body: String,
    #[form(choice)]
    pub post_id: uuid::Uuid,
}

#[layout("/admin")]
async fn admin_layout(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    Panel::layout_shell(cx, slot).await
}

pub fn router(db: Db) -> Router {
    build_router(db, Some(load_assets()), Some(upload_dir()))
}

/// Build the showcase router without filesystem assets for markup tests.
///
/// This is deliberately separate from [`router`]: the application path fails
/// loudly when its generated bundle is missing, while tests can exercise the
/// server-rendered markup without pretending an asset bundle exists.
///
/// It installs **no uploader** either, which pins the framework's default for
/// a file field with no store. A test that needs the demo store uses
/// [`router_with_app_uploads`].
pub fn router_for_tests(db: Db) -> Router {
    build_router(db, None, None)
}

/// Build the showcase router with uploads at the directory the application
/// itself uses.
///
/// The media library's page writes through this configuration — the panel's
/// `serve_dir` mount and the store are two ends of one directory.
pub fn router_with_app_uploads(db: Db) -> Router {
    build_router(db, None, Some(upload_dir()))
}

/// Where the showcase writes uploaded bytes: `SHOWCASE_UPLOAD_DIR`, or
/// `target/showcase-uploads` so a local run works with no configuration.
pub(crate) fn upload_dir() -> PathBuf {
    std::env::var_os("SHOWCASE_UPLOAD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/showcase-uploads"))
}

/// The URL prefix uploads are served under.
///
/// It matches the `serve_dir` route below, and the app owns both ends: the
/// store decides the path it returns, so the framework never has to guess a
/// URL convention.
pub const UPLOAD_URL_PREFIX: &str = "/uploads";

/// The showcase's own uploader: write the bytes into the served directory and
/// return the URL they are served at.
///
/// A demo, not a framework default — the trait is the seam and drivers are the
/// app's business. The UUID prefix keeps two uploads of `cover.png` apart, and
/// the framework hands this store a name already sanitized to a basename.
/// Writing the file is this app's job; swapping in an object store
/// means replacing this type and nothing else.
///
/// The media library's page builds this store too: it parses its own
/// multipart body, so the sanitized name is not the framework's to guarantee
/// there — the store applies [`basename`] itself rather than trusting every
/// caller to have done it.
pub(crate) struct DirUploader {
    dir: PathBuf,
}

impl DirUploader {
    pub(crate) fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
}

impl Uploader for DirUploader {
    async fn store(&self, filename: &str, bytes: &[u8]) -> Result<String, String> {
        let name = format!("{}-{}", uuid::Uuid::new_v4(), basename(filename));
        // Failure reasons are rendered to the user, so they say what the user
        // can act on and never leak the path that failed.
        tokio::fs::create_dir_all(&self.dir)
            .await
            .map_err(|_| "the upload directory is not writable".to_string())?;
        tokio::fs::write(self.dir.join(&name), bytes)
            .await
            .map_err(|_| "the upload could not be written".to_string())?;
        // The caller stores this string and renders it verbatim as the file's
        // URL, so it has to be one: the segment is percent-encoded,
        // or `cover #1.png` would be served as `cover ` plus a fragment, and a
        // `%22` the browser sent in the filename would decode to a quote the
        // file on disk does not carry.
        Ok(format!("{UPLOAD_URL_PREFIX}/{}", url_segment(&name)))
    }

    /// Whether the served directory still holds the file a URL names.
    ///
    /// The candidate comes from a form the panel re-rendered, so it is a URL
    /// this store itself produced; the check is a lookup rather than a path
    /// join — every entry in the store's own root is encoded back to its URL
    /// form and compared, so a `%2F`, a `..` or an absolute path a client
    /// submits never reaches the filesystem.
    async fn holds(&self, path: &str) -> bool {
        let Some(segment) = path.strip_prefix(UPLOAD_URL_PREFIX) else {
            return false;
        };
        let Some(segment) = segment.strip_prefix('/') else {
            return false;
        };
        if segment.is_empty() {
            return false;
        }
        let Ok(mut entries) = tokio::fs::read_dir(&self.dir).await else {
            return false;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            if url_segment(&entry.file_name().to_string_lossy()) == segment {
                return true;
            }
        }
        false
    }
}

/// The longest client filename the showcase keeps, in bytes.
///
/// 255 bytes is the common per-component limit on Linux filesystems, and the
/// store's `{uuid}-` prefix takes 37 of them, so the basename is capped to what
/// is left. The media library records the name it stores, so the row's
/// `filename` and the file on disk cannot disagree.
const MAX_BASENAME_BYTES: usize = 218;

/// Reduce a client-supplied filename to the basename this app stores and shows.
/// Strips directory components (`../../etc/passwd` → `passwd`), drops control
/// characters, trims the ends, and caps the byte length preserving the tail, so
/// the extension survives. The framework sanitizes the names its own form
/// parser hands an [`Uploader`]; the media library's page parses its own
/// multipart body, and this is the one rule both callers apply.
pub(crate) fn basename(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let clean: String = base.chars().filter(|c| !c.is_control()).collect();
    let trimmed = clean.trim();
    if trimmed.len() <= MAX_BASENAME_BYTES {
        return trimmed.to_string();
    }
    // Walk the cut point forward to a char boundary: slicing a multibyte
    // character would panic on an attacker-controlled filename.
    let mut start = trimmed.len() - MAX_BASENAME_BYTES;
    while !trimmed.is_char_boundary(start) {
        start += 1;
    }
    trimmed[start..].to_string()
}

/// `name` as one URL path segment: everything outside the unreserved set is
/// percent-encoded (RFC 3986 §2.3).
///
/// The store's contract is that the string it returns is fetchable verbatim
/// and a client filename is arbitrary: a space must not become the
/// end of the URL, a `#` must not start a fragment, and a `%` must not decode
/// to something else.
fn url_segment(name: &str) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(name.len());
    for byte in name.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

fn build_router(db: Db, bundle: Option<AssetBundle>, uploads: Option<PathBuf>) -> Router {
    let mut panel = Panel::new("admin")
        .app_context(db)
        .brand(
            Brand::new("Tablo Blog").logo(
                "data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20viewBox='0%200%2024%2024'%3E%3Ccircle%20cx='12'%20cy='12'%20r='10'%20fill='%236366f1'/%3E%3Ctext%20x='12'%20y='16'%20text-anchor='middle'%20font-size='12'%20fill='white'%20font-family='sans-serif'%3EA%3C/text%3E%3C/svg%3E",
            ),
        )
        // Light by default: the header toggle is the only thing that
        // turns dark on. `Panel::dark_mode` stays available for an app that
        // wants a dark-first panel.
        .home::<Dashboard>()
        .resource::<UserResource>()
        .resource::<AuthorResource>()
        .resource::<PostResource>()
        .resource::<CommentResource>()
        .page::<MediaLibraryPage>()
        .page::<LiveActivityPage>();
    // No "Published" saved-view entry: it would point at
    // `/admin/posts?f.status=published`, i.e. the Blog Posts table with a
    // filter — the same page twice in the sidebar, and the one arrangement the
    // shell's path matching highlights twice at once.
    // Demo credentials stay available for local development via
    // SHOWCASE_LOGIN_HINT, but the default login page is shippable with no
    // hint. Empty values install nothing (no empty hint paragraph).
    if let Ok(hint) = std::env::var("SHOWCASE_LOGIN_HINT")
        && !hint.trim().is_empty()
    {
        panel = panel.login_hint(hint);
    }
    if let Some(dir) = uploads {
        // Both ends of the demo: the store writes into `dir` and
        // returns `{UPLOAD_URL_PREFIX}/…`, and the panel serves exactly that
        // prefix from the same directory — which is why the stored path is
        // fetchable without the framework inventing a URL convention.
        panel = panel
            .serve_dir(format!("{UPLOAD_URL_PREFIX}/{{*file}}"), dir.clone())
            .uploads(DirUploader::new(dir));
    }
    match bundle {
        Some(bundle) => panel
            .assets(bundle)
            .shell_assets(tailwind::stylesheet!(), GEIST)
            .build()
            .expect("showcase panel builds"),
        None => panel.build().expect("showcase panel builds"),
    }
}

fn load_assets() -> AssetBundle {
    match AssetBundle::load() {
        Ok(bundle) => bundle,
        Err(near_executable) => {
            // Cargo places test executables in `target/*/deps`, while the
            // bundle remains beside the package binary in `target/*`.
            let test_bundle = std::env::current_exe()
                .ok()
                .and_then(|exe| {
                    let dir = exe.parent()?;
                    if dir.file_name().is_some_and(|name| name == "deps") {
                        Some(dir.parent()?.to_path_buf())
                    } else {
                        None
                    }
                })
                .map(|dir| dir.join("assets"));
            match test_bundle {
                Some(dir) => AssetBundle::load_dir(&dir).unwrap_or_else(|test_error| {
                    panic!(
                        "showcase asset bundle is unavailable: executable lookup failed ({near_executable}); tried {} ({test_error})",
                        dir.display()
                    )
                }),
                None => panic!(
                    "showcase asset bundle is unavailable: executable lookup failed ({near_executable})"
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests;
