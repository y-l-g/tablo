//! A multiple choice over a list column: the options a create and an edit store, the list an
//! edit keeps when it posts nothing, and what the parse and the mount refuse.

use tablo::{
    Allow, DeclarationErrorKind, ManyToManyFault, Resource, ResourceDef, Schema, Table, TextColumn,
    lens,
    testing::{ErrorKind, mount_with_handle},
};
use toasty::Db;
use topcoat::router::Router;
use uuid::Uuid;

use crate::common::{TestClient, input_values, mount, panel, refusal, test_db};

#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo::Options)]
enum Channel {
    Email,
    #[option(value = "text", label = "Text message")]
    Sms,
    Push,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Contact {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
    channels: Vec<String>,
    muted: Vec<Channel>,
}

#[derive(tablo::RecordForm)]
#[form(model = Contact)]
struct ContactForm {
    name: String,
    #[form(options = Channel)]
    channels: Vec<String>,
    #[form(options)]
    muted: Vec<Channel>,
}

struct ContactResource;

impl Resource for ContactResource {
    type Model = Contact;
    type Form = ContactForm;

    fn declare() -> ResourceDef<Self> {
        let c = ContactForm::controls();
        ResourceDef::new()
            .slug("contacts")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Contact.name))))
            .form(Schema::new((c.name, c.channels.max_items(2), c.muted)))
    }
}

async fn setup() -> (Router, tablo::PanelHandle, Db) {
    let db = test_db(toasty::models!(Contact)).await;
    let (router, handle) =
        mount_with_handle(db.clone(), panel().resource::<ContactResource>()).expect("panel builds");
    (router, handle, db)
}

async fn contacts(db: &Db) -> Vec<Contact> {
    Contact::all()
        .exec(&mut db.clone())
        .await
        .expect("query contacts")
}

async fn stored(db: &Db, contact: &Contact) -> Vec<String> {
    Contact::get_by_id(&mut db.clone(), contact.id)
        .await
        .expect("the contact")
        .channels
}

#[tokio::test]
async fn a_create_stores_the_checked_options() {
    let (router, handle, db) = setup().await;
    let client = TestClient::new(&router).panel(&handle);
    let reply = client
        .resource::<ContactResource>()
        .create()
        .set(ContactFormField::Name, "Ada")
        .add(ContactFormField::Channels, "")
        .add(ContactFormField::Channels, "text")
        .add(ContactFormField::Channels, "push")
        .add(ContactFormField::Muted, "text")
        .submit()
        .await;
    assert_eq!(reply.status(), 303, "{}", reply.html());
    let contact = contacts(&db).await.remove(0);
    assert_eq!(contact.channels, ["text", "push"]);
    assert_eq!(
        contact.muted,
        [Channel::Sms],
        "an enum list stores the variants"
    );

    let edit = client
        .resource::<ContactResource>()
        .edit_page(&contact)
        .await;
    assert_eq!(
        input_values(edit.html(), "channels"),
        ["", "text", "push"],
        "the edit form checks the stored options, after the hidden blank"
    );
    assert_eq!(input_values(edit.html(), "muted"), ["", "text"]);
}

#[tokio::test]
async fn an_edit_posting_the_blank_alone_stores_the_empty_list_and_one_posting_nothing_keeps_it() {
    let (router, handle, db) = setup().await;
    let client = TestClient::new(&router).panel(&handle);
    let contacts = client.resource::<ContactResource>();
    let reply = contacts
        .create()
        .set(ContactFormField::Name, "Ada")
        .add(ContactFormField::Channels, "email")
        .submit()
        .await;
    assert_eq!(reply.status(), 303);
    let contact = self::contacts(&db).await.remove(0);

    let reply = contacts
        .edit(&contact)
        .set(ContactFormField::Name, "Ada L.")
        .submit()
        .await;
    assert_eq!(reply.status(), 303);
    assert_eq!(
        stored(&db, &contact).await,
        ["email"],
        "an unposted list stays"
    );

    let reply = contacts
        .edit(&contact)
        .set(ContactFormField::Channels, "")
        .submit()
        .await;
    assert_eq!(reply.status(), 303);
    assert!(stored(&db, &contact).await.is_empty(), "no box checked");
}

#[tokio::test]
async fn a_value_the_options_do_not_offer_or_one_too_many_writes_nothing() {
    let (router, handle, db) = setup().await;
    let client = TestClient::new(&router).panel(&handle);
    let contacts = client.resource::<ContactResource>();
    for posted in [&["email", "sms"][..], &["email", "text", "push"]] {
        let mut submission = contacts.create().set(ContactFormField::Name, "Ada");
        for value in posted {
            submission = submission.add(ContactFormField::Channels, value);
        }
        let reply = submission.submit().await;
        assert_eq!(reply.status(), 200, "{posted:?} is refused");
        assert_eq!(
            reply.field_error(ContactFormField::Channels),
            Some(ErrorKind::Invalid),
            "{posted:?}"
        );
    }
    assert!(self::contacts(&db).await.is_empty());
}

/// A choice over a list column's options, required through `min_items`.
struct RequiredContactResource;

impl Resource for RequiredContactResource {
    type Model = Contact;
    type Form = ContactForm;

    fn declare() -> ResourceDef<Self> {
        let c = ContactForm::controls();
        ResourceDef::new()
            .slug("required-contacts")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Contact.name))))
            .form(Schema::new((c.name, c.channels.min_items(1), c.muted)))
    }
}

