//! What a mount refuses, across declarations: each mistake named with the type that declares it,
//! the site it is in and its kind, beside the valid shape of each that mounts.

use tablo::{
    Ability, Auth, DeclarationErrorKind, Detail, NoForm, Page, PageDef, Panel, RecordPage,
    RecordPageDef, Relation, RelationColumn, Resource, ResourceDef, RouterBuilderPanelExt, Schema,
    SegmentFault, SelectFilter, Site, Table, Tenancy, TenancyFault, TenantId, TextColumn, lens,
    relation,
};
use toasty::Db;
use topcoat::{
    Result,
    context::Cx,
    router::{Body, RouteFn, RouteFuture, Router, RouterBuilderDiscoverExt},
    view::{View, view},
};
use uuid::Uuid;

use crate::common::{mount, refusal};

/// One refusal, as the mount names it.
type Refused = (Option<&'static str>, Site, DeclarationErrorKind);

/// The refusals of `mounted`.
fn refused<T>(mounted: Result<T>) -> Vec<Refused> {
    refusal(mounted)
        .into_iter()
        .map(|error| (error.resource, error.site, error.kind))
        .collect()
}

/// A mistake in `T`'s declaration.
fn of<T>(site: Site, kind: DeclarationErrorKind) -> Refused {
    (Some(std::any::type_name::<T>()), site, kind)
}

/// A mistake in the panel's own configuration.
fn panel_error(kind: DeclarationErrorKind) -> Refused {
    (None, Site::Panel, kind)
}

/// `panel` mounted on a router holding no `Db`.
fn mount_without_db(panel: Panel) -> Result<Router> {
    Ok(Router::builder().discover().panel(panel)?.build())
}

/// A database that knows `models` and holds none of their tables: a mount reads only the schema.
async fn schema_only(models: toasty::schema::ModelSet) -> Db {
    Db::builder()
        .models(models)
        .connect("sqlite::memory:")
        .await
        .expect("connect")
}

/// Declares the resource `$name` over `$model` with `$def` as its `declare` body, list-only
/// without a `$form`.
macro_rules! resource {
    ($name:ident, $model:ty => $def:expr) => {
        resource!($name, $model, NoForm<$model> => $def);
    };
    ($name:ident, $model:ty, $form:ty => $def:expr) => {
        struct $name;

        impl Resource for $name {
            type Model = $model;
            type Form = $form;

            fn declare() -> ResourceDef<Self> {
                $def
            }
        }
    };
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Item {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
}

fn items() -> Table<Item> {
    Table::new(TextColumn::new(lens!(Item.name)))
}

/// A slug or prefix that is not one route segment, a slug two resources share, and a panel with
/// no `Db` are refused before any database is read; a panel built without one says so too.
#[test]
fn a_mount_refuses_what_needs_no_database_to_find() {
    resource!(Hostile, Item => ResourceDef::new().slug("a\"b\r\n").table(items()));
    resource!(First, Item => ResourceDef::new().slug("items").table(items()));
    resource!(Second, Item => ResourceDef::new().slug("items").table(items()));
    resource!(BraceOpen, Item => ResourceDef::new().slug("a{b").table(items()));
    resource!(BraceClose, Item => ResourceDef::new().slug("a}b").table(items()));
    resource!(ParenOpen, Item => ResourceDef::new().slug("a(b").table(items()));
    resource!(ParenClose, Item => ResourceDef::new().slug("a)b").table(items()));

    let no_db = || panel_error(DeclarationErrorKind::MissingDb);
    let slug = |segment: &str, fault: char| DeclarationErrorKind::InvalidSegment {
        item: "ResourceDef::slug",
        segment: segment.to_string(),
        fault: SegmentFault::Char(fault),
    };
    let refusals = [
        (
            "a panel with no Db",
            refused(mount_without_db(Panel::new("admin"))),
            vec![no_db()],
        ),
        (
            "a hostile slug",
            refused(mount_without_db(Panel::new("admin").resource::<Hostile>())),
            vec![
                of::<Hostile>(Site::Registration, slug("a\"b\r\n", '"')),
                no_db(),
            ],
        ),
        (
            "two resources on one slug",
            refused(mount_without_db(
                Panel::new("admin").resource::<First>().resource::<Second>(),
            )),
            vec![
                of::<Second>(
                    Site::Registration,
                    DeclarationErrorKind::DuplicateSlug {
                        slug: "items".to_string(),
                    },
                ),
                no_db(),
            ],
        ),
        (
            "a `{` in a slug",
            refused(mount_without_db(
                Panel::new("admin").resource::<BraceOpen>(),
            )),
            vec![
                of::<BraceOpen>(Site::Registration, slug("a{b", '{')),
                no_db(),
            ],
        ),
        (
            "a `}` in a slug",
            refused(mount_without_db(
                Panel::new("admin").resource::<BraceClose>(),
            )),
            vec![
                of::<BraceClose>(Site::Registration, slug("a}b", '}')),
                no_db(),
            ],
        ),
        (
            "a `(` in a slug",
            refused(mount_without_db(
                Panel::new("admin").resource::<ParenOpen>(),
            )),
            vec![
                of::<ParenOpen>(Site::Registration, slug("a(b", '(')),
                no_db(),
            ],
        ),
        (
            "a `)` in a slug",
            refused(mount_without_db(
                Panel::new("admin").resource::<ParenClose>(),
            )),
            vec![
                of::<ParenClose>(Site::Registration, slug("a)b", ')')),
                no_db(),
            ],
        ),
        (
            "a `{` in a panel prefix",
            refused(mount_without_db(Panel::new("adm{in}"))),
            vec![
                panel_error(DeclarationErrorKind::InvalidSegment {
                    item: "panel prefix",
                    segment: "adm{in}".to_string(),
                    fault: SegmentFault::Char('{'),
                }),
                no_db(),
            ],
        ),
    ];
    for (label, refused, expected) in refusals {
        assert_eq!(refused, expected, "{label}");
    }
}

/// `Panel::endpoint` binds a shard or procedure: a route outside Topcoat's runtime path refuses
/// the panel, since the runtime gate never sees it.
#[test]
fn an_endpoint_outside_the_runtime_path_refuses_the_panel() {
    fn report(_cx: &Cx, _body: Body) -> RouteFuture<'_> {
        Box::pin(async { unreachable!("the panel never mounts") })
    }
    let route = RouteFn::new(http::Method::GET, "/reports", report);
    let errors = refused(mount_without_db(Panel::new("admin").endpoint(route)));
    assert!(
        errors.iter().any(|(_, _, kind)| *kind
            == DeclarationErrorKind::EndpointOutsideRuntime {
                path: "/reports".to_string()
            }),
        "{errors:?}"
    );
}

struct Dashboard;

impl Page for Dashboard {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => "home" })
    }
}

