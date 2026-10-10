#[derive(Debug, Clone, toasty::Model)]
struct User {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
    age: i64,
}

#[derive(tablo_core::RecordForm)]
#[form(model = User)]
struct UserForm {
    nickname: String,
}

fn main() {}
