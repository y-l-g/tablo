#[derive(Debug, Clone, toasty::Model)]
struct User {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
}

fn main() {
    tablo_core::Field::text(User::fields().name()).options(["admin", "member"]);
}
