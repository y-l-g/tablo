//! A hand-written record form whose errors no control renders fails closed: the edit answers
//! 500 and writes nothing.

use std::collections::HashSet;

use http::StatusCode;
use tablo::{
    Ability, FieldErrors, FormValues, RecordForm, Resource, ResourceDef, ResourcePage, Schema,
};
use topcoat::context::Cx;

use super::{Item, PriorityForm, PriorityFormField, item_db, item_table, reload, seed_item};
use crate::common::{panel_router, post_fields};

/// An unkeyable record rule fails closed.
#[tokio::test]
async fn an_unkeyable_record_rule_fails_closed() {
    /// [`PriorityForm`] with no keys: nothing can render its errors.
    struct Keyless(PriorityForm);

    impl tablo::FormFields for Keyless {
        type Field = PriorityFormField;
        type Keys = <PriorityForm as tablo::FormFields>::Keys;

        fn keys() -> Self::Keys {
            <PriorityForm as tablo::FormFields>::keys()
        }
    }

    impl RecordForm for Keyless {
        type Model = Item;

        fn fields() -> Vec<tablo::FormField<PriorityFormField>> {
            Vec::new()
        }

        fn control(_field: PriorityFormField) -> Schema<Self> {
            Schema::default()
        }

        fn hydrate(cx: &Cx, record: &Item) -> FormValues {
            PriorityForm::hydrate(cx, record)
        }

        fn parse(_cx: &Cx, _values: &FormValues) -> Result<Self, Vec<tablo::FieldError>> {
            Ok(Keyless(PriorityForm { priority: 1 }))
        }

        fn into_create(self) -> <Item as toasty::schema::Model>::Create {
            self.0.into_create()
        }

        fn into_update<'a>(
            self,
            record: &'a mut Item,
            named: &HashSet<PriorityFormField>,
        ) -> Option<<Item as toasty::schema::Model>::Update<'a>> {
            self.0.into_update(record, named)
        }

        fn exec_update<'a>(
            update: <Item as toasty::schema::Model>::Update<'a>,
            ex: &'a mut dyn toasty::Executor,
        ) -> impl std::future::Future<Output = toasty::Result<()>> + Send + 'a {
            PriorityForm::exec_update(update, ex)
        }
    }

    struct Refusing;

    impl Resource for Refusing {
        type Model = Item;
        type Form = Keyless;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("items")
                .policy_fn(|_cx, ability| matches!(ability, Ability::View(_) | Ability::Update(_)))
                .table(item_table())
                .without_page(ResourcePage::Create)
        }

        fn validate_record(_cx: &Cx, _form: &Keyless) -> FieldErrors<Keyless> {
            let mut errors = FieldErrors::<Keyless>::new();
            errors.add(|f| f.priority, "never");
            errors
        }
    }

    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = panel_router::<Refusing>(db.clone());
    let response = post_fields(&router, &format!("/admin/items/{}/edit", item.id), &[]).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(reload(&db, item.id).await.priority, 7, "nothing is written");
}

/// An unkeyable parse failure fails closed.
#[tokio::test]
async fn an_unkeyable_parse_failure_fails_closed() {
    /// [`PriorityForm`] with no keys, refusing on a key nothing renders.
    struct Unkeyable(PriorityForm);

    impl tablo::FormFields for Unkeyable {
        type Field = PriorityFormField;
        type Keys = <PriorityForm as tablo::FormFields>::Keys;

        fn keys() -> Self::Keys {
            <PriorityForm as tablo::FormFields>::keys()
        }
    }

    impl RecordForm for Unkeyable {
        type Model = Item;

        fn fields() -> Vec<tablo::FormField<PriorityFormField>> {
            Vec::new()
        }

        fn control(_field: PriorityFormField) -> Schema<Self> {
            Schema::default()
        }

        fn hydrate(cx: &Cx, record: &Item) -> FormValues {
            PriorityForm::hydrate(cx, record)
        }

        fn parse(_cx: &Cx, _values: &FormValues) -> Result<Self, Vec<tablo::FieldError>> {
            Err(vec![tablo::FieldError::invalid("never", "never renders")])
        }

        fn into_create(self) -> <Item as toasty::schema::Model>::Create {
            self.0.into_create()
        }

        fn into_update<'a>(
            self,
            record: &'a mut Item,
            named: &HashSet<PriorityFormField>,
        ) -> Option<<Item as toasty::schema::Model>::Update<'a>> {
            self.0.into_update(record, named)
        }

        fn exec_update<'a>(
            update: <Item as toasty::schema::Model>::Update<'a>,
            ex: &'a mut dyn toasty::Executor,
        ) -> impl std::future::Future<Output = toasty::Result<()>> + Send + 'a {
            PriorityForm::exec_update(update, ex)
        }
    }

    struct Parseless;

    impl Resource for Parseless {
        type Model = Item;
        type Form = Unkeyable;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("items")
                .policy_fn(|_cx, ability| matches!(ability, Ability::View(_) | Ability::Update(_)))
                .table(item_table())
                .without_page(ResourcePage::Create)
        }
    }

    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = panel_router::<Parseless>(db.clone());
    let response = post_fields(&router, &format!("/admin/items/{}/edit", item.id), &[]).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(reload(&db, item.id).await.priority, 7, "nothing is written");
}
