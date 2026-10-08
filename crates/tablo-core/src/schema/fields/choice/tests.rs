//! Dependent choices: the related rows whose column equals the parent field's value.

use std::collections::HashMap;

use toasty::stmt::{List, Query};
use topcoat::context::{Cx, CxTestBuilder};

use crate::{
    Ability, DeclarationErrorKind,
    form::FieldErrors,
    schema::{ChoiceField, Field, OptionSource, Schema, Source},
    test_support::{Html as _, memory_db},
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

async fn render(world: &World, values: &HashMap<String, String>) -> String {
    let country = country();
    let city = city(&country);
    let errors = FieldErrors::new();
    Schema::new((country, city))
        .render(&world.cx, Source::form(values, &errors))
        .await
        .html(&world.cx)
        .await
}

/// The city select offers the chosen country's cities, and none before a country is chosen.
#[tokio::test]
async fn a_dependent_choice_offers_the_rows_of_its_parent_value() {
    let world = world().await;
    let html = render(
        &world,
        &HashMap::from([("country_id".to_string(), world.france.clone())]),
    )
    .await;
    assert!(
        html.contains("Paris"),
        "the parent's row is offered: {html}"
    );
    assert!(
        !html.contains("Berlin"),
        "another parent's row is not: {html}"
    );
    assert!(
        html.contains("data-options-parent=\"country_id\"")
            && html.contains(&format!("data-options-parent-value=\"{}\"", world.france)),
        "the script learns what to refetch on: {html}"
    );

    let html = render(&world, &HashMap::new()).await;
    assert!(
        !html.contains("Paris") && !html.contains("Berlin"),
        "a blank parent offers no city: {html}"
    );
}

/// A submission must name a row of the parent value it posts, at validation and in the write.
#[tokio::test]
async fn a_dependent_choice_refuses_a_row_of_another_parent_value() {
    let world = world().await;
    let country = country();
    let city = city(&country);
    let check = async |value: &str, parent: Option<&str>| {
        city.validate_exists(&world.cx, value, parent).await
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
        city.recheck(&world.cx, &world.paris, Some(&world.france), &mut tx)
            .await
            .is_empty()
    );
    assert_eq!(
        city.recheck(&world.cx, &world.berlin, Some(&world.france), &mut tx)
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
