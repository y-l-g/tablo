use super::*;
use crate::{Tenancy, TenantId, lens, test_support::memory_db};

/// A model with its own tenant column.
#[derive(Debug, Clone, toasty::Model)]
struct Owned {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: TenantId,
    name: String,
}

/// A resource scoped by its model's tenant column.
struct OwnedResource;

impl Resource for OwnedResource {
    type Model = Owned;
    type Form = crate::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .tenancy(Tenancy::column(Owned::fields().tenant_id()))
            .table(crate::table::Table::new(crate::table::TextColumn::new(
                lens!(Owned.name),
            )))
    }

    fn query(_cx: &Cx) -> toasty::stmt::Query<List<Owned>> {
        toasty::stmt::Query::<List<Owned>>::all().filter(Owned::fields().name().ne("Hidden"))
    }
}

/// The tenancy filter is ANDed onto the resource's own query, and a request
/// with no tenant runs no query at all.
#[tokio::test]
async fn tenancy_is_anded_onto_the_base_query() {
    let mut db = memory_db(toasty::models!(Owned)).await;
    let mine = uuid::Uuid::new_v4();
    let theirs = uuid::Uuid::new_v4();
    for (tenant_id, name) in [
        (TenantId::from(mine), "Mine"),
        (TenantId::from(mine), "Hidden"),
        (TenantId::from(theirs), "Theirs"),
    ] {
        toasty::create!(Owned { tenant_id, name })
            .exec(&mut db)
            .await
            .unwrap();
    }
    let tenantless = crate::test_support::panel_cx::<OwnedResource>(&db);
    let cx = tenantless.with(crate::Tenant(mine));
    let mut db = crate::db::db(&cx);
    let rows = scoped_query::<OwnedResource>(&cx)
        .expect("the request has a tenant")
        .exec(&mut db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Mine");

    assert!(scoped_query::<OwnedResource>(&tenantless).is_err());
}

/// A model whose tenant column accepts no tenant.
#[derive(Debug, Clone, toasty::Model)]
struct Assignable {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: Option<TenantId>,
    name: String,
}

/// A resource scoped by its nullable tenant column.
struct AssignableResource;

impl Resource for AssignableResource {
    type Model = Assignable;
    type Form = OptionTenantForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .tenancy(Tenancy::column(Assignable::fields().tenant_id()))
            .table(crate::table::Table::new(crate::table::TextColumn::new(
                lens!(Assignable.name),
            )))
    }
}

#[derive(crate::RecordForm)]
#[form(model = Assignable)]
struct OptionTenantForm {
    name: String,
}

/// A nullable tenant column is stamped `Some(request tenant)`, and scoped so
/// that neither another tenant's row nor a tenantless one is served.
#[tokio::test]
async fn a_nullable_tenant_column_is_stamped_and_scoped() {
    let mut db = memory_db(toasty::models!(Assignable)).await;
    let mine = uuid::Uuid::new_v4();
    let theirs = uuid::Uuid::new_v4();
    for (tenant_id, name) in [(Some(TenantId::from(theirs)), "Theirs"), (None, "Nobody")] {
        toasty::create!(Assignable { tenant_id, name })
            .exec(&mut db)
            .await
            .unwrap();
    }
    let cx = crate::test_support::panel_cx::<AssignableResource>(&db).with(crate::Tenant(mine));
    let mut db = crate::db::db(&cx);
    let created = write_create::<AssignableResource>(
        &cx,
        OptionTenantForm {
            name: "Mine".to_string(),
        },
        &mut db,
    )
    .await
    .expect("the create runs");
    assert_eq!(created.tenant_id, Some(TenantId::from(mine)));

    let rows = scoped_query::<AssignableResource>(&cx)
        .expect("the request has a tenant")
        .exec(&mut db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "only the request tenant's row is served");
    assert_eq!(rows[0].name, "Mine");
}

#[derive(Debug, Clone, toasty::Model)]
struct Note {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
}

/// A resource with no record label: the title keeps the resource's label and the record key.
struct Unlabelled;

impl Resource for Unlabelled {
    type Model = Note;
    type Form = crate::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().table(crate::table::Table::new(crate::table::TextColumn::new(
            lens!(Note.title),
        )))
    }
}

/// A resource that titles its records with the note's title.
struct Titled;

impl Resource for Titled {
    type Model = Note;
    type Form = crate::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .table(crate::table::Table::new(crate::table::TextColumn::new(
                lens!(Note.title),
            )))
            .record_title(lens!(Note.title))
    }
}

fn note() -> Note {
    Note {
        id: uuid::Uuid::nil(),
        title: "A Title".to_string(),
    }
}

#[test]
fn a_resource_without_a_record_title_titles_the_record_with_its_key() {
    assert_eq!(
        crate::test_support::mounted::<Unlabelled>().record_title(&note(), "8f14e45f"),
        "Note 8f14e45f"
    );
}

#[test]
fn a_declared_record_title_titles_the_record_with_its_column() {
    let mounted = crate::test_support::mounted::<Titled>();
    assert_eq!(mounted.record_title(&note(), "8f14e45f"), "A Title");
    let untitled = Note {
        title: " ".to_string(),
        ..note()
    };
    assert_eq!(
        mounted.record_title(&untitled, "8f14e45f"),
        "Note 8f14e45f",
        "a record whose column is blank falls back to the key"
    );
}
