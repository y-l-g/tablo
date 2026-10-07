//! Serves the showcase panel with demo resources.

use std::path::PathBuf;

use tablo::{
    Ability, Action, Auth, BooleanColumn, Brand, ColumnWidth, Committed, ComputedColumn,
    DateFilter, Field, FieldErrors, Grid, Group, Options, Panel, PublicLink, QueryFilter,
    RecordForm, Relation, Resource, ResourceDef, RouterBuilderPanelExt, Schema, Section,
    SelectFilter, Table, Tenancy, TernaryFilter, TextColumn, Uploader, lens, tenant_id, when,
};
use toasty::Db;
use topcoat::{
    Result,
    asset::{AssetBundle, RouterBuilderAssetExt},
    context::Cx,
    font::{Font, fontsource::fontsource_font},
    router::{Router, RouterBuilderDiscoverExt},
    tailwind,
    view::{ViewExt, view},
};

use crate::{
    dashboard::Dashboard,
    live::LiveActivityPage,
    media::{MediaLibrary, MediaLibraryPage},
    models::{
        Author, BLOCKED_TENANT, Comment, MediaAsset, Post, PostStatus, Publication,
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
    }

    /// Refuses a negative age.
    fn validate_record(_cx: &Cx, form: &UserForm) -> FieldErrors<UserFormField> {
        let mut errors = FieldErrors::new();
        if form.age < 0 {
            errors.add(UserFormField::Age, "Age must be zero or more");
        }
        errors
    }

    /// Wakes the live feed after a committed write.
    async fn after_commit(_cx: &Cx, _committed: Committed<User>) -> Result<()> {
        crate::live::notify();
        Ok(())
    }
}

#[derive(tablo::RecordForm)]
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

    fn declare() -> ResourceDef<Self> {
        let c = AuthorForm::controls();
        ResourceDef::new()
            .label("Writer")
            .icon(tablo::ui::icons::PEN_LINE)
            .policy(when(blog_open))
            .tenancy(Tenancy::column(Author::fields().tenant_id()))
            .table(AuthorForm::table())
            .form(Schema::new((c.name, c.email.email())))
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
            .policy(when(blog_open))
            .tenancy(Tenancy::column(Post::fields().tenant_id()))
            .table(post_table())
            .form(post_form())
            .view(post_view())
            // Lists the post's comments.
            .relation(Relation::has_many::<CommentResource>(
                Comment::fields().post_id(),
            ))
            // Publishes drafts from a row or for the selection.
            .action::<PublishPosts>()
    }

    /// Names the detail heading with the post title.
    fn record_label(_cx: &Cx, record: &Post) -> Option<String> {
        Some(record.title.clone())
    }

    /// Links the post's public page.
    fn public_link(_cx: &Cx, record: &Post) -> Option<PublicLink> {
        if record.status == crate::blog::PUBLISHED {
            Some(PublicLink {
                url: format!("/blog/{}", record.id),
                label: "View public post",
            })
        } else {
            None
        }
    }

    /// Shows the computed reading stats and the cover.
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
}

/// The post form: content, details, SEO and publication.
fn post_form() -> Schema {
    let c = PostForm::controls();
    Schema::new((
        Section::new("Content").schema((
            c.title.placeholder("A title editors click"),
            c.body.multiline(6).placeholder("The full story…"),
        )),
        Section::new("Details").schema((
            Grid::new(2).schema((c.status, c.featured)),
            c.author_id
                .relationship::<AuthorResource>(|a: &Author| a.name.clone())
                .searchable()
                .label("Author"),
            c.cover_id
                .relationship::<MediaLibrary>(|m: &MediaAsset| m.filename.clone())
                .searchable()
                .label("Cover"),
            c.tags,
        )),
        Group::new().schema((
            Section::new("SEO").schema(c.seo),
            Section::new("Publication").schema(c.publication),
        )),
    ))
}

