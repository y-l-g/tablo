#[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
struct User {
    #[key]
    #[auto]
    id: uuid::Uuid,
    age: i64,
}

fn main() {
    tablo_core::TextColumn::new(tablo_core::lens!(User.age)).searchable();
}
