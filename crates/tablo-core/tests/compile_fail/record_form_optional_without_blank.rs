#[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
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
    #[form(optional)]
    age: i64,
}

fn main() {}
