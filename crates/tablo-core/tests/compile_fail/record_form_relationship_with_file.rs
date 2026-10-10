#[derive(Debug, Clone, toasty::Model)]
struct Post {
    #[key]
    #[auto]
    id: uuid::Uuid,
    author_id: String,
}

struct Authors;

#[derive(tablo_core::RecordForm)]
#[form(model = Post)]
struct PostForm {
    #[form(relationship = Authors, file)]
    author_id: String,
}

fn main() {}
