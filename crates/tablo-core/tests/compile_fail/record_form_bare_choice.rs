#[derive(Debug, Clone, toasty::Model)]
struct Post {
    #[key]
    #[auto]
    id: uuid::Uuid,
    author_id: uuid::Uuid,
}

#[derive(tablo_core::RecordForm)]
#[form(model = Post)]
struct PostForm {
    #[form(choice)]
    author_id: uuid::Uuid,
}

fn main() {}
