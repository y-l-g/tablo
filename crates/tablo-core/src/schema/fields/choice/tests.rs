//! Dependent choices: the related rows whose column equals the parent field's value.

use toasty::stmt::{List, Query};
use topcoat::context::{Cx, CxTestBuilder};

use crate::{
    Ability, DeclarationErrorKind,
    schema::{ChoiceField, Field, OptionSource, Schema},
    test_support::memory_db,
    values::FormValue,
};

#[derive(Debug, toasty::Model, Clone)]
struct Country {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
}

#[derive(Debug, toasty::Model, Clone)]
struct City {
    #[key]
    #[auto]
    id: uuid::Uuid,
    country_id: uuid::Uuid,
    name: String,
}

#[derive(Debug, toasty::Model)]
struct Address {
    #[key]
    #[auto]
    id: uuid::Uuid,
    country_id: uuid::Uuid,
    city_id: uuid::Uuid,
}

macro_rules! source {
    ($source:ident, $model:ident) => {
        struct $source;
        impl OptionSource for $source {
            type Model = $model;
            fn label(_cx: &Cx, record: &$model) -> String {
                record.name.clone()
            }
            fn scoped_query(_cx: &Cx) -> topcoat::Result<Query<List<$model>>> {
                Ok(Query::all())
            }
            fn allows(_cx: &Cx, ability: Ability<'_, $model>) -> bool {
                ability.is_read()
            }
        }
    };
}

source!(Countries, Country);
source!(Cities, City);

fn country() -> ChoiceField {
    Field::choice(Address::fields().country_id()).relationship::<Countries>()
}

fn city(country: &ChoiceField) -> ChoiceField {
    Field::choice(Address::fields().city_id())
        .relationship::<Cities>()
        .depends_on(country, City::fields().country_id())
}

/// Two countries with one city each.
struct World {
    cx: Cx,
    france: String,
    paris: String,
    berlin: String,
}

async fn world() -> World {
    let mut db = memory_db(toasty::models!(Country, City, Address)).await;
    let mut country_with = async |country: &str, city: &str| {
        let country = toasty::create!(Country {
            name: country.to_string()
        })
        .exec(&mut db)
        .await
        .unwrap();
        let city = toasty::create!(City {
            country_id: country.id,
            name: city.to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
        (country.id.to_string(), city.id.to_string())
    };
    let (france, paris) = country_with("France", "Paris").await;
    let (_, berlin) = country_with("Germany", "Berlin").await;
    World {
        cx: CxTestBuilder::new().app_context(db).build(),
        france,
        paris,
        berlin,
    }
}

/// A submission must name a row of the parent value it posts, at validation and in the write.
#[tokio::test]
async fn a_dependent_choice_refuses_a_row_of_another_parent_value() {
    let world = world().await;
    let country = country();
    let city = city(&country);
    let check = async |value: &str, parent: Option<&str>| {
        city.validate_exists(&world.cx, &value.into(), parent, None)
            .await
    };
    assert!(check(&world.paris, Some(&world.france)).await.is_empty());
    for parent in [
        Some(world.france.as_str()),
        Some(""),
        None,
        Some("not-a-key"),
    ] {
        assert_eq!(
            check(&world.berlin, parent).await,
            ["City id is invalid"],
            "parent {parent:?}"
        );
    }

    let mut handle = crate::db::db(&world.cx);
    let mut tx = handle.transaction().await.unwrap();
    assert!(
        city.recheck(
            &world.cx,
            &(&world.paris).into(),
            Some(&world.france),
            None,
            &mut tx
        )
        .await
        .is_empty()
    );
    assert_eq!(
        city.recheck(
            &world.cx,
            &(&world.berlin).into(),
            Some(&world.france),
            None,
            &mut tx
        )
        .await,
        ["City id is invalid"]
    );
}

#[test]
fn a_dependent_choice_names_a_column_of_its_source_and_a_placed_parent() {
    let misdeclared = |field: ChoiceField| {
        let country = country();
        let field = field.depends_on(&country, City::fields().country_id());
        Schema::new((country, field)).declaration_errors()
    };
    let error = [DeclarationErrorKind::MisdeclaredDependentChoice {
        field: "city_id".to_string(),
    }];
    assert_eq!(
        misdeclared(Field::choice(Address::fields().city_id()).relationship::<Countries>()),
        error,
        "a column of another model than the source"
    );
    assert_eq!(
        misdeclared(Field::choice(Address::fields().city_id()).options(["a"])),
        error,
        "no relationship to narrow"
    );

    assert_eq!(
        Schema::new(city(&country())).declaration_errors(),
        [DeclarationErrorKind::UnplacedParentField {
            field: "city_id".to_string(),
            parent: "country_id".to_string(),
        }]
    );
    let country = country();
    let city = city(&country);
    assert!(Schema::new((country, city)).declaration_errors().is_empty());
}

/// Past the cap a choice that cannot search keeps the key the record stores, which its select
/// renders, and refuses any other.
#[tokio::test]
async fn a_choice_past_the_cap_keeps_its_stored_key() {
    let world = world().await;
    let france: uuid::Uuid = world.france.parse().unwrap();
    let mut db = crate::db::db(&world.cx);
    for i in 0..crate::schema::MAX_RELATIONSHIP_OPTIONS {
        toasty::create!(City {
            country_id: france,
            name: format!("Town {i}"),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let country = country();
    let city = city(&country);
    let paris = FormValue::from(world.paris.as_str());
    let check = async |stored: Option<&FormValue>| {
        city.validate_exists(&world.cx, &paris, Some(&world.france), stored)
            .await
    };
    assert!(check(Some(&paris)).await.is_empty(), "the stored key");
    assert_eq!(
        check(None).await,
        ["City id could not load options, retry"],
        "a key the record does not store"
    );
}

/// Past the cap a searchable dependent choice checks the posted key among its parent's rows.
#[tokio::test]
async fn a_searchable_dependent_choice_past_the_cap_checks_among_its_parent_rows() {
    let world = world().await;
    let france: uuid::Uuid = world.france.parse().unwrap();
    let mut db = crate::db::db(&world.cx);
    for i in 0..crate::schema::MAX_RELATIONSHIP_OPTIONS {
        toasty::create!(City {
            country_id: france,
            name: format!("Town {i}"),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let country = country();
    let city = city(&country).searchable();
    assert!(
        city.validate_exists(&world.cx, &(&world.paris).into(), Some(&world.france), None)
            .await
            .is_empty()
    );
    assert_eq!(
        city.validate_exists(
            &world.cx,
            &(&world.berlin).into(),
            Some(&world.france),
            None
        )
        .await,
        ["City id is invalid"]
    );
}
