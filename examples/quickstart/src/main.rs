//! The smallest complete Tablo app: one model, one resource, the panel that
//! serves it, and the layout that frames it in the admin shell.
//!
//! `cargo run`, then open <http://127.0.0.1:3000/admin/books> and sign in as
//! `admin@example.com` / `secret`.

use tablo::{
    auth::{AdminUser, AuthSession, hash_password},
    prelude::*,
};
use toasty::Db;
use topcoat::{
    Result,
    asset::AssetBundle,
    context::Cx,
    font::{Font, fontsource::fontsource_font},
    router::{Slot, layout},
    tailwind,
    view::View,
};

/// The shell's sans font, self-hosted as a Topcoat asset.
const GEIST: Font = fontsource_font!(GEIST, host: Asset);

/// The persisted model: every column the panel renders comes from this type.
#[derive(Debug, Clone, toasty::Model)]
pub struct Book {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub title: String,
}

/// What the create and edit forms parse into.
#[derive(tablo::RecordForm)]
#[form(model = Book)]
pub struct BookForm {
    pub title: String,
}

pub struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = BookForm;

    fn can_view_any(_cx: &Cx) -> bool {
        true
    }

    fn can_view(_cx: &Cx, _record: &Book) -> bool {
        true
    }

    fn can_create(_cx: &Cx) -> bool {
        true
    }

    fn can_update(_cx: &Cx, _record: &Book) -> bool {
        true
    }

    fn table() -> Table<Book> {
        Table::new(
            |b: &Book| b.id.to_string(),
            TextColumn::r#for(Book::fields().title(), |b: &Book| b.title.clone())
                .searchable()
                .sortable(),
        )
    }

    fn form(_dx: &tablo::DeclCx) -> Schema {
        Schema::new(Field::text(Book::fields().title()))
    }
}

/// Frames every page under the panel prefix in the admin shell.
#[layout("/admin")]
async fn admin_layout(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    Panel::layout_shell(cx, slot).await
}

/// The panel, before assets: `main` adds the bundle, the tests build it bare.
fn panel(db: Db) -> Panel {
    Panel::new("admin")
        .app_context(db)
        .resource::<BookResource>()
}

/// An in-memory database holding the app's model and the two tables the
/// shipped password auth reads.
async fn connect() -> Result<Db> {
    let db = Db::builder()
        .models(toasty::models!(Book, AdminUser, AuthSession))
        .connect("sqlite::memory:")
        .await?;
    db.push_schema().await?;
    Ok(db)
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut db = connect().await?;
    toasty::create!(AdminUser {
        email: "admin@example.com".to_string(),
        password_hash: hash_password("secret")?,
        display_name: "Admin".to_string(),
        active: true,
        tenant_id: None,
        created_at: jiff::Timestamp::now(),
    })
    .exec(&mut db)
    .await?;

    // The bundle sits beside a binary Topcoat's bundler built; a plain
    // `cargo run` has none and serves the shell without its stylesheet.
    let router = match AssetBundle::load() {
        Ok(bundle) => panel(db)
            .assets(bundle)
            .shell_assets(tailwind::stylesheet!(), GEIST)
            .build()?,
        Err(error) => {
            eprintln!("no asset bundle ({error}): serving the unstyled shell");
            panel(db).build()?
        }
    };
    topcoat::start(router).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tablo::testing::TestClient;

    use super::{connect, panel};

    /// The stylesheet `build.rs` generated.
    const STYLESHEET: &str = include_str!(concat!(env!("OUT_DIR"), "/tailwind.css"));

    #[test]
    fn the_stylesheet_holds_classes_only_tablo_writes() {
        // One class written only in `tablo-core` and one only in `tablo-ui`:
        // each is in the stylesheet only if the build scanned that crate's
        // sources where Cargo put them. The names are split so this file,
        // which the build scans too, never spells them whole.
        for (stem, tail) in [("table-", "fixed"), ("tabular-", "nums")] {
            let rule = format!(".{stem}{tail}{{");
            assert!(STYLESHEET.contains(&rule), "{stem}{tail} is missing");
        }
    }

    #[tokio::test]
    async fn the_panel_serves_its_login_page_and_gates_the_list() {
        let router = panel(connect().await.expect("connect"))
            .build()
            .expect("the panel builds");
        let client = TestClient::new(&router);

        let login = client.get("/admin/login").await;
        assert_eq!(login.status(), 200);

        let list = client.get("/admin/books").await;
        assert!(
            list.status().is_redirection(),
            "an anonymous request is sent to the login page, got {}",
            list.status()
        );
    }
}
