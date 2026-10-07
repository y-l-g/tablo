//! The public blog: `/blog` and `/blog/{id}`, served with no session.

use tablo::{Panel, db::db};
use toasty::stmt::Include;
use topcoat::{
    Result,
    context::Cx,
    router::{
        Slot,
        error::{RouterErrorExt, not_found},
        href, layout, page, path_param,
    },
    view::{View, class, view},
};

use crate::models::{Author, MediaAsset, Post, PostStatus};

// The status marking a post visible to the public.
pub(crate) const PUBLISHED: &str = PostStatus::Published.value();

path_param!(pub id: uuid::Uuid);

/// The public shell.
#[layout("/blog")]
async fn blog_layout(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    Panel::document(
        cx,
        "Tablo Blog",
        view! {
            <div class="flex min-h-screen flex-col bg-background text-foreground">
                <header class="border-b border-border">
                    <nav
                        class="mx-auto flex w-full max-w-3xl items-center gap-6 px-6 py-4"
                    >
                        <a
                            let href = href!(page);
                            let current = href.is_current(cx);
                            href=(href)
                            aria-current=(current.then_some("page"))
                            class=(class!(
                                "font-semibold",
                                "text-foreground" if current,
                                "text-muted-foreground hover:text-foreground" if !current,
                            ))
                        >
                            "Tablo Blog"
                        </a>
                        if let Some(admin) = tablo::url::panel(cx) {
                            <a
                                href=(admin)
                                class="ml-auto text-sm text-muted-foreground hover:text-foreground"
                            >
                                "Admin"
                            </a>
                        }
                    </nav>
                </header>

                <main class="mx-auto w-full max-w-3xl flex-1 px-6 py-10">(slot)</main>

                <footer class="border-t border-border">
                    <p
                        class="mx-auto w-full max-w-3xl px-6 py-4 text-sm text-muted-foreground"
                    >
                        "Published with Tablo."
                    </p>
                </footer>
            </div>
        },
    )
    .await
}

/// The list: published posts only, newest first, each linking to its page.
#[page("/blog")]
async fn page(cx: &Cx) -> Result<impl View> {
    let mut db = db(cx);
    // N+1 touch: the author is included.
    let include_author: Include<Post, Author> = Post::fields().author().into();
    let posts = Post::filter(Post::fields().status().eq(PUBLISHED.to_string()))
        .order_by(Post::fields().created_at().desc())
        .include(include_author)
        .exec(&mut db)
        .await?;

    Ok(view! {
        <h1 class="text-3xl font-bold tracking-tight">"Blog"</h1>

        if posts.is_empty() {
            <p class="mt-6 text-muted-foreground">
                "No posts have been published yet."
            </p>
        } else {
            <ul class="mt-8 flex flex-col gap-8">
                for post in &posts {
                    <li>
                        <article>
                            <h2 class="text-xl font-semibold tracking-tight">
                                <a
                                    href=(href!(post_page, Id(post.id)))
                                    class="hover:underline"
                                >
                                    (&post.title)
                                </a>
                            </h2>
                            <p class="mt-1 text-sm text-muted-foreground">
                                (author_name(post))
                                " · "
                                (post.created_at.strftime("%Y-%m-%d").to_string())
                            </p>
                            if !post.seo.description.is_empty() {
                                <p class="mt-3 text-muted-foreground">
                                    (&post.seo.description)
                                </p>
                            }
                        </article>
                    </li>
                }
            </ul>
        }
    })
}

/// Renders one published post; drafts answer not-found.
#[page("/blog/{id}")]
async fn post_page(cx: &Cx) -> Result<impl View> {
    // A malformed id answers not-found.
    let id = path_param::<Id>(cx).map_err(|_| not_found())?.to_owned();

    let mut db = db(cx);
    let include_author: Include<Post, Author> = Post::fields().author().into();
    let post = Post::filter(
        Post::fields()
            .id()
            .eq(id)
            .and(Post::fields().status().eq(PUBLISHED.to_string())),
    )
    .include(include_author)
    .first()
    .exec(&mut db)
    .await?
    .ok_or_not_found()?;

    let cover = match post.cover_id {
        Some(cover_id) => {
            MediaAsset::filter(MediaAsset::fields().id().eq(cover_id))
                .first()
                .exec(&mut db)
                .await?
        }
        None => None,
    };

    Ok(view! {
        <a
            href=(href!(page))
            class="text-sm text-muted-foreground hover:text-foreground"
        >
            "← All posts"
        </a>

        <article class="mt-4">
            <h1 class="text-3xl font-bold tracking-tight">(&post.title)</h1>
            <p class="mt-2 text-sm text-muted-foreground">
                (author_name(&post))
                " · "
                (post.created_at.strftime("%Y-%m-%d").to_string())
            </p>

            if !post.seo.description.is_empty() {
                <p class="mt-6 text-lg text-muted-foreground">
                    (&post.seo.description)
                </p>
            }

            if let Some(asset) = cover.as_ref().and_then(|asset| cover_url(asset)) {
                <img
                    src=(asset.to_string())
                    alt=(post.title.clone())
                    class="mt-8 w-full rounded-lg border border-border"
                >
            }

            <div class="mt-8 leading-7">(&post.body)</div>
        </article>
    })
}

/// Reads the author's display name.
fn author_name(post: &Post) -> String {
    if post.author.is_unloaded() {
        "(unknown author)".to_string()
    } else {
        post.author.get().name.clone()
    }
}

/// Returns the cover URL when the row names a servable path.
fn cover_url(asset: &MediaAsset) -> Option<&str> {
    let path = asset.path.trim();
    path.starts_with('/').then_some(path)
}
