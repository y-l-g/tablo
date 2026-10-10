use tablo_core::{Ability, Action, NoForm, Resource};
use topcoat::{Result, context::Cx};

#[derive(Debug, Clone, toasty::Model)]
struct Post {
    #[key]
    #[auto]
    id: uuid::Uuid,
    published: bool,
}

struct PostResource;

impl Resource for PostResource {
    type Model = Post;
    type Form = NoForm<Post>;
}

struct CommentResource;

impl Resource for CommentResource {
    type Model = Post;
    type Form = NoForm<Post>;
}

struct Publish;

impl Action<PostResource> for Publish {
    type Input = ();
    const NAME: &'static str = "publish";

    async fn run(_: &Cx, _: &[Post], _: (), _: &mut dyn toasty::Executor) -> Result<()> {
        Ok(())
    }
}

fn post_policy(ability: Ability<'_, Post>) -> bool {
    // `Publish` is an action of `PostResource`, not of `CommentResource`.
    ability.is_action::<Publish, CommentResource>()
}

fn main() {
    let _ = post_policy;
}