struct Reports;

impl Page for Reports {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => "reports" })
    }
}

/// A page on the slug `items`.
struct ItemsPage;

impl Page for ItemsPage {
    fn declare() -> PageDef {
        PageDef::new().slug("items")
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => "items" })
    }
}

/// A page whose slug spans two segments.
struct NestedPage;

impl Page for NestedPage {
    fn declare() -> PageDef {
        PageDef::new().slug("reports/q3")
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => "nested" })
    }
}

/// Declares the record page `$name` of `Paged` at `$slug`.
macro_rules! record_page {
    ($name:ident, $slug:expr) => {
        struct $name;

        impl RecordPage<Paged> for $name {
            fn declare() -> RecordPageDef<Item> {
                RecordPageDef::new().slug($slug)
            }

            async fn render(cx: &Cx, _item: Item) -> Result<impl View> {
                Ok(view! { cx => "page" })
            }
        }
    };
}

record_page!(EditPage, "edit");
record_page!(HistoryPage, "history");
record_page!(OtherHistoryPage, "history");
record_page!(NestedRecordPage, "a/b");
resource!(Paged, Item => ResourceDef::new()
    .slug("paged")
    .table(items())
    .record_page::<EditPage>()
    .record_page::<HistoryPage>()
    .record_page::<OtherHistoryPage>()
    .record_page::<NestedRecordPage>());

/// A record page's slug is one segment after the record's key, apart from the record's own
/// routes and the resource's other record pages.
#[test]
fn a_mount_refuses_a_record_page_it_cannot_route() {
    let page = |slug: &str, kind| of::<Paged>(Site::RecordPage(slug.to_string()), kind);
    assert_eq!(
        refused(mount_without_db(Panel::new("admin").resource::<Paged>())),
        [
            page(
                "edit",
                DeclarationErrorKind::ReservedSlug {
                    slug: "edit".to_string()
                }
            ),
            page(
                "history",
                DeclarationErrorKind::DuplicateSlug {
                    slug: "history".to_string()
                }
            ),
            page(
                "a/b",
                DeclarationErrorKind::InvalidSegment {
                    item: "RecordPageDef::slug",
                    segment: "a/b".to_string(),
                    fault: SegmentFault::Char('/'),
                }
            ),
            panel_error(DeclarationErrorKind::MissingDb),
        ]
    );
}

