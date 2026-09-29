//! Bulk delete POST — ids via the `ids` form field, bounded, confirmed,
//! all-or-nothing in one framework transaction.

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
    fetch::composite_pk_error,
};
use crate::{
    db::db,
    notification::{Notification, notify_write_failure, set_notification},
    resource::{Committed, Resource},
};

/// Failure-toast wording for the delete handlers.
const WRITE_BULK_DELETE: &str = "delete the selected rows";

/// Bulk delete POST — ids via `ids` form field (comma-separated).
///
/// Identity is the typed PK fetch alone: the display key
/// is never re-matched, so non-canonical keys (uppercase UUID,
/// email key) cannot 404 a batch whose rows exist. Bounded by
/// `MAX_BULK_IDS` so the `IN` list cannot be amplified into a DoS.
/// Fetch, policy checks, and deletes share one framework transaction
/// a mid-loop failure deletes zero rows.
pub(crate) fn resource_bulk_delete<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::<_, BoxView<'_>>::new(
        async move {
            gate::<R>(cx)?;
            // The whole-resource half of the policy, before the body is read:
            // a resource that allows no delete renders no delete chrome.
            if !R::can_delete_any(cx) {
                return Err(forbidden().into());
            }
            // Delete/bulk-delete carry no file parts: only the values half is read.
            let values = parse_form_body(cx, body).await?.values;
            crate::csrf::verify(cx, &values)?;
            // Confirmation marker, mirroring the row delete: the bulk
            // bar's dialog carries `confirm=1`, so a POST without it did not
            // come from the confirming control. Checked after CSRF verification
            // and before any DB work — a forged POST answers 400
            // without touching a connection.
            if !values.get("confirm").is_some_and(|v| truthy(v)) {
                return Err(topcoat::router::error::bad_request(
                    "bulk delete requires confirmation",
                )
                .into());
            }
            let ids_raw = values.get("ids").cloned().unwrap_or_default();
            let ids = parse_bulk_ids(&ids_raw, MAX_BULK_IDS);
            if ids.is_empty() {
                // No ids is a validation miss, not a raw 400 page:
                // the bulk bar cannot submit without a selection, so only a
                // crafted (or stale) POST gets here — answer like any other
                // mutation, with the list and the reason.
                set_notification(cx, Notification::error("Select at least one row to delete"));
                return Err(see_other(list_url(cx, &R::slug())).into());
            }
            if ids.len() > MAX_BULK_IDS {
                return Err(topcoat::router::error::bad_request(format!(
                    "too many ids (max {MAX_BULK_IDS})"
                ))
                .into());
            }
            // Fetch only the requested rows through the tenancy-scoped seam:
            // one `pk IN (…)` query replaces the #75 item-1
            // fetch-everything-then-match loop. A malformed id cannot exist and
            // maps to 404; a missing/wrong-tenant id makes the fetch come back
            // short and 404s as well.
            let keys: Vec<&str> = ids.iter().map(String::as_str).collect();
            let Some(pk_filter) = crate::schema::pk_in_expr::<R::Model>(&keys) else {
                if let Some(error) = composite_pk_error::<R>() {
                    return Err(error);
                }
                return Err(topcoat::router::error::not_found().into());
            };
            let mut db = db(cx);
            let mut tx = db.transaction().await.map_err(crate::db::unavailable)?;
            // The batch fetch reads only the records' own columns:
            // the policy predicates and the write below never touch a relation.
            let rows = crate::resource::scoped_query_with::<R>(
                cx,
                &crate::resource::IncludeNeeds::default(),
            )?
            .filter(pk_filter)
            .exec(&mut tx)
            .await
            .map_err(crate::db::unavailable)?;
            if rows.len() != ids.len() {
                return Err(topcoat::router::error::not_found().into());
            }
            for rec in &rows {
                // Edit contract on every row: viewing precedes
                // deleting, same as the edit GET/POST pair.
                if !R::can_view(cx, rec) {
                    return Err(forbidden().into());
                }
                if !R::can_delete(cx, rec) {
                    return Err(forbidden().into());
                }
            }
            // All checks passed — perform bulk delete inside the tx, then
            // commit once. Any error drops `tx` uncommitted: zero rows
            // deleted, never half-applied.
            // The hook names the whole batch: a bulk delete is one
            // write, so it is one `after_commit` call, not one per row. Keeping
            // a copy is the price of that (bounded by `MAX_BULK_IDS`); handing
            // the rows over by reference would mean changing two record-fn
            // signatures for a copy this small.
            let committed_rows = rows.clone();
            if let Err(error) = R::bulk_delete_records(cx, rows, &mut tx).await {
                notify_write_failure(cx, WRITE_BULK_DELETE);
                // Same seam as the row delete.
                return Err(crate::db::hook_failure(error));
            }
            if let Err(error) = tx.commit().await {
                notify_write_failure(cx, WRITE_BULK_DELETE);
                return Err(crate::db::unavailable(error));
            }
            crate::resource::run_after_commit::<R>(cx, Committed::deleted(committed_rows)).await;
            set_notification(cx, Notification::success("Bulk deleted"));
            Err(see_other(list_url(cx, &R::slug())).into())
        },
    )))
}

