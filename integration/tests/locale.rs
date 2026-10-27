//! The locale a request reads the panel in, from the signed-in user, else the browser's
//! `Accept-Language`, else the panel's default, and the app's catalog over Tablo's in it; and the
//! locale a mail reads, its recipient's; and an option's derived label, translated wherever it
//! renders.

use http::header::VARY;
use tablo::{
    Allow, Auth, AuthMail, Authenticator, DeclarationErrorKind, LocaleFault, Panel, PanelUser,
    PasswordReset, Resource, ResourceDef, RouterBuilderPanelExt, SelectFilter, Table, TextColumn,
    auth::{AuthSession, AuthToken, PasswordResetter},
    lens,
    testing::{filter_options, form_body, page_direction, page_language, rows},
};
use toasty::Db;
use topcoat::{
    context::Cx,
    l10n::{Locale, locale},
    mail::{MailConfig, Mailbox, MemoryTransport, RouterBuilderMailExt},
    router::{Router, RouterBuilderDiscoverExt},
};
use uuid::Uuid;

use crate::common::{
    TestClient, body_string, field_error_message, mail::mails, mount, refusal, test_db,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo::Options)]
enum Genre {
    Novel,
    Essay,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Book {
    #[key]
    #[auto]
    id: Uuid,
    page_count: String,
    genre: Genre,
}

#[derive(tablo::RecordForm)]
#[form(model = Book)]
struct BookForm {
    page_count: String,
    #[form(options)]
    genre: Genre,
}

struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = BookForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().slug("books").policy(Allow).table(
            Table::new((
                TextColumn::new(lens!(Book.page_count)),
                TextColumn::new(lens!(Book.genre)),
            ))
            .filters(SelectFilter::of(lens!(Book.genre))),
        )
    }
}

/// An account reading the panel in its own locale, or in none.
#[derive(Debug, Clone, toasty::Model)]
struct Reader {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
    email: String,
    locale: Option<String>,
}

impl PanelUser for Reader {
    fn user_id(&self) -> String {
        self.id.to_string()
    }

    fn display_name(&self) -> &str {
        &self.name
    }

    fn email(&self) -> Option<&str> {
        Some(&self.email)
    }

    fn locale(&self) -> Option<Locale> {
        self.locale.as_deref()?.parse().ok()
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
            .exec(&mut tablo::db::db(cx)?)
            .await?)
    }
}

impl PasswordResetter for ReaderAuth {
    type User = Reader;

    async fn find_by_email(&self, cx: &Cx, email: &str) -> topcoat::Result<Option<Reader>> {
        Ok(Reader::filter(Reader::fields().email().eq(email))
            .first()
            .exec(&mut tablo::db::db(cx)?)
            .await?)
    }

    async fn reset_password(
        &self,
        _cx: &Cx,
        _user_id: &str,
        _password: &str,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        Ok(())
    }
}

async fn db() -> Db {
    test_db(toasty::models!(Book, Reader, AuthSession, AuthToken)).await
}

/// A reader named `name`, reading in `locale`.
async fn reader(db: &Db, name: &str, locale: Option<&str>) -> Reader {
    toasty::create!(Reader {
        name: name.to_string(),
        email: format!("{}@example.com", name.to_lowercase()),
        locale: locale.map(str::to_string),
    })
    .exec(&mut db.clone())
    .await
    .unwrap()
}

/// A panel offering French, then English, with the app's own French for the derived labels of
/// `page_count` and of the genre `Novel`, and for one of Tablo's messages.
fn panel(auth: Auth<impl tablo::PanelUser>) -> Panel {
    Panel::new("admin")
        .auth(auth)
        .locales([locale!("fr"), locale!("en")])
        .translations(
            locale!("fr"),
            "field-page_count = Nombre de pages\n\
             option-genre =\n    .novel = Roman\n\
             tablo-field-required = Il faut { $label }",
        )
        .resource::<BookResource>()
}

fn router(db: &Db, auth: Auth<impl tablo::PanelUser>) -> Router {
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
        // A language the browser refuses reads none, and weight wins over order.
        (Some("fr;q=0, en"), &english),
        (Some("fr;q=0.1, en;q=0.9"), &english),
    ] {
        let client = TestClient::new(&router);
        let client = match languages {
            Some(languages) => client.language(languages),
            None => client,
        };
        assert_eq!(&read(&client).await, expected, "{languages:?}");
    }
}

/// A signed-in user's own locale wins over the browser's, a region reading its language; one the
/// panel does not offer counts as none.
#[tokio::test]
async fn a_users_locale_wins_over_the_browsers() {
    let db = db().await;
    let router = router(&db, Auth::custom(ReaderAuth));
    for (name, locale, expected) in [
        ("Ada", Some("en"), "en"),
        ("Lin", Some("en-GB"), "en"),
        ("Grace", None, "fr"),
        ("Hedy", Some("de"), "fr"),
    ] {
        let reader = reader(&db, name, locale).await;
        let client = TestClient::new(&router)
            .language("fr")
            .sign_in(&db, "admin", &reader)
            .await;
        let (lang, _) = read(&client).await;
        assert_eq!(lang.as_deref(), Some(expected), "{name}");
    }
}

