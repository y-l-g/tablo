#[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
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
    age: i32,
}

fn main() {}
