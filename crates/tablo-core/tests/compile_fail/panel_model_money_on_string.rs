#[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
struct User {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[columns(money = EUR)]
    name: String,
}

fn main() {}
