//! A relation through other relations, over test-local models: a customer's payments, which
//! belong to its orders. The table lists what the path reaches, carries the payments' own
//! actions, and offers no write of the relation itself.

use tablo::{Action, ActionDef, Places, Relation, Resource, ResourceDef, Table, TextColumn, lens};
use toasty::{Db, Deferred};
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{body_string, get, mount, panel, post_fields, rows, test_db};

#[derive(Debug, toasty::Model, tablo::PanelModel, Clone)]
struct Customer {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
    #[has_many]
    orders: Deferred<Vec<Order>>,
    #[has_many(via = orders.payments)]
    payments: Deferred<Vec<Payment>>,
}

#[derive(Debug, toasty::Model, tablo::PanelModel, Clone)]
struct Order {
    #[key]
    #[auto]
    id: Uuid,
    #[index]
    customer_id: Uuid,
    #[belongs_to(key = customer_id, references = id)]
    customer: Deferred<Customer>,
    #[has_many]
    payments: Deferred<Vec<Payment>>,
}

#[derive(Debug, toasty::Model, tablo::PanelModel, Clone)]
struct Payment {
    #[key]
    #[auto]
    id: Uuid,
    #[index]
    order_id: Uuid,
    #[belongs_to(key = order_id, references = id)]
    order: Deferred<Order>,
    memo: String,
}

struct CustomerResource;

impl Resource for CustomerResource {
    type Model = Customer;
    type Form = tablo::NoForm<Customer>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy_fn(|_cx, _ability| true)
            .table(Table::new(TextColumn::new(lens!(Customer.name))))
            .detail(tablo::Detail::new(TextColumn::new(lens!(Customer.name))))
            .relation(Relation::through::<PaymentResource>(
                Customer::fields().payments(),
            ))
    }
}

struct PaymentResource;

impl Resource for PaymentResource {
    type Model = Payment;
    type Form = tablo::NoForm<Payment>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy_fn(|_cx, _ability| true)
            .table(Table::new(TextColumn::new(lens!(Payment.memo))))
            .action::<Refund>()
    }
}

/// A payment's own action, which the customer's table carries on each row.
struct Refund;

impl Action<PaymentResource> for Refund {
    type Input = ();
    const NAME: &'static str = "refund";

    fn declare() -> ActionDef<PaymentResource> {
        ActionDef::new().places(Places::ROW)
    }

    async fn run(
        _: &Cx,
        _: &[Payment],
        _: (),
        _: &mut dyn toasty::Executor,
    ) -> topcoat::Result<tablo::Outcome, tablo::WriteError<()>> {
        Ok(tablo::Outcome::done())
    }
}

/// Ada pays two orders, one of them twice; Bob pays one. Answers the router, Ada, and Bob's
/// payment.
async fn fixture() -> (Router, Customer, Payment) {
    let mut db: Db = test_db(toasty::models!(Customer, Order, Payment)).await;
    let ada = toasty::create!(Customer { name: "Ada" })
        .exec(&mut db)
        .await
        .unwrap();
    let bob = toasty::create!(Customer { name: "Bob" })
        .exec(&mut db)
        .await
        .unwrap();
    let mut bobs = None;
    for (customer, memos) in [
        (&ada, &["ada-first", "ada-second"][..]),
        (&ada, &["ada-third"][..]),
        (&bob, &["bob-only"][..]),
    ] {
        let order = toasty::create!(Order {
            customer_id: customer.id,
        })
        .exec(&mut db)
        .await
        .unwrap();
        for memo in memos {
            let payment = toasty::create!(Payment {
                order_id: order.id,
                memo: memo.to_string(),
            })
            .exec(&mut db)
            .await
            .unwrap();
            if customer.id == bob.id {
                bobs = Some(payment);
            }
        }
    }
    let router = mount(
        db,
        panel()
            .resource::<CustomerResource>()
            .resource::<PaymentResource>(),
    )
    .expect("panel builds");
    (router, ada, bobs.expect("Bob pays once"))
}

/// The memos the page's table lists, sorted.
fn listed_memos(html: &str) -> Vec<String> {
    let mut memos: Vec<String> = rows(html)
        .into_iter()
        .flat_map(|row| row.cells)
        .filter(|cell| cell.starts_with("ada-") || cell.starts_with("bob-"))
        .collect();
    memos.sort();
    memos
}

#[tokio::test]
async fn the_page_lists_the_payments_of_the_customers_orders_with_their_own_actions() {
    let (router, ada, _bobs) = fixture().await;
    let html = body_string(get(&router, &format!("/admin/customers/{}", ada.id)).await).await;
    assert_eq!(
        listed_memos(&html),
        ["ada-first", "ada-second", "ada-third"],
        "{html}"
    );
    assert!(html.contains("data-relation=\"payments\""), "{html}");
    assert!(
        !html.contains("/admin/payments/-/create"),
        "no create button: {html}"
    );
    for key in tablo::testing::row_keys(&html) {
        let refund = format!("/admin/payments/{key}/-/actions/refund");
        assert!(
            tablo::testing::action_label(&html, &refund).is_some(),
            "row {key} carries Refund: {html}"
        );
    }
}

/// The relation takes no link: attaching another customer's payment finds no relation to write.
#[tokio::test]
async fn the_relation_takes_no_attach() {
    let (router, ada, bobs) = fixture().await;
    let attach = format!(
        "/admin/customers/{}/-/relations/payments/-/actions/attach",
        ada.id
    );
    let bob_payment = bobs.id.to_string();
    let response = post_fields(&router, &attach, &[("record", &bob_payment)]).await;
    assert_eq!(response.status(), 404);
}
