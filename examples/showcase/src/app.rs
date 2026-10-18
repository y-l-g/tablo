//! Serves the showcase panel with demo resources.

use std::path::PathBuf;

use tablo::{
    Ability, Action, ActionLayout, Auth, AuthMail, BooleanColumn, Brand, ComputedColumn,
    CountColumn, DateColumn, DateFilter, Detail, DirUploader, EmailVerification, EmbeddedColumn,
    FieldErrors, Grid, Group, HeaderAction, ModelColumns, NavigationItem, Pagination, Panel,
    PasswordReset, PublicLink, QueryFilter, Relation, RelationColumn, RepeaterColumn, Resource,
    ResourceDef, RouterBuilderPanelExt, Schema, Section, SelectFilter, Table, Tenancy,
    TenantRegistration, TernaryFilter, TextColumn, TwoFactor, lens, relation, scoped_query,
    tenant_id, when,
};
use toasty::Db;
use topcoat::{
    Result,
    asset::{AssetBundle, RouterBuilderAssetExt},
    context::Cx,
    cookie::Key,
    font::{Font, fontsource::fontsource_font},
    mail::{FileTransport, MailConfig, Mailbox, RouterBuilderMailExt},
    router::{Router, RouterBuilderDiscoverExt},
    tailwind,
};

use crate::{
    dashboard::Dashboard,
    media::{MediaLibrary, MediaLibraryPage},
    models::{
        Author, AuthorForm, BLOCKED_TENANT, Category, CategoryForm, Comment, Link, Post,
        PostStatus, Publication, REMOVED_COMMENT_BODY, Seo, User, UserForm, UserFormField,
    },
    staff::StaffAuth,
};

/// The theme's sans font.
const GEIST: Font = fontsource_font!(GEIST, host: Asset);

/// Counts the whitespace-separated words in `body`.
pub fn word_count(body: &str) -> usize {
    body.split_whitespace().count()
}

/// Counts the minutes `words` take to read at 200 words per minute.
pub fn read_minutes(words: usize) -> i64 {
    if words == 0 {
        0
    } else {
        words.div_ceil(200) as i64
    }
}

/// Admin resource for `User`.
pub struct UserResource;

impl Resource for UserResource {
    type Model = User;
    type Form = UserForm;

    fn declare() -> ResourceDef<Self> {
        let c = UserForm::controls();
        ResourceDef::new()
            .icon(tablo::ui::icons::USERS)
            // Refuses writes to Ken's account.
            .policy_fn(|_cx, ability: Ability<'_, User>| match ability {
                Ability::Update(user) | Ability::Delete(user) => user.name != "Ken Thompson",
                _ => true,
            })
            .form(Schema::new(Section::new("Profile").schema((
                c.name.placeholder("Ada Lovelace"),
                c.email.email().unique().placeholder("ada@example.com"),
                c.role,
                c.active,
                c.age,
            ))))
            .detail(Detail::new(Section::new("Profile").columns(User::detail())))
            .record_title(lens!(User.name))
    }

    /// Refuses a negative age.
    fn validate_record(_cx: &Cx, form: &UserForm) -> FieldErrors<UserFormField> {
        let mut errors = FieldErrors::new();
        if form.age < 0 {
            errors.add(UserFormField::Age, "Age must be zero or more");
        }
        errors
    }
}

pub struct AuthorResource;

impl Resource for AuthorResource {
    type Model = Author;
    type Form = AuthorForm;

    fn declare() -> ResourceDef<Self> {
        let c = AuthorForm::controls();
        ResourceDef::new()
            .label("Writer")
            .icon(tablo::ui::icons::PEN_LINE)
            .navigation_group("Blog")
            .policy(when(blog_open))
            .tenancy(Tenancy::column(lens!(Author.tenant_id)))
            .form(Schema::new(
                Section::new("Profile").schema((c.name, c.email.email())),
            ))
            .detail(Detail::new(
                Section::new("Profile").columns(Author::detail()),
            ))
            // Names the detail heading with the author's name.
            .record_title(lens!(Author.name))
    }
}

/// Refuses the blocked tenant.
fn blog_open(cx: &Cx) -> bool {
    tenant_id(cx) != Some(BLOCKED_TENANT)
}

pub struct PostResource;

