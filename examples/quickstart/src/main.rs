use tablo::{
    auth::{AdminUser, AuthSession, create_admin},
    prelude::*,
};
use toasty::Db;
use topcoat::{
    Result,
    asset::{AssetBundle, RouterBuilderAssetExt},
    context::Cx,
    font::{Font, fontsource::fontsource_font},
    router::{Router, RouterBuilder, RouterBuilderDiscoverExt},
    tailwind,
};

const GEIST: Font = fontsource_font!(GEIST, host: Asset);

#[derive(Debug, Clone, toasty::Model)]
pub struct Book {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub title: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Book)]
pub struct BookForm {
    pub title: String,
}

pub struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = BookForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().policy(|_cx: &Cx, ability: Ability<'_, Book>| {
            !matches!(ability, Ability::DeleteAny | Ability::Delete(_))
        })
    }
}

fn panel() -> Panel {
    Panel::new("admin").resource::<BookResource>()
}

fn router(db: Db) -> RouterBuilder {
    Router::builder().discover().app_context(db)
}

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
    create_admin(&mut db, "admin@example.com", "secret", "Admin").await?;

    let router = match AssetBundle::load() {
        Ok(bundle) => router(db)
            .assets(bundle)
            .panel(panel().shell_assets(tailwind::stylesheet!(), GEIST))?,
        Err(error) => {
            eprintln!("no asset bundle ({error}): serving the unstyled shell");
            router(db).panel(panel())?
        }
    };
    topcoat::start(router.build()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tablo::{RouterBuilderPanelExt, testing::TestClient};

    use super::{connect, panel, router};

    const STYLESHEET: &str = include_str!(concat!(env!("OUT_DIR"), "/tailwind.css"));

    #[test]
    fn the_stylesheet_holds_classes_only_tablo_writes() {
        for (stem, tail) in [("table-", "fixed"), ("tabular-", "nums")] {
            let rule = format!(".{stem}{tail}{{");
            assert!(STYLESHEET.contains(&rule), "{stem}{tail} is missing");
        }
    }

    #[test]
    fn the_stylesheet_carries_tablos_default_theme() {
        for token in ["--primary:", "--sidebar-accent:"] {
            assert!(STYLESHEET.contains(token), "{token} is missing");
        }
        assert!(STYLESHEET.contains(".dark{"), "the dark tokens are missing");
    }

    #[tokio::test]
    async fn the_panel_serves_its_login_page_and_gates_the_list() {
        let router = router(connect().await.expect("connect"))
            .panel(panel())
            .expect("the panel mounts")
            .build();
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
