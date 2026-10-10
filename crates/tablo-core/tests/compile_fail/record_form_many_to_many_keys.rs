#[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
struct Post {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[has_many]
    taggings: toasty::Deferred<Vec<Tagging>>,
    #[has_many(via = taggings.tag)]
    tags: toasty::Deferred<Vec<Tag>>,
}

#[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
struct Tag {
    #[key]
    #[auto]
    id: uuid::Uuid,
}

#[derive(Debug, Clone, toasty::Model)]
struct Tagging {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[index]
    post_id: uuid::Uuid,
    #[belongs_to(key = post_id, references = id)]
    post: toasty::Deferred<Post>,
    #[index]
    tag_id: uuid::Uuid,
    #[belongs_to(key = tag_id, references = id)]
    tag: toasty::Deferred<Tag>,
}

struct TagResource;

impl tablo_core::Resource for TagResource {
    type Model = Tag;
    type Form = tablo_core::NoForm<Tag>;
}

#[derive(tablo_core::RecordForm)]
#[form(model = Post)]
struct PostForm {
    #[form(relationship = TagResource)]
    tags: Vec<i64>,
}

fn main() {}