impl Resource for PostResource {
    type Model = Post;
    type Form = PostForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .label("Blog Post")
            .icon(tablo::ui::icons::FILE_TEXT)
            .navigation_group("Blog")
            // The drafts waiting in the current blog.
            .navigation_badge(Post::fields().status().eq(PostStatus::Draft))
            .policy(when(blog_open))
            .tenancy(Tenancy::column(lens!(Post.tenant_id)))
            .table(post_table())
            .form(post_form())
            .detail(post_detail())
            // Names the detail heading with the post title.
            .record_title(lens!(Post.title))
            // Links a published post's public page.
            .public_link(|_cx: &Cx, post: &Post| {
                (post.status == PostStatus::Published).then(|| PublicLink {
                    url: format!("/blog/{}", post.id),
                    label: "View public post",
                })
            })
            // Lists the post's comments.
            .relation(Relation::has_many::<CommentResource>(
                Comment::fields().post_id(),
            ))
            // Publishes drafts from a row, the detail and edit pages, or for the selection.
            .action::<PublishPosts>()
            // Adds or replaces tags, asking which first.
            .action::<TagPosts>()
            // Publishes every draft from the list's header, confirming first.
            .header_action::<PublishAllDrafts>()
    }
}

/// The post form: content, details, SEO and publication.
fn post_form() -> Schema<PostForm> {
    let c = PostForm::controls();
    Schema::new((
        Section::new("Content").schema((
            c.title.placeholder("A title editors click"),
            c.body.multiline(6).placeholder("The full story…"),
        )),
        Section::new("Details").schema((
            Grid::new(2).schema((c.status, c.featured)),
            c.author_id.searchable().label("Author"),
            c.cover_id.searchable().label("Cover"),
            c.categories.searchable(),
            c.tags,
        )),
        Group::new().schema((
            Section::new("SEO").schema(c.seo),
            Section::new("Publication").schema(c.publication),
        )),
        Section::new("Further reading").schema(c.links.add_label("Add link")),
    ))
}

/// The post detail page.
fn post_detail() -> Detail<Post> {
    Detail::new((
        Section::new("Post").columns((
            TextColumn::new(lens!(Post.title)),
            TextColumn::new(lens!(Post.body)),
            ComputedColumn::new("Reading", |p: &Post| {
                let words = word_count(&p.body);
                format!("{words} words · {} min read", read_minutes(words))
            }),
        )),
        Section::new("Details").columns(Group::new().columns((
            Grid::new(2).columns((
                TextColumn::new(lens!(Post.status)),
                BooleanColumn::new(lens!(Post.featured)),
            )),
            post_author_column(),
            RelationColumn::list::<CategoryResource>(relation!(Post.categories)),
            TextColumn::new(lens!(Post.tags)),
        ))),
        Section::new("SEO").columns(EmbeddedColumn::new(lens!(Post.seo))),
        Section::new("Publication").columns(EmbeddedColumn::new(lens!(Post.publication))),
        Section::new("Further reading").columns(RepeaterColumn::new(lens!(Post.links))),
    ))
}

/// The post's author, by name.
fn post_author_column() -> RelationColumn<Post> {
    RelationColumn::of::<AuthorResource>(relation!(Post.author))
}

/// The post list.
fn post_table() -> Table<Post> {
    Table::new((
        TextColumn::new(lens!(Post.title)).searchable().sortable(),
        // Each reader picks whether the status and the creation date show.
        TextColumn::new(lens!(Post.status)).badge().toggleable(),
        BooleanColumn::new(lens!(Post.featured)),
        post_author_column(),
        CountColumn::new(relation!(Post.comments)),
        DateColumn::new(lens!(Post.created_at)).hidden_by_default(),
    ))
    // Publish and Tag join View, Edit and Delete behind one button.
    .action_layout(ActionLayout::Menu)
    .filters((
        SelectFilter::of(lens!(Post.status)).multiple(),
        TernaryFilter::new(lens!(Post.featured)).labels("Featured", "Not featured"),
        DateFilter::new(lens!(Post.created_at))
            .label("Created")
            .range(),
        SelectFilter::relationship::<AuthorResource>(lens!(Post.author_id)).label("Author"),
        QueryFilter::new("promoted", "Promoted")
            .option(
                "Promoted",
                Post::fields()
                    .featured()
                    .eq(true)
                    .and(Post::fields().status().eq(PostStatus::Published)),
            )
            .option(
                "Backlog",
                Post::fields()
                    .featured()
                    .eq(false)
                    .and(Post::fields().status().eq(PostStatus::Draft)),
            ),
    ))
    .group_by(lens!(Post.status))
}

/// Publishes draft posts.
pub struct PublishPosts;

impl Action<PostResource> for PublishPosts {
    type Input = ();
    const NAME: &'static str = "publish";

    fn can_run(_cx: &Cx, post: &Post) -> bool {
        post.status != PostStatus::Published
    }

    async fn run(_cx: &Cx, posts: &[Post], _: (), ex: &mut dyn toasty::Executor) -> Result<()> {
        for post in posts {
            Post::filter(Post::fields().id().eq(post.id))
                .update()
                .status(PostStatus::Published)
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }
}

/// Publishes every draft of the tenant, from the post list's header.
pub struct PublishAllDrafts;

impl HeaderAction for PublishAllDrafts {
    type Input = ();
    const NAME: &'static str = "publish-all-drafts";
    const CONFIRM: bool = true;

