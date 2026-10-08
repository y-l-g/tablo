//! Dependent choices over HTTP: a city choice offers the cities of the posted country, through the
//! options endpoint and on submit.

use std::collections::HashMap;

use tablo::{
    Ability, Action, ActionInput, ActionInputFault, DeclarationErrorKind, Field, FieldError,
    NoForm, Resource, ResourceDef, Schema, Site, Table, TextColumn, lens,
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::framework::common::{body_string, get, memory_db, mount, panel, post_fields, refusal};

#[derive(Debug, Clone, toasty::Model)]
struct Country {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
}

#[derive(Debug, Clone, toasty::Model)]
struct City {
    #[key]
    #[auto]
    id: Uuid,
    country_id: Uuid,
    name: String,
}

#[derive(Debug, Clone, toasty::Model)]
struct Address {
    #[key]
    #[auto]
    id: Uuid,
    country_id: Uuid,
    city_id: Uuid,
}

fn reads(_cx: &Cx, ability: Ability<'_, impl toasty::schema::Model>) -> bool {
    ability.is_read()
}

struct CountryResource;

impl Resource for CountryResource {
    type Model = Country;
    type Form = NoForm<Country>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(reads)
            .table(Table::new(TextColumn::new(lens!(Country.name))))
            .record_title(lens!(Country.name))
    }
}

struct CityResource;

impl Resource for CityResource {
    type Model = City;
    type Form = NoForm<City>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(reads)
            .table(Table::new(TextColumn::new(lens!(City.name))))
            .record_title(lens!(City.name))
    }
}

struct AddressResource;

impl Resource for AddressResource {
    type Model = Address;
    type Form = AddressForm;

