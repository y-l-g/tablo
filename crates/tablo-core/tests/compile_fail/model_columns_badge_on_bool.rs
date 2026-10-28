#[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
struct User {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[columns(badge)]
    active: bool,
}

fn main() {}