/// The post detail page.
fn post_view() -> Schema {
    let c = PostForm::controls();
    Schema::new((
        Section::new("Post").schema((c.title, c.body.multiline(6))),
        Section::new("Details")
            .schema(Group::new().schema((Grid::new(2).schema((c.status, c.featured)), c.tags))),
        Section::new("SEO").schema(c.seo),
        Section::new("Publication").schema(
            Field::text(Post::fields().publication().published().published_at())
                .label("Published at"),
        ),
    ))
}

/// The post list.
fn post_table() -> Table<Post> {
    Table::new((
        TextColumn::new(lens!(Post.title)).searchable().sortable(),
        TextColumn::new(lens!(Post.status))
            .format(|status| PostStatus::label_of(status))
            .width(ColumnWidth::Narrow),
        BooleanColumn::new(lens!(Post.featured)),
        ComputedColumn::new("Author", |p: &Post| {
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
        ComputedColumn::new("Comments", |p: &Post| {
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
    ))
    .filters((
        SelectFilter::new(Post::fields().status(), PostStatus::options()),
        TernaryFilter::new(Post::fields().featured()),
        DateFilter::new(Post::fields().created_at()),
        QueryFilter::new("promoted", "Promoted")
            .option(
                "Promoted",
                Post::fields().featured().eq(true).and(
                    Post::fields()
                        .status()
                        .eq(PostStatus::Published.value().to_string()),
                ),
            )
            .option(
                "Backlog",
                Post::fields().featured().eq(false).and(
                    Post::fields()
                        .status()
                        .eq(PostStatus::Draft.value().to_string()),
                ),
            ),
    ))
    .group_by(lens!(Post.status))
}

/// Publishes draft posts.
pub struct PublishPosts;

impl Action<PostResource> for PublishPosts {
    const NAME: &'static str = "publish";

    fn label(_cx: &Cx) -> String {
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

#[derive(tablo::RecordForm)]
#[form(model = Post)]
pub struct PostForm {
    pub title: String,
    #[form(optional)]
    pub body: String,
    #[form(options = PostStatus, blank = PostStatus::Draft.value())]
    pub status: String,
    pub featured: bool,
    #[form(choice)]
    pub author_id: uuid::Uuid,
    #[form(choice)]
    pub cover_id: Option<uuid::Uuid>,
    #[form(optional)]
    pub tags: String,
    #[form(embed)]
    pub seo: Seo,
    #[form(embed)]
    pub publication: Publication,
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
            // Hides removed comments.
            .policy(|_cx: &Cx, ability: Ability<'_, Comment>| match ability {
                Ability::View(comment) => comment.body != REMOVED_COMMENT_BODY,
                _ => true,
            })
            .tenancy(Tenancy::via(Comment::fields().post().tenant_id()))
            .table(Table::new((
                TextColumn::new(lens!(Comment.body)).searchable().sortable(),
                ComputedColumn::new("Post", |c: &Comment| {
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
                .width(ColumnWidth::Wide)
                .include(Comment::fields().post()),
            )))
            .form(Schema::new((
                c.body.multiline(4).placeholder("Write a reply…"),
                c.post_id
                    .relationship::<PostResource>(|p: &Post| p.title.clone())
                    .searchable()
                    .label("Post"),
            )))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Comment)]
pub struct CommentForm {
    pub body: String,
    #[form(choice)]
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

/// The admin panel's resources and pages: what the router mounts, and what a background job builds
/// its context from with [`Panel::context`].
pub fn admin_panel() -> Panel {
    Panel::new("admin")
        .auth(Auth::custom(StaffAuth))
        .brand(
            Brand::new("Tablo Blog").logo(
                "data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20viewBox='0%200%2024%2024'%3E%3Ccircle%20cx='12'%20cy='12'%20r='10'%20fill='%236366f1'/%3E%3Ctext%20x='12'%20y='16'%20text-anchor='middle'%20font-size='12'%20fill='white'%20font-family='sans-serif'%3EA%3C/text%3E%3C/svg%3E",
            ),
        )
        .home::<Dashboard>()
        .resource::<UserResource>()
        .resource::<AuthorResource>()
        .resource::<PostResource>()
        .resource::<CommentResource>()
        .page::<MediaLibraryPage>()
        .page::<LiveActivityPage>()
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
