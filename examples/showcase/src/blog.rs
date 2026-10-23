//! The public blog: `/blog` and `/blog/{id}`, served with no session.

use tablo::db::db;
use toasty::stmt::Include;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        Slot,
        error::{RouterErrorExt, not_found},
        href, layout, page, path_param,
    },
    view::{Unescaped, View, class, view},
};

use crate::{
    app::{AdminPanel, LOGO},
    models::{Author, MediaAsset, Post, PostStatus},
};

path_param!(pub id: uuid::Uuid);

/// The public shell.
#[layout("/blog")]
async fn blog_layout(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    // Outside the admin prefix the request belongs to no panel, so the blog names it.
    let AdminPanel(admin) = app_context::<AdminPanel>(cx);
    admin
        .document(
            cx,
            "Tablo Blog",
            view! {
                <div class="flex min-h-screen flex-col bg-background text-foreground">
                    <header class="border-b border-border">
                        <nav
                            class="mx-auto flex w-full max-w-3xl items-center gap-3 px-6 py-4"
                        >
                            <a
                                let href = href!(page);
                                let current = href.is_current(cx);
                                href=(href)
                                aria-current=(current.then_some("page"))
                                class="flex items-center gap-2.5 font-semibold tracking-tight"
                            >
                                <img src=(LOGO) alt="" class="size-7">
                                "Tablo Blog"
                            </a>
                            if let Some(admin) = admin.url() {
                                <a
                                    href=(admin)
                                    class=(class!(
                                        "ml-auto",
                                        tablo::ui::button_variants(
                                            tablo::ui::ButtonVariant::Outline,
                                            tablo::ui::ButtonSize::Sm,
                                        ),
                                    ))
                                >
                                    "Admin"
                                </a>
                            }
                        </nav>
                    </header>

                    <main class="mx-auto w-full max-w-3xl flex-1 px-6 py-12 sm:py-16">
                        (slot)
                    </main>

                    <footer class="border-t border-border">
                        <p
                            class="mx-auto w-full max-w-3xl px-6 py-6 text-sm text-muted-foreground"
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
    let posts = Post::filter(Post::fields().status().eq(PostStatus::Published))
        .order_by(Post::fields().created_at().desc())
        .include(include_author)
        .exec(&mut db)
        .await?;

    Ok(view! {
        <h1 class="text-4xl font-semibold tracking-tight">"Blog"</h1>
        <p class="mt-3 text-lg text-muted-foreground">
            "Notes on building admin panels in Rust."
        </p>

        if posts.is_empty() {
            <p class="mt-12 text-muted-foreground">
                "No posts have been published yet."
            </p>
        } else {
            <ul class="mt-12 divide-y divide-border border-y border-border">
                for post in &posts {
                    <li>
                        <article
                            class="grid gap-1 py-8 sm:grid-cols-[9rem_1fr] sm:gap-6"
                        >
                            <p
                                class="text-sm text-muted-foreground tabular-nums sm:pt-1"
                            >
                                (date(post))
                            </p>
                            <div>
                                <h2 class="text-xl font-semibold tracking-tight">
                                    <a
                                        href=(href!(post_page, Id(post.id)))
                                        class="hover:text-primary"
                                    >
                                        (&post.title)
                                    </a>
                                </h2>
                                if !post.seo.description.is_empty() {
                                    <p class="mt-2 text-muted-foreground">
                                        (&post.seo.description)
                                    </p>
                                }
                                <p class="mt-3 text-sm font-medium">(author_name(post))</p>
                            </div>
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
            .and(Post::fields().status().eq(PostStatus::Published)),
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

        <article class="mt-8">
            <p class="text-sm text-muted-foreground tabular-nums">
                (date(&post))
                " · "
                (author_name(&post))
            </p>
            <h1 class="mt-3 text-4xl font-semibold tracking-tight text-balance">
                (&post.title)
            </h1>

            if !post.seo.description.is_empty() {
                <p class="mt-4 text-xl leading-8 text-muted-foreground">
                    (&post.seo.description)
                </p>
            }

            if let Some(asset) = cover.as_ref().and_then(|asset| cover_url(asset)) {
                <img
                    src=(tablo::url::file(cx, asset))
                    alt=(post.title.clone())
                    class="mt-10 w-full rounded-xl border border-border"
                >
            }

            // `clean` keeps only the elements a rich text tool produces.
            <div
                class="mt-10 text-lg leading-8 [&_p]:my-5 [&_h2]:mt-10 [&_h2]:text-2xl [&_h2]:font-semibold [&_h2]:tracking-tight [&_h3]:mt-8 [&_h3]:text-xl [&_h3]:font-semibold [&_ul]:list-disc [&_ul]:pl-6 [&_ol]:list-decimal [&_ol]:pl-6 [&_blockquote]:border-l-2 [&_blockquote]:border-primary [&_blockquote]:pl-4 [&_blockquote]:text-muted-foreground [&_a]:text-primary [&_a]:underline [&_a]:underline-offset-4 [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:bg-muted [&_pre]:p-4 [&_pre]:text-sm"
            >
                (Unescaped::new_unchecked(tablo::rich_text::clean(&post.body)))
            </div>
        </article>
    })
}

/// The post's date, as a reader writes it.
fn date(post: &Post) -> String {
    post.created_at.strftime("%b %-d, %Y").to_string()
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