/// Max ids accepted by bulk delete: bounds the `IN` list.
const MAX_BULK_IDS: usize = 400;

/// Parse + dedupe bulk `ids` while preserving order, so a repeated id can't
/// make the fetched-rows count check misfire.
///
/// `max` bounds the parse itself, not just the final list: a 10 MiB
/// body of distinct ids stops at `max + 1` entries (which the handler then
/// rejects with 400) instead of allocating millions of strings while the
/// `MAX_BULK_IDS` check waits for the parse to finish. Deduping uses a set, so
/// the scan stays linear in the number of ids.
///
/// Known limit: the split happens after url-decoding, so a
/// `String`-PK id containing a literal comma (`%2C`) splits into phantom
/// ids and the batch 404s. Comma-bearing string PKs need a different
/// transport (future work); all other PK types are comma-free.
fn parse_bulk_ids(raw: &str, max: usize) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for s in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if seen.insert(s) {
            ids.push(s.to_string());
            if ids.len() > max {
                break;
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use toasty::Db;
    use topcoat::Result;

    use super::*;
    use crate::panel::test_support::{Dummy, dummy_table, panel_for};

    #[test]
    fn parse_bulk_ids_dedupes_and_trims() {
        assert!(parse_bulk_ids("", MAX_BULK_IDS).is_empty());
        assert_eq!(
            parse_bulk_ids("a, b ,a,, c", MAX_BULK_IDS),
            vec!["a", "b", "c"]
        );
        // The cap bounds the parse too: stop at max + 1 for the handler's 400.
        assert_eq!(parse_bulk_ids("a,b,c,d,e", 3).len(), 4);
    }

    #[tokio::test]
    async fn bulk_delete_caps_ids_and_ignores_display_key() {
        use crate::resource::Resource;

        struct UpperKeyResource;
        impl Resource for UpperKeyResource {
            type Model = Dummy;
            type Form = crate::NoForm<Self::Model>;
            fn slug() -> String {
                "dummies".to_string()
            }
            fn can_delete_any(_cx: &Cx) -> bool {
                true
            }
            fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
                true
            }
            fn table(_cx: &Cx) -> crate::resource::Table<Dummy> {
                // Non-canonical display key: bulk must still resolve
                // via the typed PK fetch alone. The record key stays canonical
                // The renderer emits it for bulk values, so the
                // display/URL split is exercised, not bypassed.
                crate::resource::Table::new_split(
                    |d: &Dummy| d.id.to_string().to_uppercase(),
                    |d: &Dummy| d.id.to_string(),
                    crate::resource::TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| {
                        d.name.clone()
                    }),
                )
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
        let router = panel_for::<UpperKeyResource>(db)
            .build()
            .expect("panel builds");
        // Canonical lowercase id succeeds despite an uppercase display key.
        let token = uuid::Uuid::new_v4().to_string();
        let ok = router
            .handle(
                http::Request::builder()
                    .uri("/admin/dummies/bulk-delete")
                    .method(http::Method::POST)
                    .header(
                        http::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .header(
                        http::header::COOKIE,
                        format!("{}={token}", crate::csrf::COOKIE_NAME),
                    )
                    .body(Body::from(format!(
                        "ids={}&confirm=1&csrf_token={token}",
                        row.id
                    )))
                    .unwrap(),
            )
            .await;
        assert!(
            ok.status().is_redirection(),
            "PK-authenticated bulk must not 404 on display-key mismatch, got {}",
            ok.status()
        );
        // Over-cap batch is a clear 400 before any DB work.
        let big = (0..(MAX_BULK_IDS + 1))
            .map(|i| format!("00000000-0000-0000-0000-{:012}", i))
            .collect::<Vec<_>>()
            .join(",");
        let capped = router
            .handle(
                http::Request::builder()
                    .uri("/admin/dummies/bulk-delete")
                    .method(http::Method::POST)
                    .header(
                        http::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .header(
                        http::header::COOKIE,
                        format!("{}={token}", crate::csrf::COOKIE_NAME),
                    )
                    .body(Body::from(format!(
                        "ids={big}&confirm=1&csrf_token={token}"
                    )))
                    .unwrap(),
            )
            .await;
        assert_eq!(capped.status(), http::StatusCode::BAD_REQUEST);
        // Missing token is 403.
        let no_token = router
            .handle(
                http::Request::builder()
                    .uri("/admin/dummies/bulk-delete")
                    .method(http::Method::POST)
                    .header(
                        http::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .body(Body::from(format!("ids={}", row.id)))
                    .unwrap(),
            )
            .await;
        assert_eq!(no_token.status(), http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn bulk_delete_mid_loop_failure_deletes_zero_rows() {
        // GH #84 acceptance: fetch, policy checks, and deletes share one
        // framework transaction — an impl that fails halfway rolls everything
        // back instead of half-applying.

        use crate::resource::Resource;

        struct FlakyBulkResource;
        impl Resource for FlakyBulkResource {
            type Model = Dummy;
            type Form = crate::NoForm<Self::Model>;
            fn slug() -> String {
                "dummies".to_string()
            }
            fn can_delete_any(_cx: &Cx) -> bool {
                true
            }
            fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
                true
            }
            fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
                dummy_table(cx)
            }
            async fn bulk_delete_records(
                _cx: &Cx,
                records: Vec<Dummy>,
                ex: &mut dyn toasty::Executor,
            ) -> Result<()> {
                // Delete the first row, then blow up: without the
                // framework tx the first delete would stick.
                let first = records.into_iter().next().unwrap();
                Dummy::filter(Dummy::fields().id().eq(first.id))
                    .delete()
                    .exec(&mut *ex)
                    .await
                    .map_err(topcoat::Error::from)?;
                Err(std::io::Error::other("boom").into())
            }
        }

        let mut db = Db::builder()
            .models(toasty::models!(Dummy))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        for name in ["one", "two"] {
            toasty::create!(Dummy {
                name: name.to_string(),
            })
            .exec(&mut db)
            .await
            .unwrap();
        }
        let mut db_ids = db.clone();
        let rows = Dummy::all().exec(&mut db_ids).await.unwrap();
        assert_eq!(rows.len(), 2);
        let ids = rows
            .iter()
            .map(|r| r.id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let router = panel_for::<FlakyBulkResource>(db.clone())
            .build()
            .expect("panel builds");
        let token = uuid::Uuid::new_v4().to_string();
        let resp = router
            .handle(
                http::Request::builder()
                    .uri("/admin/dummies/bulk-delete")
                    .method(http::Method::POST)
                    .header(
                        http::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .header(
                        http::header::COOKIE,
                        format!("{}={token}", crate::csrf::COOKIE_NAME),
                    )
                    .body(Body::from(format!(
                        "ids={ids}&confirm=1&csrf_token={token}"
                    )))
                    .unwrap(),
            )
            .await;
        assert!(
            resp.status().is_server_error(),
            "mid-loop failure must error, got {}",
            resp.status()
        );
        let rows = Dummy::all().exec(&mut db_ids).await.unwrap();
        assert_eq!(
            rows.len(),
            2,
            "rollback must leave zero rows deleted, got {}",
            2 - rows.len()
        );
    }
}