    async fn run(cx: &Cx, _: (), ex: &mut dyn toasty::Executor) -> Result<()> {
        // The tenant-scoped query: a header action sees no record, so it scopes its own.
        let drafts = scoped_query::<PostResource>(cx)?
            .filter(Post::fields().status().eq(PostStatus::Draft))
            .exec(&mut *ex)
            .await?;
        for post in drafts {
            Post::filter(Post::fields().id().eq(post.id))
                .update()
                .status(PostStatus::Published)
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }
}

/// What tagging asks for: the tags, and whether they replace the post's own.
#[derive(tablo::FormInput)]
pub struct Tagging {
    #[form(label = "Tags", placeholder = "rust, async")]
    pub tags: String,
    #[form(label = "Replace existing tags")]
    pub replace: bool,
}

/// Adds tags to posts, or replaces theirs, from a row or for the selection.
pub struct TagPosts;

impl Action<PostResource> for TagPosts {
    type Input = Tagging;
    const NAME: &'static str = "tag";

    async fn run(
        _cx: &Cx,
        posts: &[Post],
        tagging: Tagging,
        ex: &mut dyn toasty::Executor,
    ) -> Result<()> {
        for post in posts {
            let kept = if tagging.replace {
                ""
            } else {
                post.tags.as_str()
            };
            Post::filter(Post::fields().id().eq(post.id))
                .update()
                .tags(merge_tags(kept, &tagging.tags))
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }
}

/// `added`'s comma-separated tags after `kept`'s, trimmed, each once, in first-seen order.
fn merge_tags(kept: &str, added: &str) -> String {
    let mut tags: Vec<&str> = Vec::new();
    for tag in kept.split(',').chain(added.split(',')).map(str::trim) {
        if !tag.is_empty() && !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags.join(",")
}

#[derive(tablo::RecordForm)]
#[form(model = Post)]
pub struct PostForm {
    pub title: String,
    #[form(optional)]
    pub body: String,
    #[form(options, blank = PostStatus::Draft)]
    pub status: PostStatus,
    pub featured: bool,
    #[form(relationship = AuthorResource)]
    pub author_id: uuid::Uuid,
    #[form(relationship = MediaLibrary)]
    pub cover_id: Option<uuid::Uuid>,
    #[form(relationship = CategoryResource)]
    pub categories: Vec<uuid::Uuid>,
    #[form(optional)]
    pub tags: String,
    #[form(embed)]
    pub seo: Seo,
    #[form(embed)]
    pub publication: Publication,
    #[form(repeat)]
    pub links: Vec<Link>,
}

/// Files posts by topic.
pub struct CategoryResource;

impl Resource for CategoryResource {
    type Model = Category;
    type Form = CategoryForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .plural_label("Categories")
            .icon(tablo::ui::icons::TAG)
            .navigation_group("Blog")
            .policy(when(blog_open))
            .tenancy(Tenancy::column(lens!(Category.tenant_id)))
            .record_title(lens!(Category.name))
            // Lists the category's posts, attaching and detaching them.
            .relation(Relation::belongs_to_many::<PostResource>(
                Category::fields().posts(),
            ))
    }
}

/// Moderates comments.
pub struct CommentResource;

impl Resource for CommentResource {
    type Model = Comment;
    type Form = CommentForm;

