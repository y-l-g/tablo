//! The locale a request reads the panel in, from the signed-in user, else the browser's
//! `Accept-Language`, else the panel's default, and the app's catalog over Tablo's in it.

use tablo::{
    Allow, Auth, Authenticator, DeclarationErrorKind, LocaleFault, Panel, PanelUser, Resource,
    ResourceDef, Table, TextColumn, auth::AuthSession, lens, testing::page_language,
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{TestClient, body_string, field_error_message, mount, refusal, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Book {
    #[key]
    #[auto]
    id: Uuid,
    page_count: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Book)]
struct BookForm {
    page_count: String,
}

struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = BookForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("books")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Book.page_count))))
    }
}

/// An account reading the panel in its own locale, or in none.
#[derive(Debug, Clone, toasty::Model)]
struct Reader {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
    locale: Option<String>,
}

impl PanelUser for Reader {
    fn user_id(&self) -> String {
        self.id.to_string()
    }

    fn display_name(&self) -> &str {
        &self.name
    }

    fn locale(&self) -> Option<&str> {
        self.locale.as_deref()
    }
}

struct ReaderAuth;

impl Authenticator for ReaderAuth {
    type User = Reader;

    async fn verify(
        &self,
        _cx: &Cx,
        _login: &str,
        _password: &str,
    ) -> topcoat::Result<Option<Reader>> {
        Ok(None)
    }

    async fn find_by_id(&self, cx: &Cx, id: &str) -> topcoat::Result<Option<Reader>> {
        let Ok(id) = Uuid::parse_str(id) else {
            return Ok(None);
        };
        Ok(Reader::filter(Reader::fields().id().eq(id))
            .first()
            .exec(&mut tablo::db::db(cx))
            .await?)
    }
}

async fn db() -> Db {
    test_db(toasty::models!(Book, Reader, AuthSession)).await
}

/// A panel offering French, then English, with the app's own French for the derived label of
/// `page_count` and for one of Tablo's messages.
fn panel(auth: Auth) -> Panel {
    Panel::new("admin")
        .auth(auth)
        .locales(["fr", "en"])
        .translations(
            "fr",
            "field-page_count = Nombre de pages\ntablo-field-required = Il faut { $label }",
        )
        .resource::<BookResource>()
}

fn router(db: &Db, auth: Auth) -> Router {
    mount(db.clone(), panel(auth)).expect("panel builds")
}

/// The `<html lang>` of the create page and the error the empty form renders, as `client` reads
/// them.
async fn read(client: &TestClient<'_>) -> (Option<String>, Option<String>) {
    let lang = page_language(&body_string(client.get("/admin/books/-/create").await).await);
    let response = client.submit("/admin/books/-/create", "page_count=").await;
    let error = field_error_message(&body_string(response).await, "page_count");
    (lang, error)
}

/// The browser's best match among the offered locales wins, a region reading its language; a
/// language the panel does not offer reads the default.
#[tokio::test]
async fn accept_language_picks_the_offered_locale() {
    let db = db().await;
    let router = router(&db, Auth::disabled());
    let french = (
        Some("fr".to_string()),
        Some("Il faut Nombre de pages".to_string()),
    );
    let english = (
        Some("en".to_string()),
        Some("Page count is required".to_string()),
    );
    for (languages, expected) in [
        (None, &french),
        (Some("en-GB, fr;q=0.5"), &english),
        (Some("fr-CA, en;q=0.5"), &french),
        (Some("de"), &french),
    ] {
        let client = TestClient::new(&router);
        let client = match languages {
            Some(languages) => client.language(languages),
            None => client,
        };
        assert_eq!(&read(&client).await, expected, "{languages:?}");
    }
}

/// A signed-in user's own locale wins over the browser's; one the panel does not offer counts as
/// none.
#[tokio::test]
async fn a_users_locale_wins_over_the_browsers() {
    let db = db().await;
    let router = router(&db, Auth::custom(ReaderAuth));
    for (name, locale, expected) in [
        ("Ada", Some("en"), "en"),
        ("Grace", None, "fr"),
        ("Hedy", Some("de"), "fr"),
    ] {
        let reader = toasty::create!(Reader {
            name: name.to_string(),
            locale: locale.map(str::to_string),
        })
        .exec(&mut db.clone())
        .await
        .unwrap();
        let client = TestClient::new(&router)
            .language("fr")
            .sign_in(&db, "admin", &reader)
            .await;
        let (lang, _) = read(&client).await;
        assert_eq!(lang.as_deref(), Some(expected), "{name}");
    }
}

/// A panel refuses to mount a locale no catalog translates and a catalog for a locale it does
/// not offer.
#[tokio::test]
async fn mount_refuses_a_locale_no_request_could_read() {
    let db = db().await;
    let kinds: Vec<_> = refusal(mount(
        db,
        Panel::new("admin")
            .auth(Auth::disabled())
            .locales(["en", "tlh"])
            .translations("de", "hello = Hallo")
            .resource::<BookResource>(),
    ))
    .into_iter()
    .map(|error| error.kind)
    .collect();
    assert_eq!(
        kinds,
        [
            DeclarationErrorKind::Locale(LocaleFault::Unoffered {
                locale: "de".to_string()
            }),
            DeclarationErrorKind::Locale(LocaleFault::Untranslated {
                locale: "tlh".to_string()
            }),
        ]
    );
}