#[tokio::test]
async fn min_items_requires_a_choice() {
    let db = test_db(toasty::models!(Contact)).await;
    let (router, handle) =
        mount_with_handle(db.clone(), panel().resource::<RequiredContactResource>())
            .expect("panel builds");
    let client = TestClient::new(&router).panel(&handle);
    let reply = client
        .resource::<RequiredContactResource>()
        .create()
        .set(ContactFormField::Name, "Ada")
        .set(ContactFormField::Channels, "")
        .submit()
        .await;
    assert_eq!(reply.status(), 200);
    assert!(reply.field_error(ContactFormField::Channels).is_some());
    assert!(contacts(&db).await.is_empty());
}

/// A multiple choice over records, bound to a list column rather than a many-to-many field.
struct KeyedContactResource;

impl Resource for KeyedContactResource {
    type Model = Contact;
    type Form = ContactForm;

    fn declare() -> ResourceDef<Self> {
        let c = ContactForm::controls();
        ResourceDef::new()
            .slug("keyed-contacts")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Contact.name))))
            .form(Schema::new((
                c.name,
                c.channels.relationship::<ContactResource>(),
                c.muted,
            )))
    }
}

#[tokio::test]
async fn mounting_refuses_a_relationships_multiple_choice_over_a_list_column() {
    let db = test_db(toasty::models!(Contact)).await;
    let kinds: Vec<DeclarationErrorKind> = refusal(mount(
        db,
        panel()
            .resource::<ContactResource>()
            .resource::<KeyedContactResource>(),
    ))
    .into_iter()
    .map(|error| error.kind)
    .collect();
    assert_eq!(
        kinds,
        [DeclarationErrorKind::ManyToMany {
            field: "channels".to_string(),
            fault: ManyToManyFault::NoJoin,
        }]
    );
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Note {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    labels: Vec<String>,
}

#[derive(tablo::RecordForm)]
#[form(model = Note)]
struct NoteForm {
    title: String,
    #[form(tags, options = Channel)]
    labels: Vec<String>,
}

struct NoteResource;

impl Resource for NoteResource {
    type Model = Note;
    type Form = NoteForm;

    fn declare() -> ResourceDef<Self> {
        let c = NoteForm::controls();
        ResourceDef::new()
            .slug("notes")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Note.title))))
            .form(Schema::new((c.title, c.labels.max_items(3))))
    }
}

async fn notes(db: &Db) -> Vec<Note> {
    Note::all()
        .exec(&mut db.clone())
        .await
        .expect("query notes")
}

/// Tags take typed values beside their suggestions: comma-separated, trimmed, each once.
#[tokio::test]
async fn tags_store_typed_values_and_suggestions() {
    let db = test_db(toasty::models!(Note)).await;
    let (router, handle) =
        mount_with_handle(db.clone(), panel().resource::<NoteResource>()).expect("panel builds");
    let client = TestClient::new(&router).panel(&handle);
    let notes_client = client.resource::<NoteResource>();
    let reply = notes_client
        .create()
        .set(NoteFormField::Title, "Plans")
        .add(NoteFormField::Labels, "")
        .add(NoteFormField::Labels, "email")
        .add(NoteFormField::Labels, " urgent , ,email, q3 ")
        .submit()
        .await;
    assert_eq!(reply.status(), 303, "{}", reply.html());
    let note = notes(&db).await.remove(0);
    assert_eq!(note.labels, ["email", "urgent", "q3"]);

    let edit = notes_client.edit_page(&note).await;
    assert_eq!(
        input_values(edit.html(), "labels"),
        ["", "email", "urgent", "q3"],
        "a checked box per held value, a typed one or a suggestion"
    );

    let reply = notes_client
        .edit(&note)
        .set(NoteFormField::Labels, "")
        .add(NoteFormField::Labels, "q3")
        .add(NoteFormField::Labels, "launch")
        .submit()
        .await;
    assert_eq!(reply.status(), 303);
    let stored = Note::get_by_id(&mut db.clone(), note.id)
        .await
        .expect("the note")
        .labels;
    assert_eq!(stored, ["q3", "launch"], "unchecked tags drop");

    let reply = notes_client
        .edit(&note)
        .set(NoteFormField::Labels, "a, b, c, d")
        .submit()
        .await;
    assert_eq!(reply.status(), 200, "max_items caps typed tags");
    assert_eq!(
        reply.field_error(NoteFormField::Labels),
        Some(ErrorKind::Invalid)
    );
}

/// Tags over a relationship's records, whose values no one types.
struct RelatedTagsResource;

impl Resource for RelatedTagsResource {
    type Model = Note;
    type Form = NoteForm;

    fn declare() -> ResourceDef<Self> {
        let c = NoteForm::controls();
        ResourceDef::new()
            .slug("related-notes")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Note.title))))
            .form(Schema::new((
                c.title,
                c.labels.relationship::<NoteResource>(),
            )))
    }
}

#[tokio::test]
async fn mounting_refuses_tags_over_records() {
    let db = test_db(toasty::models!(Note)).await;
    let kinds: Vec<DeclarationErrorKind> = refusal(mount(
        db,
        panel()
            .resource::<NoteResource>()
            .resource::<RelatedTagsResource>(),
    ))
    .into_iter()
    .map(|error| error.kind)
    .collect();
    assert!(
        kinds.contains(&DeclarationErrorKind::FieldRule {
            field: "labels".to_string(),
            fault: tablo::FieldRuleFault::TagsOverRecords,
        }),
        "{kinds:?}"
    );
}
