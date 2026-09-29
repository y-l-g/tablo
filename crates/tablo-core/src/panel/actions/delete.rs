//! Delete action POST — confirmation-marked, policy-checked, run in the
//! framework transaction: the checked record flows into the write.

use topcoat::{
    context::Cx,
    router::{
        Body,
        error::{forbidden, see_other},
    },
    view::{BoxView, HoistView, internal::ThenView},
};

use super::{
    super::{
        forms::{parse_form_body, truthy},
        gate::{gate, list_url},
    },
    fetch::find_by_key_narrowed,
};
use crate::{
    db::db,
    notification::{Notification, notify_write_failure, set_notification},
    resource::{Committed, Resource},
};

/// Failure-toast wording for the delete handlers.
const WRITE_DELETE: &str = "delete the record";

/// Delete action POST — confirmation-marked, policy-checked, and run in the
/// framework transaction: the checked record flows into the write.
///
/// The confirmation is the row's alert dialog on the list page: the
/// Delete link opens `?delete=<key>` and the dialog's form POSTs here with
/// `confirm=1`. Authentication comes before any DB work: the CSRF
/// check and the confirmation marker run first, so a forged POST answers 403
/// without opening a transaction, holding a pooled connection across the body
/// read, or probing record existence (create/bulk-delete ordering).
/// The dialog itself is deliberately fetch-free and policy-blind: it carries
/// no record data and embeds only the caller's own CSRF token, and the
/// policy/tenancy checks run against the loaded record here.
pub(crate) fn resource_delete<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::<_, BoxView<'_>>::new(
        async move {
            gate::<R>(cx)?;
            // Delete/bulk-delete carry no file parts: only the values half is read.
            let values = parse_form_body(cx, body).await?.values;
            crate::csrf::verify(cx, &values)?;
            let confirmed = values.get("confirm").is_some_and(|v| truthy(v));
            if !confirmed {
                // The confirmation UI is the list-page alert dialog:
                // the row link opens `?delete=<key>` and the dialog's form carries
                // `confirm=1`. This route only accepts that confirmed POST, so a
                // missing marker is a malformed client, not a user path.
                return Err(
                    topcoat::router::error::bad_request("delete requires confirmation").into(),
                );
            }
            // Confirmed and authenticated: open the transaction only now (GH
            // #144), fetch through the tenant-scoped query, check Policy against the
            // loaded record, and delete inside the tx — commit makes the checked
            // delete durable, any error rolls it back. Delete takes
            // the edit contract: `can_view` plus
            // `can_delete` — a record that cannot be viewed cannot be deleted
            // by UUID-guessing the route.
            let mut db = db(cx);
            let mut tx = db.transaction().await.map_err(crate::db::unavailable)?;
            let id = topcoat::router::path_param_segment(cx, "id").to_string();
            // The delete path reads only the record's own columns:
            // `can_view`/`can_delete` are Rust predicates over those, and
            // `delete_record` consumes the snapshot.
            let record = find_by_key_narrowed::<R>(cx, &id, &mut tx).await?;
            if !R::can_view(cx, &record) {
                return Err(forbidden().into());
            }
            if !R::can_delete(cx, &record) {
                return Err(forbidden().into());
            }
            // `delete_record` consumes the record, and the hook names what was
            // removed: the pre-delete snapshot, since the row is gone
            // by the time it runs.
            let committed_record = record.clone();
            if let Err(error) = R::delete_record(cx, record, &mut tx).await {
                notify_write_failure(cx, WRITE_DELETE);
                // Same seam as create/update: the driver's text
                // stays in the log, an app-authored hook error keeps its own.
                return Err(crate::db::hook_failure(error));
            }
            if let Err(error) = tx.commit().await {
                notify_write_failure(cx, WRITE_DELETE);
                return Err(crate::db::unavailable(error));
            }
            // Post-commit: the tx is gone, so the hook may open its
            // own handle, and a rollback above never reaches this line.
            crate::resource::run_after_commit::<R>(cx, Committed::deleted(vec![committed_record]))
                .await;
            set_notification(cx, Notification::success("Deleted"));
            Err(see_other(list_url(cx, &R::slug())).into())
        },
    )))
}

#[cfg(test)]
mod tests {
    use toasty::Db;
    use topcoat::Result;

    use super::*;
    use crate::panel::test_support::{Dummy, dummy_table, panel_for};