/// A record page of `Tabbed` at `kids`.
struct KidsPage;

impl RecordPage<Tabbed> for KidsPage {
    fn declare() -> RecordPageDef<Parent> {
        RecordPageDef::new().slug("kids")
    }

    async fn render(cx: &Cx, _parent: Parent) -> Result<impl View> {
        Ok(view! { cx => "kids" })
    }
}

resource!(Kids, Child => ResourceDef::new().slug("kids"));
resource!(EditKids, Child => ResourceDef::new().slug("edit"));
resource!(Tabbed, Parent => ResourceDef::new()
    .slug("tabbed")
    .record_page::<KidsPage>()
    .relation(Relation::has_many::<Kids>(Child::fields().parent_id()).tab())
    .relation(Relation::has_many::<EditKids>(Child::fields().parent_id()).tab()));

/// A relation tab serves after the record's key at its related resource's slug, so it may not
/// take one of the record's own routes or a record page's slug.
#[test]
fn a_mount_refuses_a_relation_tab_it_cannot_route() {
    let tab = |slug: &str, kind| of::<Tabbed>(Site::Relation(slug.to_string()), kind);
    assert_eq!(
        refused(mount_without_db(
            Panel::new("admin")
                .resource::<Tabbed>()
                .resource::<Kids>()
                .resource::<EditKids>()
        )),
        [
            tab(
                "kids",
                DeclarationErrorKind::DuplicateSlug {
                    slug: "kids".to_string()
                }
            ),
            tab(
                "edit",
                DeclarationErrorKind::ReservedSlug {
                    slug: "edit".to_string()
                }
            ),
            panel_error(DeclarationErrorKind::MissingDb),
        ]
    );
}

/// A page on the panel's own sign-in path.
struct LoginPage;

impl Page for LoginPage {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => "login" })
    }
}

/// Pages and resources share one slug namespace, in either order, around the paths the panel
/// routes itself; a slug is one segment; a panel has one home, and a page type mounts once.
#[test]
fn a_mount_refuses_a_page_it_cannot_route() {
    resource!(Items, Item => ResourceDef::new().slug("items").table(items()));
    let held = || DeclarationErrorKind::DuplicateSlug {
        slug: "items".to_string(),
    };
    let twice = || of::<Reports>(Site::Registration, DeclarationErrorKind::DuplicatePage);
    let first = |panel: Panel| refused(mount_without_db(panel)).remove(0);
    let refusals = [
        (
            "a page on a resource's slug",
            first(Panel::new("admin").resource::<Items>().page::<ItemsPage>()),
            of::<ItemsPage>(Site::Registration, held()),
        ),
        (
            "a resource on a page's slug",
            first(Panel::new("admin").page::<ItemsPage>().resource::<Items>()),
            of::<Items>(Site::Registration, held()),
        ),
        (
            "a slug of two segments",
            first(Panel::new("admin").page::<NestedPage>()),
            of::<NestedPage>(
                Site::Registration,
                DeclarationErrorKind::InvalidSegment {
                    item: "PageDef::slug",
                    segment: "reports/q3".to_string(),
                    fault: SegmentFault::Char('/'),
                },
            ),
        ),
        (
            "a second home",
            first(Panel::new("admin").home::<Dashboard>().home::<Reports>()),
            of::<Reports>(Site::Registration, DeclarationErrorKind::SecondHome),
        ),
        (
            "a page registered twice",
            first(
                Panel::new("admin")
                    .page::<Reports>()
                    .page_with::<Reports>(|def| def.slug("figures")),
            ),
            twice(),
        ),
        (
            "a home registered as a page too",
            first(Panel::new("admin").home::<Reports>().page::<Reports>()),
            twice(),
        ),
        (
            "a page on the sign-in path",
            first(Panel::new("admin").page::<LoginPage>()),
            of::<LoginPage>(
                Site::Registration,
                DeclarationErrorKind::ReservedSlug {
                    slug: "login".to_string(),
                },
            ),
        ),
    ];
    for (label, refused, expected) in refusals {
        assert_eq!(refused, expected, "{label}");
    }
}

