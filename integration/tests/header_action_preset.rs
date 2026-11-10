//! A header action's button placed in a page's content with the input the page chose, which it
//! posts without asking for it.

use http::header::LOCATION;
use tablo::{HeaderAction, Page, PageDef, header_action_button};
use toasty::Db;
use topcoat::{
    context::Cx,
    router::Router,
    view::{View, view},
};
use uuid::Uuid;

use crate::common::{body_string, get, input_value, mount, panel, post_fields, test_db};

#[derive(Debug, toasty::Model, Clone)]
struct Subscription {
    #[key]
    #[auto]
    id: Uuid,
    plan: String,
    yearly: bool,
}

/// What subscribing takes: a plan and how often it bills.
#[derive(tablo::FormInput)]
struct Choice {
    plan: String,
    yearly: bool,
}

struct Subscribe;

impl HeaderAction for Subscribe {
    type Input = Choice;
    const NAME: &'static str = "subscribe";

    async fn run(
        _cx: &Cx,
        choice: Choice,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<tablo::Outcome, tablo::WriteError<ChoiceField>> {
        toasty::create!(Subscription {
            plan: choice.plan,
            yearly: choice.yearly,
        })
        .exec(&mut *ex)
        .await?;
        Ok(tablo::Outcome::done())
    }
}

/// A plan card per billing interval, each its own Subscribe button.
struct PlansPage;

impl Page for PlansPage {
    fn declare() -> PageDef {
        PageDef::new().header_action::<Subscribe>()
    }

    async fn render(cx: &Cx) -> topcoat::Result<impl View> {
        let yearly = Choice {
            plan: "pro".to_string(),
            yearly: true,
        };
        Ok(view! {
            tablo::ui::page(
                tablo::ui::page_content(
                    <div data-card="pro-yearly">
                        (header_action_button::<Self, Subscribe>(cx, &yearly))
                    </div>
                )
            )
        })
    }
}

async fn router() -> (Db, Router) {
    let db = test_db(toasty::models!(Subscription)).await;
    let router = mount(db.clone(), panel().page::<PlansPage>()).expect("panel builds");
    (db, router)
}

/// The button posts the page's input to the action's route without a dialog asking for it, and
/// the action runs on that input, landing back on the page.
#[tokio::test]
async fn a_placed_button_posts_the_input_its_page_chose() {
    let (db, router) = router().await;
    let html = body_string(get(&router, "/admin/plans").await).await;
    assert!(
        html.contains("<form method=\"post\" action=\"/admin/plans/-/actions/subscribe\""),
        "{html}"
    );
    assert_eq!(input_value(&html, "plan").as_deref(), Some("pro"));
    assert_eq!(input_value(&html, "yearly").as_deref(), Some("true"));
    assert!(
        !html.contains("action-input-"),
        "no dialog asks for the input: {html}"
    );

    // What the button's form holds, as a click posts it.
    let fields = [("plan", "pro"), ("yearly", "true")];
    let response = post_fields(&router, "/admin/plans/-/actions/subscribe", &fields).await;
    assert_eq!(response.status(), 303);
    assert_eq!(response.headers().get(LOCATION).unwrap(), "/admin/plans");
    let subscriptions = Subscription::all().exec(&mut db.clone()).await.unwrap();
    assert_eq!(subscriptions.len(), 1);
    assert_eq!(subscriptions[0].plan, "pro");
    assert!(subscriptions[0].yearly);
}
