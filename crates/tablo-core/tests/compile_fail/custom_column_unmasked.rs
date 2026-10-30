//! A column outside tablo cannot claim it shows no field a related resource hides: the
//! framework's own column methods are sealed, so it answers their safe defaults.

#[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
struct User {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
}

struct Name;

impl tablo_core::extend::Column<User> for Name {
    fn name(&self) -> &str {
        "name"
    }

    fn label(&self) -> tablo_core::Text {
        "Name".into()
    }

    fn text(&self, _cx: &topcoat::context::Cx, row: &User) -> String {
        row.name.clone()
    }

    fn masked(&self, _cx: &topcoat::context::Cx) -> bool {
        false
    }
}

fn main() {}
