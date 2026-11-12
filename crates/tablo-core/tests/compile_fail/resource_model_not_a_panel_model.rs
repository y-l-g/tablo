use tablo_core::{NoForm, Resource};

#[derive(Debug, Clone, toasty::Model)]
struct Post {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
}

struct PostResource;

impl Resource for PostResource {
    type Model = Post;
    type Form = NoForm<Post>;
}

fn main() {}
