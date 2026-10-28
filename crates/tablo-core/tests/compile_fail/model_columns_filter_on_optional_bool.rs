#[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
struct User {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[columns(filter)]
    active: Option<bool>,
}

fn main() {}
