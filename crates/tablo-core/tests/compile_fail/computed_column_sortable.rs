#[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
struct User {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
}

fn main() {
    tablo_core::ComputedColumn::new("Name", |_, u: &User| u.name.clone()).sortable();
}
