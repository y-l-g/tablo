//! Serves the showcase panel with demo resources.

use std::path::PathBuf;

use tablo::{
    Ability, Action, Auth, BooleanColumn, Brand, ColumnWidth, ComputedColumn, CountColumn,
    DateFilter, Detail, EmbeddedColumn, FieldErrors, Grid, Group, HeaderAction, Panel, PublicLink,
    QueryFilter, RecordForm, Relation, RelationColumn, RepeaterColumn, Resource, ResourceDef,
    RouterBuilderPanelExt, Schema, Section, SelectFilter, Table, Tenancy, TernaryFilter,
    TextColumn, Uploader, lens, relation, scoped_query, tenant_id, when,
};
use toasty::Db;
use topcoat::{
    Result,
    asset::{AssetBundle, RouterBuilderAssetExt},
    context::Cx,
    font::{Font, fontsource::fontsource_font},
    router::{Router, RouterBuilderDiscoverExt},
    tailwind,
};

use crate::{
    dashboard::Dashboard,
    media::{MediaLibrary, MediaLibraryPage},
    models::{
        Author, BLOCKED_TENANT, Category, Comment, Link, Post, PostStatus, Publication,
        REMOVED_COMMENT_BODY, Role, Seo, User,
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
            .policy(|_cx: &Cx, ability: Ability<'_, User>| match ability {
                Ability::Update(user) | Ability::Delete(user) => user.name != "Ken Thompson",
                _ => true,
            })
            .table(
                UserForm::table().column(
                    TextColumn::new(lens!(User.created_at))
                        .format(|at| at.strftime("%Y-%m-%d").to_string())
                        .sortable()
                        .width(ColumnWidth::Rem(8)),
                ),
            )
            .form(Schema::new(Section::new("Profile").schema((
                c.name.placeholder("Ada Lovelace"),
                c.email.email().unique().placeholder("ada@example.com"),
                c.role,
                c.active,
                c.age,
            ))))
            .detail(Detail::new(
                Section::new("Profile").columns(<UserForm as RecordForm>::detail()),
            ))
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

#[derive(tablo::RecordForm)]
#[form(model = User)]
pub struct UserForm {
    pub name: String,
    pub email: String,
    #[form(options, blank = Role::Member)]
    pub role: Role,
    #[form(blank = true)]
    pub active: bool,
    #[form(blank = 0)]
    pub age: i64,
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
            .table(AuthorForm::table())
            .form(Schema::new(
                Section::new("Profile").schema((c.name, c.email.email())),
            ))
            .detail(Detail::new(
                Section::new("Profile").columns(<AuthorForm as RecordForm>::detail()),
            ))
            // Names the detail heading with the author's name.
            .record_title(lens!(Author.name))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Author)]
pub struct AuthorForm {
    pub name: String,
    pub email: String,
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
        TextColumn::new(lens!(Post.status)).width(ColumnWidth::Narrow),
        BooleanColumn::new(lens!(Post.featured)),
        post_author_column(),
        CountColumn::new(relation!(Post.comments)),
    ))
    .filters((
        SelectFilter::of(lens!(Post.status)),
        TernaryFilter::new(lens!(Post.featured)),
        DateFilter::new(lens!(Post.created_at)),
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
#[derive(tablo::ActionInput)]
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

#[derive(tablo::RecordForm)]
#[form(model = Category)]
pub struct CategoryForm {
    pub name: String,
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
            .policy(|_cx: &Cx, ability: Ability<'_, Comment>| match ability {
                Ability::View(comment) => comment.body != REMOVED_COMMENT_BODY,
                _ => true,
            })
            .tenancy(Tenancy::via(Comment::fields().post().tenant_id()))
            .table(Table::new((
                TextColumn::new(lens!(Comment.body)).searchable().sortable(),
                comment_post_column(),
            )))
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

/// The URL prefix uploads are served under.
pub const UPLOAD_URL_PREFIX: &str = "/uploads";

/// Writes uploaded bytes into the served directory.
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
        tokio::fs::create_dir_all(&self.dir)
            .await
            .map_err(|_| "the upload directory is not writable".to_string())?;
        tokio::fs::write(self.dir.join(&name), bytes)
            .await
            .map_err(|_| "the upload could not be written".to_string())?;
        Ok(format!("{UPLOAD_URL_PREFIX}/{}", url_segment(&name)))
    }

    /// Reports whether the served directory holds the file a URL names.
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
const MAX_BASENAME_BYTES: usize = 218;

/// Reduces a client-supplied filename to the stored basename.
pub(crate) fn basename(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let clean: String = base.chars().filter(|c| !c.is_control()).collect();
    let trimmed = clean.trim();
    if trimmed.len() <= MAX_BASENAME_BYTES {
        return trimmed.to_string();
    }
    let mut start = trimmed.len() - MAX_BASENAME_BYTES;
    while !trimmed.is_char_boundary(start) {
        start += 1;
    }
    trimmed[start..].to_string()
}

/// Encodes `name` as one URL path segment.
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

/// The admin panel's resources and pages: what the router mounts, and what a test builds its
/// context from with [`Panel::context`].
pub fn admin_panel() -> Panel {
    Panel::new("admin")
        .auth(Auth::custom(StaffAuth).registration(StaffAuth))
        .brand(
            Brand::new("Tablo Blog").logo(
                "data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20viewBox='0%200%2024%2024'%3E%3Ccircle%20cx='12'%20cy='12'%20r='10'%20fill='%236366f1'/%3E%3Ctext%20x='12'%20y='16'%20text-anchor='middle'%20font-size='12'%20fill='white'%20font-family='sans-serif'%3EA%3C/text%3E%3C/svg%3E",
            ),
        )
        .home::<Dashboard>()
        .resource::<UserResource>()
        .resource::<AuthorResource>()
        .resource::<PostResource>()
        .resource::<CategoryResource>()
        .resource::<CommentResource>()
        .page::<MediaLibraryPage>()
}

pub fn build_router(db: Db, bundle: Option<AssetBundle>, uploads: Option<PathBuf>) -> Router {
    let mut panel = admin_panel();
    if let Ok(hint) = std::env::var("SHOWCASE_LOGIN_HINT")
        && !hint.trim().is_empty()
    {
        panel = panel.login_hint(hint);
    }
    if let Some(dir) = uploads {
        panel = panel
            .serve_dir(format!("{UPLOAD_URL_PREFIX}/{{*file}}"), dir.clone())
            .uploads(DirUploader::new(dir));
    }
    let mut builder = Router::builder().discover().app_context(db);
    if let Some(bundle) = bundle {
        builder = builder.assets(bundle);
        panel = panel.shell_assets(tailwind::stylesheet!(), GEIST);
    }
    builder.panel(panel).expect("showcase panel mounts").build()
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

#[cfg(test)]
mod tests;