/// A parent row that carries its tenant, and a child that inherits it.
#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Parent {
    #[key]
    #[auto]
    id: Uuid,
    tenant_id: TenantId,
    name: String,
    #[has_many]
    children: toasty::Deferred<Vec<Child>>,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Child {
    #[key]
    #[auto]
    id: Uuid,
    #[index]
    parent_id: Uuid,
    #[belongs_to(key = parent_id, references = id)]
    parent: toasty::Deferred<Parent>,
    name: String,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Subscriber {
    #[key]
    #[auto]
    id: Uuid,
    nickname: String,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Doc {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
#[key(owner, slot)]
struct Seat {
    owner: String,
    slot: i64,
    label: String,
}

#[derive(Debug, Clone, toasty::Embed)]
struct Place {
    city: String,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Ticket {
    #[key]
    #[auto]
    id: Uuid,
    tenant_id: TenantId,
    title: String,
    place: Place,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
#[unique(tenant_id, email)]
struct Author {
    #[key]
    #[auto]
    id: Uuid,
    tenant_id: TenantId,
    email: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Child)]
struct NameForm {
    name: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Child)]
struct KeyedForm {
    name: String,
    parent_id: Uuid,
}

#[derive(tablo::RecordForm)]
#[form(model = Child)]
struct SeatedForm {
    name: String,
    #[form(relationship = Seats)]
    parent_id: Uuid,
}

#[derive(tablo::RecordForm)]
#[form(model = Subscriber)]
struct NicknameForm {
    nickname: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Author)]
struct AuthorForm {
    email: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Ticket)]
struct TicketForm {
    title: String,
}

fn creates(_cx: &Cx, ability: Ability<'_, impl toasty::schema::Model>) -> bool {
    matches!(ability, Ability::ViewAny | Ability::Create)
}

fn nicknames() -> Table<Subscriber> {
    Table::new(TextColumn::new(lens!(Subscriber.nickname)))
}

resource!(Parents, Parent => ResourceDef::new()
    .tenancy(Tenancy::none())
    .table(Table::new(TextColumn::new(lens!(Parent.name)))));
resource!(FilteredChildren, Child => ResourceDef::new()
.slug("filtered-children")
.tenancy(Tenancy::none())
.table(
    Table::new(TextColumn::new(lens!(Child.name)))
        .filters(SelectFilter::relationship::<Parents>(lens!(Child.parent_id)))
));
resource!(Children, Child => ResourceDef::new()
    .tenancy(Tenancy::none())
    .table(Table::new((
        TextColumn::new(lens!(Child.name)),
        RelationColumn::of::<Parents>(relation!(Child.parent)),
    )))
    .detail(Detail::new(RelationColumn::of::<Parents>(relation!(Child.parent)))));
resource!(ColumnThroughRelation, Child => ResourceDef::new()
    .slug("children")
    .tenancy(Tenancy::column(Child::fields().parent().tenant_id()))
    .table(Table::new(TextColumn::new(lens!(Child.name)))));
resource!(Inherited, Child => ResourceDef::new()
    .slug("inherited")
    .tenancy(Tenancy::via(Child::fields().parent().tenant_id()))
    .table(Table::new(TextColumn::new(lens!(Child.name)))));
resource!(ViaOwnColumn, Parent => ResourceDef::new()
    .slug("parents")
    .tenancy(Tenancy::via(Parent::fields().tenant_id()))
    .table(Table::new(TextColumn::new(lens!(Parent.name)))));
resource!(ScopedParents, Parent => ResourceDef::new()
    .slug("parents")
    .tenancy(Tenancy::column(Parent::fields().tenant_id()))
    .table(Table::new(TextColumn::new(lens!(Parent.name)))));
// Serves every tenant's parents on purpose.
resource!(OpenParents, Parent => ResourceDef::new()
    .slug("open-parents")
    .tenancy(Tenancy::none())
    .table(Table::new(TextColumn::new(lens!(Parent.name)))));
// Declares no tenancy over a model with a tenant column.
resource!(UndeclaredParents, Parent => ResourceDef::new()
    .slug("undeclared-parents")
    .table(Table::new(TextColumn::new(lens!(Parent.name)))));
// Declares no tenancy over a child of a model a tenant-scoped resource serves.
resource!(UndeclaredChildren, Child => ResourceDef::new()
    .slug("undeclared-children")
    .table(Table::new(TextColumn::new(lens!(Child.name)))));
// A child inheriting its tenant through `parent`, whose form declares no key.
resource!(WithoutKey, Child, NameForm => ResourceDef::new()
    .slug("children")
    .policy(creates)
    .tenancy(Tenancy::via(Child::fields().parent().tenant_id()))
    .table(Table::new(TextColumn::new(lens!(Child.name))))
    .form(Schema::default()));
resource!(OverOpenParent, Child, KeyedForm => ResourceDef::new()
.slug("children")
.policy(creates)
.tenancy(Tenancy::via(Child::fields().parent().tenant_id()))
.table(Table::new(TextColumn::new(lens!(Child.name))))
.form({
    let c = KeyedForm::controls();
    Schema::new((c.name, c.parent_id.choice().relationship::<OpenParents>()))
}));
resource!(OverScopedParent, Child, KeyedForm => ResourceDef::new()
.slug("children")
.policy(creates)
.tenancy(Tenancy::via(Child::fields().parent().tenant_id()))
.table(Table::new(TextColumn::new(lens!(Child.name))))
.form({
    let c = KeyedForm::controls();
    Schema::new((c.name, c.parent_id.choice().relationship::<ScopedParents>()))
}));
resource!(Seats, Seat => ResourceDef::new().table(Table::new(TextColumn::new(lens!(Seat.label)))));
// Declares no tenancy over a model with no tenant column, which a panel serving tenants refuses.
resource!(UndeclaredDocs, Doc => ResourceDef::new()
    .slug("docs")
    .table(Table::new(TextColumn::new(lens!(Doc.title)))));
// Serves every tenant's docs on purpose.
resource!(OpenDocs, Doc => ResourceDef::new()
    .slug("docs")
    .tenancy(Tenancy::none())
    .table(Table::new(TextColumn::new(lens!(Doc.title)))));
resource!(Seated, Child, SeatedForm => {
    let c = SeatedForm::controls();
    ResourceDef::new()
        .policy(creates)
        .table(Table::new(TextColumn::new(lens!(Child.name))))
        .form(Schema::new((c.name, c.parent_id)))
});
// Not gated, so the tenant is not stamped: a create override would set it.
resource!(AuthorResource, Author, AuthorForm => ResourceDef::new()
    .slug("authors")
    .policy(creates)
    .tenancy(Tenancy::column(Author::fields().tenant_id()))
    .table(Table::new(TextColumn::new(lens!(Author.email))))
    .form(Schema::new(AuthorForm::controls().email.unique())));
resource!(UnbackedResource, Subscriber, NicknameForm => ResourceDef::new()
    .slug("subscribers")
    .policy(creates)
    .table(nicknames())
    .form(Schema::new(NicknameForm::controls().nickname.unique())));
// `Create` keeps its default, deny; only the form is declared.
resource!(ReadOnlyResource, Subscriber, NicknameForm => ResourceDef::new()
    .slug("subscribers")
    .policy_fn(|_cx, ability| matches!(ability, Ability::ViewAny))
    .table(nicknames())
    .form(Schema::new(NicknameForm::controls().nickname.unique())));
resource!(ChromeResource, Subscriber, NicknameForm => ResourceDef::new()
    .slug("subscribers")
    .policy_fn(|_cx, ability| matches!(ability, Ability::DeleteAny | Ability::Delete(_)))
    .table(nicknames()));
resource!(ChromeOffResource, Subscriber => ResourceDef::new()
    .slug("subscribers")
    .table(nicknames()));
resource!(ViewedResource, Subscriber => ResourceDef::new()
    .slug("subscribers")
    .table(nicknames())
    .detail(Detail::new(TextColumn::new(lens!(Subscriber.nickname)))));
// Two columns over one field: a duplicate name.
resource!(DuplicateColumnResource, Doc => ResourceDef::new()
.slug("docs")
.table(Table::new((
    TextColumn::new(lens!(Doc.title)),
    TextColumn::new(lens!(Doc.title)),
))));
// A page of no rows: `Table::paginate` records zero.
resource!(ZeroPageResource, Doc => ResourceDef::new()
    .slug("docs")
    .table(Table::new(TextColumn::new(lens!(Doc.title))).paginate(0)));
resource!(Tickets, Ticket, TicketForm => ResourceDef::new()
    .slug("tickets")
    .tenancy(Tenancy::column(Ticket::fields().tenant_id()))
    .policy(creates)
    .table(Table::new(TextColumn::new(lens!(Ticket.title))))
    .create_column(Ticket::fields().tenant_id())
    .create_column(Ticket::fields().place().city()));

/// Each declaration mistake a panel mounts with is refused, naming the type, site and kind of
/// every error, and the valid declaration beside each mounts.
#[tokio::test]
async fn a_mount_refuses_each_misdeclaration() {
    let db = schema_only(toasty::models!(
        Parent, Child, Subscriber, Doc, Seat, Ticket, Author
    ))
    .await;
    let panel = || Panel::new("admin").auth(Auth::disabled());
    // The relationship fields' option sources are the panel's own resources.
    let via_panel = || {
        panel()
            .resource::<OpenParents>()
            .resource::<ScopedParents>()
    };
    let mounted = |panel: Panel| mount(db.clone(), panel);

    let unguarded = DeclarationErrorKind::Tenancy {
        fault: TenancyFault::UnguardedForeignKey {
            relation: "parent".to_string(),
            keys: vec!["parent_id".to_string()],
        },
    };
    let unregistered = DeclarationErrorKind::UnregisteredLabelSource {
        column: "parent".to_string(),
        source: std::any::type_name::<Parents>(),
    };
    let unbacked = || DeclarationErrorKind::UniqueWithoutIndex {
        field: "nickname".to_string(),
    };

    let refusals = [
        (
            "a Db missing the auth models",
            refused(mounted(Panel::new("admin"))),
            vec![panel_error(DeclarationErrorKind::MissingAuthModels {
                models: vec!["AuthSession", "AdminUser"],
            })],
        ),
        // A relation column labelled by an unregistered resource would show each record by its
        // bare key.
        (
            "a relation column of an unregistered resource",
            refused(mounted(panel().resource::<Children>())),
            vec![
                of::<Children>(Site::Table, unregistered.clone()),
                of::<Children>(Site::Detail, unregistered),
            ],
        ),
        // A relationship filter over an unregistered resource would offer no option.
        (
            "a relationship filter of an unregistered resource",
            refused(mounted(panel().resource::<FilteredChildren>())),
            vec![of::<FilteredChildren>(
                Site::Table,
                DeclarationErrorKind::UnregisteredOptionSource {
                    field: "parent_id".to_string(),
                    source: std::any::type_name::<Parents>(),
                },
            )],
        ),
        (
            "a tenancy column through a relation",
            refused(mounted(panel().resource::<ColumnThroughRelation>())),
            vec![of::<ColumnThroughRelation>(
                Site::Tenancy,
                DeclarationErrorKind::Tenancy {
                    fault: TenancyFault::ColumnNotAField,
                },
            )],
        ),
        (
            "a tenancy via over its own column",
            refused(mounted(panel().resource::<ViaOwnColumn>())),
            vec![of::<ViaOwnColumn>(
                Site::Tenancy,
                DeclarationErrorKind::Tenancy {
                    fault: TenancyFault::ViaOwnColumn,
                },
            )],
        ),
        (
            "no tenancy over a model with a tenant column",
            refused(mounted(panel().resource::<UndeclaredParents>())),
            vec![of::<UndeclaredParents>(
                Site::Tenancy,
                DeclarationErrorKind::Tenancy {
                    fault: TenancyFault::Undeclared {
                        model: "Parent",
                        columns: vec!["tenant_id"],
                    },
                },
            )],
        ),
        (
            "no tenancy over a child of a tenant-scoped model",
            refused(mounted(
                panel()
                    .resource::<UndeclaredChildren>()
                    .resource::<ScopedParents>(),
            )),
            vec![of::<UndeclaredChildren>(
                Site::Tenancy,
                DeclarationErrorKind::Tenancy {
                    fault: TenancyFault::UndeclaredChild {
                        model: "Child",
                        relation: "parent".to_string(),
                    },
                },
            )],
        ),
        (
            "no tenancy beside a tenant-scoped resource",
            refused(mounted(
                panel()
                    .resource::<ScopedParents>()
                    .resource::<UndeclaredDocs>(),
            )),
            vec![of::<UndeclaredDocs>(
                Site::Tenancy,
                DeclarationErrorKind::Tenancy {
                    fault: TenancyFault::UndeclaredOnTenantPanel { model: "Doc" },
                },
            )],
        ),
        // A tenancy `via` needs the form to write the relation's key through a relationship
        // field over a tenant-scoped resource.
        (
            "a tenancy via whose form writes no key",
            refused(mounted(via_panel().resource::<WithoutKey>())),
            vec![
                of::<WithoutKey>(Site::Form, unguarded.clone()),
                of::<WithoutKey>(
                    Site::Form,
                    DeclarationErrorKind::UnwrittenColumn {
                        column: "parent_id".to_string(),
                    },
                ),
            ],
        ),
        (
            "a tenancy via over an open parent",
            refused(mounted(via_panel().resource::<OverOpenParent>())),
            vec![of::<OverOpenParent>(Site::Form, unguarded)],
        ),
        // No option value spells a composite key; `Seats` is not registered either.
        (
            "a relationship over a composite key",
            refused(mounted(panel().resource::<Seated>())),
            vec![
                of::<Seated>(
                    Site::Form,
                    DeclarationErrorKind::CompositeKeyChoice {
                        field: "parent_id".to_string(),
                    },
                ),
                of::<Seated>(
                    Site::Form,
                    DeclarationErrorKind::UnregisteredOptionSource {
                        field: "parent_id".to_string(),
                        source: std::any::type_name::<Seats>(),
                    },
                ),
            ],
        ),
        (
            "a unique marker without a unique index",
            refused(mounted(panel().resource::<UnbackedResource>())),
            vec![of::<UnbackedResource>(Site::Form, unbacked())],
        ),
        (
            "an unbacked unique marker when create is denied",
            refused(mounted(panel().resource::<ReadOnlyResource>())),
            vec![of::<ReadOnlyResource>(Site::Form, unbacked())],
        ),
        (
            "a duplicate table column",
            refused(mounted(panel().resource::<DuplicateColumnResource>())),
            vec![of::<DuplicateColumnResource>(
                Site::Table,
                DeclarationErrorKind::DuplicateColumn {
                    name: "title".to_string(),
                },
            )],
        ),
        (
            "a table with a zero page size",
            refused(mounted(panel().resource::<ZeroPageResource>())),
            vec![of::<ZeroPageResource>(
                Site::Table,
                DeclarationErrorKind::ZeroPageSize,
            )],
        ),
        // `create_column` refuses the tenant column, which the framework stamps, and an embedded
        // path, which names no one field; the embedded column is then unwritten too.
        (
            "a create column it cannot honor",
            refused(mounted(panel().resource::<Tickets>())),
            vec![
                of::<Tickets>(
                    Site::Registration,
                    DeclarationErrorKind::TraversalLens { steps: 2 },
                ),
                of::<Tickets>(
                    Site::Registration,
                    DeclarationErrorKind::CreateColumnsNameTenant {
                        column: "tenant_id".to_string(),
                    },
                ),
                of::<Tickets>(
                    Site::Form,
                    DeclarationErrorKind::UnwrittenColumn {
                        column: "place".to_string(),
                    },
                ),
            ],
        ),
    ];
    for (label, refused, expected) in refusals {
        assert_eq!(refused, expected, "{label}");
    }

    let mounts = [
        (
            "a relation column of a registered resource",
            mounted(panel().resource::<Children>().resource::<Parents>()),
        ),
        (
            "`Tenancy::none` beside a tenant-scoped resource",
            mounted(panel().resource::<ScopedParents>().resource::<OpenDocs>()),
        ),
        (
            "`Tenancy::via` through a relation",
            mounted(panel().resource::<Inherited>()),
        ),
        (
            "a key over the tenant-scoped parent",
            mounted(via_panel().resource::<OverScopedParent>()),
        ),
        (
            "a composite unique index backing the marker",
            mounted(panel().resource::<AuthorResource>()),
        ),
        (
            "a keyed table with chrome",
            mounted(panel().resource::<ChromeResource>()),
        ),
        (
            "a keyed table without chrome",
            mounted(panel().resource::<ChromeOffResource>()),
        ),
        (
            "a keyed table with a detail view",
            mounted(panel().resource::<ViewedResource>()),
        ),
    ];
    for (label, mounted) in mounts {
        mounted.expect(label);
    }
}
