#[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
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