/// A page in a right-to-left locale tells the browser its text runs right to left.
#[tokio::test]
async fn a_right_to_left_locale_reads_right_to_left() {
    let db = db().await;
    let router = mount(
        db,
        Panel::new("admin")
            .auth(Auth::disabled())
            .locales([locale!("en"), locale!("ar")])
            .translations(locale!("ar"), "field-page_count = عدد الصفحات")
            .resource::<BookResource>(),
    )
    .expect("panel builds");
    for (language, expected) in [("ar", "rtl"), ("en", "ltr")] {
        let client = TestClient::new(&router).language(language);
        let html = body_string(client.get("/admin/books").await).await;
        assert_eq!(page_language(&html).as_deref(), Some(language));
        assert_eq!(page_direction(&html).as_deref(), Some(expected));
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
            .locales([locale!("en"), locale!("tlh")])
            .translations(locale!("de"), "hello = Hallo")
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

/// A page of a panel offering several locales tells caches it varies with the browser's
/// languages.
#[tokio::test]
async fn a_page_varies_with_accept_language() {
    let db = db().await;
    let router = router(&db, Auth::disabled());
    let response = TestClient::new(&router).get("/admin/books").await;
    let varies: Vec<_> = response
        .headers()
        .get_all(VARY)
        .iter()
        .map(|value| value.to_str().unwrap().to_ascii_lowercase())
        .collect();
    assert!(
        varies.iter().any(|value| value == "accept-language"),
        "{varies:?}"
    );
}

/// A reset link is mailed in its recipient's locale, whatever browser asked for it.
#[tokio::test]
async fn a_reset_mail_reads_its_recipients_locale() {
    let db = db().await;
    let mail = MemoryTransport::new();
    let auth = Auth::custom(ReaderAuth)
        .mail(AuthMail::new(
            Mailbox::new("no-reply@example.com").unwrap(),
            "https://admin.example.com/",
        ))
        .password_reset(PasswordReset::new(ReaderAuth));
    let router = Router::builder()
        .discover()
        .with(db.clone())
        .mail(MailConfig::builder().transport(mail.clone()).build())
        .panel(panel(auth))
        .expect("panel builds")
        .build();
    reader(&db, "Ada", Some("en")).await;
    reader(&db, "Grace", None).await;
    // One request at a time: two reset mails racing their serializable token writes can abort
    // one of them on PostgreSQL and MySQL.
    for (sent, email) in ["ada@example.com", "grace@example.com"]
        .into_iter()
        .enumerate()
    {
        TestClient::new(&router)
            .language("fr")
            .submit("/admin/forgot-password", &form_body(&[("email", email)]))
            .await;
        mails(&mail, sent + 1).await;
    }
    let sent = mails(&mail, 2).await;
    let subject = |to: &str| {
        sent.iter()
            .find(|mail| {
                mail.to()
                    .iter()
                    .any(|mailbox| mailbox.to_string().contains(to))
            })
            .expect("a mail went to the reader")
            .subject()
            .to_string()
    };
    assert!(
        subject("ada@").starts_with("Reset your"),
        "{}",
        subject("ada@")
    );
    assert!(
        subject("grace@").starts_with("Réinitialisez"),
        "{}",
        subject("grace@")
    );
}

/// A derived option label reads the request's locale in the form's select, the column's cell and
/// the filter's options, and its English fallback where no catalog translates it.
#[tokio::test]
async fn an_options_label_reads_the_requests_locale() {
    let db = db().await;
    toasty::create!(Book {
        page_count: "320".to_string(),
        genre: Genre::Novel,
    })
    .exec(&mut db.clone())
    .await
    .unwrap();
    let router = router(&db, Auth::disabled());
    for (language, novel) in [("fr", "Roman"), ("en", "Novel")] {
        let client = TestClient::new(&router).language(language);
        let list = body_string(client.get("/admin/books").await).await;
        let cells: Vec<String> = rows(&list).into_iter().flat_map(|row| row.cells).collect();
        assert!(
            cells.iter().any(|cell| cell == novel),
            "{language}: {cells:?}"
        );
        let labels: Vec<String> = filter_options(&list, "genre")
            .expect("the genre filter renders")
            .into_iter()
            .map(|option| option.label)
            .collect();
        assert_eq!(labels[1..], [novel, "Essay"], "{language}");
        let form = body_string(client.get("/admin/books/-/create").await).await;
        assert!(
            form.contains(&format!(">{novel}</option>")),
            "{language}: the select reads {novel}: {form}"
        );
    }
}
