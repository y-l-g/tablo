#[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
struct User {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
}

fn main() {
    tablo_core::Field::text(User::fields().name()).options(["admin", "member"]);
}