    fn declare() -> ResourceDef<Self> {
        let c = CommentForm::controls();
        ResourceDef::new()
            .plural_label("Comments")
            .icon(tablo::ui::icons::MESSAGE_SQUARE)
            .navigation_group("Blog")
            // Hides removed comments.
            .policy_fn(|_cx, ability: Ability<'_, Comment>| match ability {
                Ability::View(comment) => comment.body != REMOVED_COMMENT_BODY,
                _ => true,
            })
            .tenancy(Tenancy::via(Comment::fields().post().tenant_id()))
            // A cursor-paged table: Previous and Next by row, without a count.
            .table(
                Table::new((
                    TextColumn::new(lens!(Comment.body)).searchable().sortable(),
                    comment_post_column(),
                ))
                .pagination(Pagination::Cursor),
            )
            .detail(Detail::new(Section::new("Comment").columns((
                TextColumn::new(lens!(Comment.body)),
                comment_post_column(),
            ))))
            .form(Schema::new(Section::new("Comment").schema((
                c.body.multiline(4).placeholder("Write a reply…"),
                c.post_id.searchable().label("Post"),
            ))))
            .record_title(lens!(Comment.body))
    }
}

/// The comment's post, by title.
fn comment_post_column() -> RelationColumn<Comment> {
    RelationColumn::of::<PostResource>(relation!(Comment.post))
}

#[derive(tablo::RecordForm)]
#[form(model = Comment)]
pub struct CommentForm {
    pub body: String,
    #[form(relationship = PostResource)]
    pub post_id: uuid::Uuid,
}

pub fn router(db: Db) -> Router {
    build_router(db, Some(load_assets()), Some(upload_dir()))
}

/// The directory uploaded bytes are written to.
pub fn upload_dir() -> PathBuf {
    std::env::var_os("SHOWCASE_UPLOAD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/showcase-uploads"))
}

/// Where the media library writes: the directory the panel stores file fields' uploads in and
/// serves, installed on a router built with one.
pub struct MediaStore(pub DirUploader);

/// The URL prefix uploads are served under.
pub const UPLOAD_URL_PREFIX: &str = "/uploads";

/// The brand's mark, an inline SVG: the sidebar logo and the favicon.
const LOGO: &str = "data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20viewBox='0%200%2024%2024'%3E%3Ccircle%20cx='12'%20cy='12'%20r='10'%20fill='%236366f1'/%3E%3Ctext%20x='12'%20y='16'%20text-anchor='middle'%20font-size='12'%20fill='white'%20font-family='sans-serif'%3EA%3C/text%3E%3C/svg%3E";

/// The admin panel's resources and pages: what the router mounts, and what a test builds its
/// handle from with [`Panel::handle`].
pub fn admin_panel() -> Panel {
    Panel::new("admin")
        .auth(
            Auth::custom(StaffAuth)
                .registration(StaffAuth)
                .profile(StaffAuth)
                .tenant_registration(TenantRegistration::new(StaffAuth))
                .tenant_profile(StaffAuth)
                .two_factor(TwoFactor::new())
                .mail(AuthMail::new(
                    Mailbox::named("Tablo Blog", "no-reply@example.com")
                        .expect("the showcase's sender address parses"),
                    std::env::var("SHOWCASE_BASE_URL")
                        .unwrap_or_else(|_| "http://localhost:3000".to_string()),
                ))
                .password_reset(PasswordReset::new(StaffAuth))
                .email_verification(EmailVerification::new(StaffAuth)),
        )
        .brand(Brand::new("Tablo Blog").logo(LOGO).favicon(LOGO))
        .home::<Dashboard>()
        .resource::<UserResource>()
        .resource::<AuthorResource>()
        .resource::<PostResource>()
        .resource::<CategoryResource>()
        .resource::<CommentResource>()
        .page::<MediaLibraryPage>()
        // The public blog, from the user menu.
        .user_menu_item(
            NavigationItem::at("View the blog", "/blog").icon(tablo::ui::icons::EXTERNAL_LINK),
        )
}

pub fn build_router(db: Db, bundle: Option<AssetBundle>, uploads: Option<PathBuf>) -> Router {
    let mut panel = admin_panel();
    if let Ok(hint) = std::env::var("SHOWCASE_LOGIN_HINT")
        && !hint.trim().is_empty()
    {
        panel = panel.login_hint(hint);
    }
    // Reset links land in `target/mail` as `.eml` files instead of an inbox.
    let mut builder = Router::builder()
        .discover()
        .app_context(db)
        // Seals two-factor secrets. An app loads a fixed key from a secret; the showcase's
        // in-memory database forgets every sealed secret with the key at each start.
        .app_context(Key::generate())
        .mail(MailConfig::builder().transport(mail_transport()).build());
    if let Some(dir) = uploads {
        builder = builder.app_context(MediaStore(DirUploader::new(UPLOAD_URL_PREFIX, dir.clone())));
        panel = panel.uploads_dir(UPLOAD_URL_PREFIX, dir);
    }
    if let Some(bundle) = bundle {
        builder = builder.assets(bundle);
        panel = panel.shell_assets(tailwind::stylesheet!(), GEIST);
    }
    builder.panel(panel).expect("showcase panel mounts").build()
}

/// The directory reset links are written to, unless `SHOWCASE_MAIL_DIR` names another.
fn mail_transport() -> FileTransport {
    FileTransport::new(
        std::env::var_os("SHOWCASE_MAIL_DIR")
            .map_or_else(|| PathBuf::from("target/mail"), PathBuf::from),
    )
}

/// `cargo run` builds the binary but not the bundle beside it; the Topcoat CLI builds both.
const RUN_HINT: &str = "run the showcase with `topcoat dev -p showcase`";

fn load_assets() -> AssetBundle {
    match AssetBundle::load() {
        Ok(bundle) => bundle,
        Err(near_executable) => {
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
                        "showcase asset bundle is unavailable: executable lookup failed ({near_executable}); tried {} ({test_error}); {RUN_HINT}",
                        dir.display()
                    )
                }),
                None => panic!(
                    "showcase asset bundle is unavailable: executable lookup failed ({near_executable}); {RUN_HINT}"
                ),
            }
        }
    }
}