    fn declare() -> ResourceDef<Self> {
        let c = AddressForm::controls();
        let city = c
            .city_id
            .depends_on(&c.country_id, City::fields().country_id());
        ResourceDef::new()
            .slug("addresses")
            .policy(|_cx: &Cx, _ability: Ability<'_, Address>| true)
            .table(Table::new(TextColumn::new(lens!(Address.id))))
            .form(Schema::new((c.country_id, city)))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Address)]
struct AddressForm {
    #[form(relationship = CountryResource)]
    country_id: Uuid,
    #[form(relationship = CityResource)]
    city_id: Uuid,
}

/// France with Paris, Germany with Berlin, and the panel over them.
struct World {
    db: Db,
    router: Router,
    france: Uuid,
    germany: Uuid,
    paris: Uuid,
    berlin: Uuid,
}

async fn world() -> World {
    let mut db = memory_db(toasty::models!(Country, City, Address)).await;
    let mut ids = Vec::new();
    for (country, city) in [("France", "Paris"), ("Germany", "Berlin")] {
        let country = toasty::create!(Country { name: country })
            .exec(&mut db)
            .await
            .unwrap();
        let city = toasty::create!(City {
            country_id: country.id,
            name: city,
        })
        .exec(&mut db)
        .await
        .unwrap();
        ids.push((country.id, city.id));
    }
    let router = mount(
        db.clone(),
        panel()
            .resource::<CountryResource>()
            .resource::<CityResource>()
            .resource::<AddressResource>(),
    )
    .expect("panel builds");
    World {
        db,
        router,
        france: ids[0].0,
        paris: ids[0].1,
        germany: ids[1].0,
        berlin: ids[1].1,
    }
}

async fn addresses(db: &Db) -> Vec<Address> {
    let mut db = db.clone();
    Address::all().exec(&mut db).await.unwrap()
}

/// The endpoint answers the cities of the parent value it is given, and none for a blank one.
#[tokio::test]
async fn the_options_endpoint_answers_the_rows_of_the_parent_value() {
    let world = world().await;
    let options = async |query: &str| {
        let response = get(&world.router, &format!("/admin/addresses/options?{query}")).await;
        (response.status(), body_string(response).await)
    };
    let (status, html) = options(&format!("field=city_id&parent={}", world.france)).await;
    assert_eq!(status, 200, "{html}");
    assert!(
        html.contains(&world.paris.to_string()) && html.contains("Paris"),
        "{html}"
    );
    assert!(!html.contains("Berlin"), "{html}");

    let (status, html) = options("field=city_id&parent=").await;
    assert_eq!(status, 200);
    assert!(
        !html.contains("<option"),
        "a blank parent offers nothing: {html}"
    );

    let (status, _) = options("field=country_id").await;
    assert_eq!(
        status, 400,
        "an independent, non-searchable choice is not served"
    );
}

/// A create naming a city of another country is refused; one of the posted country is written.
#[tokio::test]
async fn a_create_refuses_a_city_of_another_country() {
    let world = world().await;
    let create = async |country: Uuid, city: Uuid| {
        post_fields(
            &world.router,
            "/admin/addresses/create",
            &[
                ("country_id", &country.to_string()),
                ("city_id", &city.to_string()),
            ],
        )
        .await
    };
    let response = create(world.france, world.berlin).await;
    assert!(
        !response.status().is_redirection(),
        "the mismatch is refused"
    );
    let html = body_string(response).await;
    assert!(html.contains("City id is invalid"), "{html}");
    assert!(
        html.contains("Paris") && !html.contains(">Berlin<"),
        "the re-rendered form offers the posted country's cities: {html}"
    );
    assert!(addresses(&world.db).await.is_empty());

    let response = create(world.germany, world.berlin).await;
    assert!(
        response.status().is_redirection(),
        "{}",
        body_string(response).await
    );
    assert_eq!(addresses(&world.db).await[0].city_id, world.berlin);
}

/// An edit that posts only the city reads the stored country, as the parse does.
#[tokio::test]
async fn an_edit_without_the_parent_reads_its_stored_value() {
    let world = world().await;
    let response = post_fields(
        &world.router,
        "/admin/addresses/create",
        &[
            ("country_id", &world.france.to_string()),
            ("city_id", &world.paris.to_string()),
        ],
    )
    .await;
    assert!(response.status().is_redirection());
    let id = addresses(&world.db).await[0].id;
    let edit = format!("/admin/addresses/{id}/edit");

    let html = body_string(get(&world.router, &edit).await).await;
    assert!(
        html.contains("Paris") && !html.contains(">Berlin<"),
        "{html}"
    );

    let response = post_fields(
        &world.router,
        &edit,
        &[("city_id", &world.berlin.to_string())],
    )
    .await;
    assert!(
        !response.status().is_redirection(),
        "a city of another country is refused"
    );
    assert_eq!(addresses(&world.db).await[0].city_id, world.paris);
}

/// No route serves an action input's options, so mounting refuses a dependent choice there.
#[tokio::test]
async fn an_action_input_cannot_hold_a_dependent_choice() {
    struct Moving;

    impl ActionInput for Moving {
        fn schema() -> Schema {
            let country =
                Field::choice(lens!(Address.country_id)).relationship::<CountryResource>();
            let city = Field::choice(lens!(Address.city_id))
                .relationship::<CityResource>()
                .depends_on(&country, City::fields().country_id());
            Schema::new((country, city))
        }

        fn parse(_cx: &Cx, _values: &HashMap<String, String>) -> Result<Self, Vec<FieldError>> {
            Ok(Self)
        }
    }

    struct Move;

    impl Action<MovingResource> for Move {
        type Input = Moving;
        const NAME: &'static str = "move";

        fn label(_cx: &Cx) -> String {
            "Move".to_string()
        }

        async fn run(
            _: &Cx,
            _: &[Address],
            _: Moving,
            _: &mut dyn toasty::Executor,
        ) -> topcoat::Result<()> {
            Ok(())
        }
    }

    struct MovingResource;

    impl Resource for MovingResource {
        type Model = Address;
        type Form = NoForm<Address>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .policy(|_cx: &Cx, ability: Ability<'_, Address>| {
                    !matches!(ability, Ability::Create)
                })
                .table(Table::new(TextColumn::new(lens!(Address.id))))
                .action::<Move>()
        }
    }

    let db = memory_db(toasty::models!(Country, City, Address)).await;
    let errors = refusal(mount(
        db,
        panel()
            .resource::<CountryResource>()
            .resource::<CityResource>()
            .resource::<MovingResource>(),
    ));
    assert_eq!(
        errors
            .into_iter()
            .map(|error| (error.site, error.kind))
            .collect::<Vec<_>>(),
        [(
            Site::Registration,
            DeclarationErrorKind::ActionInput {
                action: "move",
                fault: ActionInputFault::DependentChoice("city_id".to_string()),
            }
        )]
    );
}
