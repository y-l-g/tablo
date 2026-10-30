//! A `has_one` relation's related form, end to end: the related resource's form inside the
//! owner's create and edit forms, written through that resource in the owner's transaction.

use http::StatusCode;
use tablo::{
    Ability, Change, DeclarationErrorKind, FieldErrors, NoForm, RelatedFormFault, Relation,
    Resource, ResourceDef, ResourcePage, Site, TextColumn, lens,
    testing::{TestClient, field_error, form_body, input_value, stale_form},
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{body_string, get, mount, panel, post_fields, refusal, test_db};

#[derive(Debug, toasty::Model, tablo::ModelColumns, Clone)]
struct Member {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
}

#[derive(Debug, toasty::Model, tablo::ModelColumns, Clone)]
struct Profile {
    #[key]
    #[auto]
    id: Uuid,
    #[unique]
    member_id: Uuid,
    bio: String,
    #[version]
    version: u64,
}

struct MemberResource;

impl Resource for MemberResource {
    type Model = Member;
    type Form = MemberForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy_fn(|_cx, _ability| true)
            .table(tablo::Table::new(TextColumn::new(lens!(Member.name))))
            .relation(Relation::has_one::<ProfileResource>(
                Profile::fields().member_id(),
            ))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Member)]
struct MemberForm {
    name: String,
}

struct ProfileResource;

impl Resource for ProfileResource {
    type Model = Profile;
    type Form = ProfileForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().policy_fn(|cx, ability| match ability {
            Ability::Create => !has_cookie(cx, "no-create"),
            Ability::Update(_) => !has_cookie(cx, "no-update"),
            _ => true,
        })
    }

    fn validate_record(
        _cx: &Cx,
        form: &ProfileForm,
    ) -> FieldErrors<<ProfileForm as tablo::RecordForm>::Field> {
        let mut errors = FieldErrors::new();
        if form.bio == "refused" {
            errors.add(ProfileFormField::Bio, "Refused");
        }
        errors
    }

    async fn before_commit(
        _cx: &Cx,
        change: &Change<Profile>,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        match change {
            Change::Create(profile) | Change::Update { after: profile, .. }
                if profile.bio == "failed" =>
            {
                Err(topcoat::Error::msg("the profile's hook failed"))
            }
            _ => Ok(()),
        }
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Profile)]
struct ProfileForm {
    bio: String,
    #[form(relationship = MemberResource)]
    member_id: Uuid,
}

