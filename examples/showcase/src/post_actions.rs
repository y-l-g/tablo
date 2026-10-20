//! The post list's actions: publishing, from a row, for the selection or from the header, and
//! tagging.

use tablo::{Action, HeaderAction, scoped_query};
use topcoat::{Result, context::Cx};

use crate::{
    app::PostResource,
    models::{Post, PostStatus},
};

/// Publishes draft posts.
pub struct PublishPosts;

impl Action<PostResource> for PublishPosts {
    type Input = ();
    const NAME: &'static str = "publish";

    fn can_run(_cx: &Cx, post: &Post) -> bool {
        post.status != PostStatus::Published
    }

    async fn run(
        _cx: &Cx,
        posts: &[Post],
        _: (),
        ex: &mut dyn toasty::Executor,
    ) -> Result<tablo::Outcome> {
        for post in posts {
            Post::filter(Post::fields().id().eq(post.id))
                .update()
                .status(PostStatus::Published)
                .exec(&mut *ex)
                .await?;
        }
        Ok(tablo::Outcome::done())
    }
}

/// Publishes every draft of the tenant, from the post list's header.
pub struct PublishAllDrafts;

impl HeaderAction for PublishAllDrafts {
    type Input = ();
    const NAME: &'static str = "publish-all-drafts";
    const CONFIRM: bool = true;

    async fn run(cx: &Cx, _: (), ex: &mut dyn toasty::Executor) -> Result<tablo::Outcome> {
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
        Ok(tablo::Outcome::done())
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
    ) -> Result<tablo::Outcome> {
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
        Ok(tablo::Outcome::done())
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