    #[tokio::test]
    async fn delete_and_bulk_delete_require_can_view() {
        // the edit contract extends to deletes — a record that
        // cannot be viewed cannot be deleted by UUID-guessing the route,
        // even with `can_delete == true`.

        use crate::resource::Resource;

        struct ViewDeniedResource;
        impl Resource for ViewDeniedResource {
            type Model = Dummy;
            type Form = crate::NoForm<Self::Model>;
            fn slug() -> String {
                "dummies".to_string()
            }
            fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
                false
            }
            fn can_delete(_cx: &Cx, _record: &Dummy) -> bool {
                true
            }
            fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
                dummy_table(cx)
            }
        }

        let mut db = Db::builder()
            .models(toasty::models!(Dummy))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let row = toasty::create!(Dummy {
            name: "Ada".to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
        let router = panel_for::<ViewDeniedResource>(db)
            .build()
            .expect("panel builds");
        let token = uuid::Uuid::new_v4().to_string();
        let post = |uri: String, body: String| {
            router.handle(
                http::Request::builder()
                    .uri(uri)
                    .method(http::Method::POST)
                    .header(
                        http::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .header(
                        http::header::COOKIE,
                        format!("{}={token}", crate::csrf::COOKIE_NAME),
                    )
                    .body(Body::from(body))
                    .unwrap(),
            )
        };
        // Single delete: view-denied is 403 despite can_delete == true.
        let single = post(
            format!("/admin/dummies/{}/delete", row.id),
            format!("confirm=1&csrf_token={token}"),
        )
        .await;
        assert_eq!(
            single.status(),
            http::StatusCode::FORBIDDEN,
            "view-denied single delete must 403, got {}",
            single.status()
        );
        // Bulk delete: same rule, per row.
        let bulk = post(
            "/admin/dummies/bulk-delete".to_string(),
            format!("ids={}&confirm=1&csrf_token={token}", row.id),
        )
        .await;
        assert_eq!(
            bulk.status(),
            http::StatusCode::FORBIDDEN,
            "view-denied bulk delete must 403, got {}",
            bulk.status()
        );
    }

    #[tokio::test]
    async fn delete_resolves_record_key_not_display_key() {
        // GH #168 defect 1 round-trip: the display key projects a non-PK value
        // (the name), the record key carries the typed PK. Handlers must 404
        // the display value and accept the record key, for single and bulk.

        use crate::resource::Resource;

        struct NameKeyResource;
        impl Resource for NameKeyResource {
            type Model = Dummy;
            type Form = crate::NoForm<Self::Model>;
            fn slug() -> String {
                "dummies".to_string()
            }
            fn can_delete(_cx: &Cx, _record: &Dummy) -> bool {
                true
            }
            fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
                true
            }
            fn table(_cx: &Cx) -> crate::resource::Table<Dummy> {
                crate::resource::Table::new_split(
                    |d: &Dummy| d.name.clone(),
                    |d: &Dummy| d.id.to_string(),
                    crate::resource::TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| {
                        d.name.clone()
                    }),
                )
            }
            async fn delete_record(
                _cx: &Cx,
                _record: Dummy,
                _ex: &mut dyn toasty::Executor,
            ) -> Result<()> {
                Ok(())
            }
            async fn bulk_delete_records(
                _cx: &Cx,
                _records: Vec<Dummy>,
                _ex: &mut dyn toasty::Executor,
            ) -> Result<()> {
                Ok(())
            }
        }

        let mut db = Db::builder()
            .models(toasty::models!(Dummy))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let row = toasty::create!(Dummy {
            name: "Ada".to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
        let router = panel_for::<NameKeyResource>(db)
            .build()
            .expect("panel builds");
        let token = uuid::Uuid::new_v4().to_string();
        let post = |uri: String, body: String| {
            router.handle(
                http::Request::builder()
                    .uri(uri)
                    .method(http::Method::POST)
                    .header(
                        http::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .header(
                        http::header::COOKIE,
                        format!("{}={token}", crate::csrf::COOKIE_NAME),
                    )
                    .body(Body::from(body))
                    .unwrap(),
            )
        };
        // Single delete with the display value 404s — it is not a PK.
        let display_single = post(
            "/admin/dummies/Ada/delete".to_string(),
            format!("confirm=1&csrf_token={token}"),
        )
        .await;
        assert_eq!(
            display_single.status(),
            http::StatusCode::NOT_FOUND,
            "display key must not resolve, got {}",
            display_single.status()
        );
        // Single delete with the record key succeeds.
        let record_single = post(
            format!("/admin/dummies/{}/delete", row.id),
            format!("confirm=1&csrf_token={token}"),
        )
        .await;
        assert!(
            record_single.status().is_redirection(),
            "record key must delete, got {}",
            record_single.status()
        );
        // Bulk with the display value 404s.
        let display_bulk = post(
            "/admin/dummies/bulk-delete".to_string(),
            format!("ids=Ada&confirm=1&csrf_token={token}"),
        )
        .await;
        assert_eq!(
            display_bulk.status(),
            http::StatusCode::NOT_FOUND,
            "display key must not resolve in bulk, got {}",
            display_bulk.status()
        );
        // Bulk with the record key succeeds.
        let record_bulk = post(
            "/admin/dummies/bulk-delete".to_string(),
            format!("ids={}&confirm=1&csrf_token={token}", row.id),
        )
        .await;
        assert!(
            record_bulk.status().is_redirection(),
            "record key must bulk-delete, got {}",
            record_bulk.status()
        );
    }
}
