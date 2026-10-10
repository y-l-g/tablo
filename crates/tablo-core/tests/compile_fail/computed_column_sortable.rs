#[derive(Debug, Clone, toasty::Model)]
struct User {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
}

fn main() {
    tablo_core::ComputedColumn::new("Name", |u: &User| u.name.clone()).sortable();
}