/// Whether the request carries the cookie `name`: the switch the profile's policy reads.
fn has_cookie(cx: &Cx, name: &str) -> bool {
    topcoat::context::try_context::<http::request::Parts>(cx).is_some_and(|parts| {
        parts
            .headers
            .get_all(http::header::COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .any(|jar| {
                jar.split(';')
                    .any(|pair| pair.trim().starts_with(&format!("{name}=")))
            })
    })
}

async fn fixture() -> (Router, Db) {
    let db = test_db(toasty::models!(Member, Profile)).await;
    let router = mount(
        db.clone(),
        panel()
            .resource::<MemberResource>()
            .resource::<ProfileResource>(),
    )
    .expect("panel builds");
    (router, db)
}

async fn member(db: &mut Db, name: &str) -> Member {
    toasty::create!(Member { name }).exec(db).await.unwrap()
}

async fn profile(db: &mut Db, member: &Member, bio: &str) -> Profile {
    toasty::create!(Profile {
        member_id: member.id,
        bio,
    })
    .exec(db)
    .await
    .unwrap()
}

async fn members(db: &mut Db) -> Vec<Member> {
    Member::all().exec(db).await.unwrap()
}

async fn profiles(db: &mut Db) -> Vec<Profile> {
    Profile::all().exec(db).await.unwrap()
}

#[tokio::test]
async fn a_create_writes_the_record_and_its_child() {
    let (router, mut db) = fixture().await;
    let response = post_fields(
        &router,
        "/admin/members/-/create",
        &[("name", "Ada"), ("profiles.bio", "Mathematician")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let [ada] = members(&mut db).await.try_into().unwrap();
    let [written] = profiles(&mut db).await.try_into().unwrap();
    assert_eq!(written.member_id, ada.id);
    assert_eq!(written.bio, "Mathematician");
}

/// The child's parse and `validate_record` run after the owner's write, in its transaction: a
/// refusal rolls the owner back too, and both forms render again.
#[tokio::test]
async fn a_refused_child_writes_neither_record() {
    let (router, mut db) = fixture().await;
    for (bio, refused) in [("", "required"), ("refused", "invalid")] {
        let response = post_fields(
            &router,
            "/admin/members/-/create",
            &[("name", "Ada"), ("profiles.bio", bio)],
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK, "{refused}");
        let html = body_string(response).await;
        assert!(
            field_error(&html, "profiles-bio").is_some(),
            "{refused}: {html}"
        );
        assert_eq!(input_value(&html, "name").as_deref(), Some("Ada"), "{html}");
        assert!(members(&mut db).await.is_empty(), "{refused}");
        assert!(profiles(&mut db).await.is_empty(), "{refused}");
    }
}

/// The child's `before_commit` runs inside the owner's transaction, so its failure keeps both.
#[tokio::test]
async fn a_failing_child_hook_writes_neither_record() {
    let (router, mut db) = fixture().await;
    let response = post_fields(
        &router,
        "/admin/members/-/create",
        &[("name", "Ada"), ("profiles.bio", "failed")],
    )
    .await;
    assert!(response.status().is_server_error(), "{}", response.status());
    assert!(members(&mut db).await.is_empty());
    assert!(profiles(&mut db).await.is_empty());
}

#[tokio::test]
async fn an_edit_updates_the_records_child() {
    let (router, mut db) = fixture().await;
    let ada = member(&mut db, "Ada").await;
    let held = profile(&mut db, &ada, "Mathematician").await;
    let edit = format!("/admin/members/{}/edit", ada.id);
    let html = body_string(get(&router, &edit).await).await;
    assert_eq!(
        input_value(&html, "profiles.bio").as_deref(),
        Some("Mathematician")
    );
    let response = post_fields(&router, &edit, &[("name", "Ada"), ("profiles.bio", "Poet")]).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let [written] = profiles(&mut db).await.try_into().unwrap();
    assert_eq!((written.id, written.bio.as_str()), (held.id, "Poet"));
}

#[tokio::test]
async fn an_edit_creates_a_child_the_record_lacks() {
    let (router, mut db) = fixture().await;
    let ada = member(&mut db, "Ada").await;
    let response = post_fields(
        &router,
        &format!("/admin/members/{}/edit", ada.id),
        &[("name", "Ada"), ("profiles.bio", "Poet")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let [written] = profiles(&mut db).await.try_into().unwrap();
    assert_eq!((written.member_id, written.bio.as_str()), (ada.id, "Poet"));
}

/// A save posting nothing under the prefix, such as an editable cell's, leaves the child alone.
#[tokio::test]
async fn a_save_posting_no_child_key_leaves_the_child() {
    let (router, mut db) = fixture().await;
    let ada = member(&mut db, "Ada").await;
    let response = post_fields(
        &router,
        &format!("/admin/members/{}/edit", ada.id),
        &[("name", "Ada Lovelace")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(members(&mut db).await[0].name, "Ada Lovelace");
    assert!(profiles(&mut db).await.is_empty(), "no child was created");
}

/// The owner's key fills the foreign key: a posted one would move the child to another record.
#[tokio::test]
async fn a_posted_foreign_key_is_refused() {
    let (router, mut db) = fixture().await;
    let ada = member(&mut db, "Ada").await;
    let bob = member(&mut db, "Bob").await;
    let response = post_fields(
        &router,
        &format!("/admin/members/{}/edit", ada.id),
        &[
            ("name", "Ada"),
            ("profiles.bio", "Poet"),
            ("profiles.member_id", &bob.id.to_string()),
        ],
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(profiles(&mut db).await.is_empty());
}

/// The related resource's policy decides whether the section renders; with none, a submission
/// posting under its prefix is refused rather than written.
#[tokio::test]
async fn the_child_policy_gates_the_section() {
    let (router, mut db) = fixture().await;
    let ada = member(&mut db, "Ada").await;
    profile(&mut db, &ada, "Mathematician").await;
    let bob = member(&mut db, "Bob").await;
    let cases = [
        ("/admin/members/-/create".to_string(), "no-create"),
        (format!("/admin/members/{}/edit", ada.id), "no-update"),
        (format!("/admin/members/{}/edit", bob.id), "no-create"),
    ];
    for (uri, switch) in cases {
        let client = TestClient::new(&router).cookie(switch, "1");
        let html = body_string(client.get(&uri).await).await;
        assert_eq!(input_value(&html, "profiles.bio"), None, "{uri}: {html}");
        let response = client
            .submit(
                &uri,
                &form_body(&[("name", "Carol"), ("profiles.bio", "Poet")]),
            )
            .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
    }
    let [kept] = profiles(&mut db).await.try_into().unwrap();
    assert_eq!(kept.bio, "Mathematician");
}

/// A record holding more than one child renders no section, since a save could not tell which
/// to edit.
#[tokio::test]
async fn a_record_with_two_children_renders_no_section() {
    #[derive(Debug, toasty::Model, tablo::ModelColumns, Clone)]
    struct Note {
        #[key]
        #[auto]
        id: Uuid,
        member_id: Uuid,
        body: String,
    }

    #[derive(tablo::RecordForm)]
    #[form(model = Note)]
    struct NoteForm {
        body: String,
        #[form(relationship = NotedResource)]
        member_id: Uuid,
    }

    struct NoteResource;

    impl Resource for NoteResource {
        type Model = Note;
        type Form = NoteForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().policy_fn(|_cx, _ability| true)
        }
    }

    struct NotedResource;

    impl Resource for NotedResource {
        type Model = Member;
        type Form = MemberForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .policy_fn(|_cx, _ability| true)
                .relation(Relation::has_one::<NoteResource>(
                    Note::fields().member_id(),
                ))
        }
    }

    let mut db = test_db(toasty::models!(Member, Note)).await;
    let ada = member(&mut db, "Ada").await;
    for body in ["first", "second"] {
        toasty::create!(Note {
            member_id: ada.id,
            body,
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let router = mount(
        db.clone(),
        panel()
            .resource::<NotedResource>()
            .resource::<NoteResource>(),
    )
    .expect("panel builds");
    let edit = format!("/admin/noteds/{}/edit", ada.id);
    let html = body_string(get(&router, &edit).await).await;
    assert_eq!(input_value(&html, "name").as_deref(), Some("Ada"), "{html}");
    assert_eq!(input_value(&html, "notes.body"), None, "{html}");
    let response = post_fields(&router, &edit, &[("name", "Ada"), ("notes.body", "third")]).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Mounting refuses a related form the owner's form cannot carry.
#[tokio::test]
async fn a_related_form_must_fit_inside_the_owners() {
    struct Listed;

    impl Resource for Listed {
        type Model = Profile;
        type Form = NoForm<Profile>;
    }

    #[derive(tablo::RecordForm)]
    #[form(model = Profile)]
    struct BioForm {
        bio: String,
    }

    struct Unkeyed;

    impl Resource for Unkeyed {
        type Model = Profile;
        type Form = BioForm;

        // A form without the foreign key cannot create a profile, so it only edits one.
        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().without_page(ResourcePage::Create)
        }
    }

    struct Named;

    impl Resource for Named {
        type Model = Profile;
        type Form = ProfileForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().slug("name")
        }
    }

    struct Owner<C>(std::marker::PhantomData<C>);

    impl<C: Resource<Model = Profile>> Resource for Owner<C> {
        type Model = Member;
        type Form = MemberForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().relation(Relation::has_one::<C>(Profile::fields().member_id()))
        }
    }

    let db = test_db(toasty::models!(Member, Profile)).await;
    let errors = refusal(mount(
        db.clone(),
        panel().resource::<Owner<Listed>>().resource::<Listed>(),
    ));
    assert_eq!(
        errors.iter().map(|error| &error.kind).collect::<Vec<_>>(),
        [&DeclarationErrorKind::RelatedForm(RelatedFormFault::NoForm)],
        "{errors:?}"
    );
    assert_eq!(errors[0].site, Site::Relation("listeds".to_string()));
    let errors = refusal(mount(
        db.clone(),
        panel().resource::<Owner<Unkeyed>>().resource::<Unkeyed>(),
    ));
    assert_eq!(
        errors.iter().map(|error| &error.kind).collect::<Vec<_>>(),
        [&DeclarationErrorKind::RelatedForm(
            RelatedFormFault::UnboundForeignKey {
                column: "member_id".to_string()
            }
        )],
        "{errors:?}"
    );
    // The owner's `name` is the child's slug: a repeater so named would post its rows under the
    // related form's prefix.
    let errors = refusal(mount(
        db,
        panel()
            .resource::<Owner<Named>>()
            .resource::<Named>()
            .resource::<MemberResource>()
            .resource::<ProfileResource>(),
    ));
    assert_eq!(
        errors.iter().map(|error| &error.kind).collect::<Vec<_>>(),
        [&DeclarationErrorKind::RelatedForm(
            RelatedFormFault::ShadowedKey {
                field: "name".to_string()
            }
        )],
        "{errors:?}"
    );
}

/// The section posts the child's version: a save after another one changed the child refuses
/// as stale, keeps the other save's write, and carries the version that now stands.
#[tokio::test]
async fn a_child_another_save_changed_refuses_as_stale() {
    let (router, mut db) = fixture().await;
    let ada = member(&mut db, "Ada").await;
    let held = profile(&mut db, &ada, "Mathematician").await;
    let edit = format!("/admin/members/{}/edit", ada.id);
    let html = body_string(get(&router, &edit).await).await;
    let opened = input_value(&html, "profiles.record_version").expect("the section's version");
    held.clone()
        .update()
        .bio("theirs")
        .exec(&mut db)
        .await
        .unwrap();
    let mine = [
        ("name", "Ada"),
        ("profiles.bio", "mine"),
        ("profiles.record_version", opened.as_str()),
    ];
    let response = post_fields(&router, &edit, &mine).await;
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_string(response).await;
    assert!(stale_form(&html), "{html}");
    assert_eq!(profiles(&mut db).await[0].bio, "theirs");
    let now = input_value(&html, "profiles.record_version").expect("the section's version");
    assert_ne!(now, opened);
    let again = [
        ("name", "Ada"),
        ("profiles.bio", "mine"),
        ("profiles.record_version", now.as_str()),
    ];
    let response = post_fields(&router, &edit, &again).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(profiles(&mut db).await[0].bio, "mine");
}

/// A child another save created after the submission was read is not the one it was read
/// against: writing it would apply a create's defaults over the other save's child.
#[tokio::test]
async fn a_child_created_since_the_read_refuses_as_stale() {
    struct RacedOwner;

    impl Resource for RacedOwner {
        type Model = Member;
        type Form = MemberForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("raced")
                .policy_fn(|_cx, _ability| true)
                .relation(Relation::has_one::<ProfileResource>(
                    Profile::fields().member_id(),
                ))
        }

        async fn update_record(
            cx: &Cx,
            record: Member,
            posted: tablo::Posted<MemberForm>,
            ex: &mut dyn toasty::Executor,
        ) -> Result<Member, tablo::WriteError<MemberFormField>> {
            // Another save, after the submission was read: the record now holds a child.
            toasty::create!(Profile {
                member_id: record.id,
                bio: "theirs",
            })
            .exec(&mut *ex)
            .await?;
            Ok(tablo::write_update::<Self>(cx, record, posted, ex).await?)
        }
    }

    let mut db = test_db(toasty::models!(Member, Profile)).await;
    let router = mount(
        db.clone(),
        panel()
            .resource::<RacedOwner>()
            .resource::<MemberResource>()
            .resource::<ProfileResource>(),
    )
    .expect("panel builds");
    let ada = member(&mut db, "Ada").await;
    let response = post_fields(
        &router,
        &format!("/admin/raced/{}/edit", ada.id),
        &[("name", "Ada"), ("profiles.bio", "mine")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(stale_form(&body_string(response).await));
    assert!(
        profiles(&mut db).await.is_empty(),
        "the rollback keeps neither"
    );
}

/// The handler a condition watching a related form's field reads is that control's posted
/// name, so the browser updates the condition as the user types.
#[tokio::test]
async fn a_condition_inside_a_related_form_watches_the_posted_name() {
    #[derive(Debug, toasty::Model, tablo::ModelColumns, Clone)]
    struct Card {
        #[key]
        #[auto]
        id: Uuid,
        member_id: Uuid,
        kind: String,
        vat: Option<String>,
    }

    #[derive(tablo::RecordForm)]
    #[form(model = Card)]
    struct CardForm {
        kind: String,
        vat: Option<String>,
        #[form(relationship = CardedResource)]
        member_id: Uuid,
    }

    struct CardResource;

    impl Resource for CardResource {
        type Model = Card;
        type Form = CardForm;

        fn declare() -> ResourceDef<Self> {
            let c = CardForm::controls();
            let vat = c.vat.visible_when(&c.kind, ["company"]);
            ResourceDef::new()
                .policy_fn(|_cx, _ability| true)
                .form(tablo::Schema::new((c.kind, vat, c.member_id)))
        }
    }

    struct CardedResource;

    impl Resource for CardedResource {
        type Model = Member;
        type Form = MemberForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .policy_fn(|_cx, _ability| true)
                .relation(Relation::has_one::<CardResource>(
                    Card::fields().member_id(),
                ))
        }
    }

    let db = test_db(toasty::models!(Member, Card)).await;
    let router = mount(
        db,
        panel()
            .resource::<CardedResource>()
            .resource::<CardResource>(),
    )
    .expect("panel builds");
    let html = body_string(get(&router, "/admin/cardeds/-/create").await).await;
    // The handler's compiled comparison carries the name it reads.
    let reads = |name: &str| html.contains(&format!("hydrate(&quot;{name}&quot;)"));
    assert!(reads("cards.kind") && !reads("kind"), "{html}");
}

#[tokio::test]
async fn a_has_one_relation_is_refused_as_a_tab() {
    struct TabbedMembers;

    impl Resource for TabbedMembers {
        type Model = Member;
        type Form = MemberForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("tabbed")
                .relation(Relation::has_one::<ProfileResource>(Profile::fields().member_id()).tab())
        }
    }

    let db = test_db(toasty::models!(Member, Profile)).await;
    let errors = refusal(mount(
        db,
        panel()
            .resource::<TabbedMembers>()
            .resource::<MemberResource>()
            .resource::<ProfileResource>(),
    ));
    assert_eq!(
        errors
            .iter()
            .map(|error| (&error.site, &error.kind))
            .collect::<Vec<_>>(),
        [(
            &Site::Relation("profiles".to_string()),
            &DeclarationErrorKind::HasOneTab
        )],
        "{errors:?}"
    );
}
