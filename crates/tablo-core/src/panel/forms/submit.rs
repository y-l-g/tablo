//! The create/edit POST pipelines.

use std::collections::{HashMap, HashSet};

use topcoat::{
    Result,
    context::Cx,
    router::{Body, error::forbidden},
    view::{BoxView, HoistView, internal::ThenView},
};

use super::{
    super::{actions::find_by_key_narrowed, gate},
    common::{
        FormParts, drop_client_typed_uploads, redirect_after_write, reject_unknown_form_keys,
        rerender_invalid_form, restore_pending_uploads, strip_transport_keys, truthy,
    },
    decode::parse_form_body,
    unique::check_unique,
};
use crate::{
    db::db,
    notification::notify_write_failure,
    resource::{Committed, Resource},
};

/// Failure-toast wording for the create/update handlers: one place,
/// so the two paths cannot drift.
const WRITE_CREATE: &str = "create the record";
const WRITE_UPDATE: &str = "save the changes";

/// The staged submission both write handlers carry into their transaction: the
/// declared schema, the upload-staged and transport-stripped values, the
/// validation errors so far, the upload paths a re-render keeps, and
/// the stored values the edit path compares against.
struct Submission {
    schema: crate::schema::Schema,
    values: HashMap<String, String>,
    errors: HashMap<String, Vec<String>>,
    carried: HashSet<String>,
    current: HashMap<String, String>,
}

/// Stage a create/edit submission: reject undeclared keys, take file values
/// only from file parts, store the uploads outside the transaction, restore the
/// paths a re-rendered form carried, backfill
/// an untouched file input from `advisory`, strip the transport keys
/// and validate — required and unique-free checks first, then the
/// async relationship existence check.
///
/// `advisory` is the edit path's pre-transaction snapshot: it seeds the stored
/// values and the untouched-file backfill. A create passes `None`, so both are
/// empty and the backfill never fires.
async fn prepare_submission<R: Resource>(
    cx: &Cx,
    parts: FormParts,
    advisory: Option<R::Model>,
) -> Result<Submission, topcoat::Error> {
    let schema = R::form(cx);
    reject_unknown_form_keys(&schema, &parts.values)?;
    let FormParts {
        mut values,
        files,
        file_part_names,
    } = parts;
    let current = advisory
        .map(|advisory| R::hydrate_form_values(cx, &advisory))
        .unwrap_or_default();
    // A declared `FileUpload` takes its value only from a file part:
    // a text part or a url-encoded pair under the same name is client-typed,
    // not an upload, and would otherwise reach the record and render as the
    // file's link.
    drop_client_typed_uploads(&schema, &file_part_names, &mut values);
    // Uploaded bytes become stored paths before validation, and outside the
    // transaction: an upload is a side effect in another system, so a
    // rolled-back transaction must not have to undo it, and a store that
    // rejects the file must be able to answer inline.
    let (upload_errors, mut carried) =
        crate::upload::store_uploads(cx, &schema, &files, &mut values).await;
    // A form re-rendered after a failed submit carries the path its store just
    // answered; the uploader must still hold it, and it wins over the record's
    // stored value below. Run before the backfill: a restored field
    // is non-empty, so the backfill leaves it alone.
    carried.extend(restore_pending_uploads(cx, &schema, &mut values).await);
    // Untouched file inputs preserve the stored path: the edit form
    // renders an empty file input (browsers never pre-fill it), so an empty
    // submit means "keep", not "clear" — without this the required check
    // rejects untouched edits and optional uploads get blanked. An explicit
    // `clear_<field>=1` opts back into clearing; a chosen file still wins over
    // it, because a replacement is not a removal.
    for name in schema.file_uploads().keys() {
        let cleared = values
            .get(&format!("clear_{name}"))
            .is_some_and(|v| truthy(v));
        let empty = values
            .get(name)
            .map(|v| v.trim().is_empty())
            .unwrap_or(true);
        if !cleared && empty && current.get(name).is_some_and(|v| !v.trim().is_empty()) {
            values.insert(name.clone(), current[name].clone());
        }
    }
    // Transport keys never reach the record fn; see `strip_transport_keys`.
    strip_transport_keys(&schema, &mut values);
    let mut errors = schema.validate_async(cx, &values).await;
    // A rejected upload owns its field's error slot: "required" would restate
    // the symptom (nothing was stored) and hide the reason.
    errors.extend(upload_errors);
    // App-level rules render inline like the Schema's own: a record fn error
    // is a 500, so a range or cross-field rule lives here, never there.
    for (field, field_errors) in R::validate(cx, &values) {
        errors.entry(field).or_default().extend(field_errors);
    }
    Ok(Submission {
        schema,
        values,
        errors,
        carried,
        current,
    })
}

/// The shared write tail: commit the transaction,
/// run the after-commit hook on the row the record fn wrote, and redirect with
/// the success flash; a failed write or commit maps to the caller's operation
/// toast and the opaque error.
///
/// `committed` names the mutation, `note` the success flash, and `failure` the
/// toast.
async fn commit_write<'a, R: Resource>(
    cx: &'a Cx,
    tx: toasty::Transaction<'_>,
    written: Result<R::Model, topcoat::Error>,
    committed: impl FnOnce(R::Model) -> Committed<R::Model>,
    note: &'static str,
    failure: &'static str,
) -> Result<BoxView<'a>, topcoat::Error> {
    match written {
        Ok(record) => match tx.commit().await {
            Ok(()) => {
                // Post-commit, so the effect cannot survive a rollback
                // the tx is gone, so the hook may open its own
                // handle.
                crate::resource::run_after_commit::<R>(cx, committed(record)).await;
                Err(redirect_after_write::<R>(cx, note))
            }
            Err(error) => {
                notify_write_failure(cx, failure);
                Err(crate::db::unavailable(error))
            }
        },
        // A unique violation that slipped past the app-side check (a
        // concurrent write) surfaces as an error, not a string-matched inline
        // message: Toasty exposes no unique-violation predicate (upstream gap
        // #117), so the failure cannot be classified here. It is still not
        // echoed raw: the driver's text goes to the log through the
        // opaque mapping, and an app-authored hook error keeps its own.
        Err(error) => {
            notify_write_failure(cx, failure);
            Err(crate::db::hook_failure(error))
        }
    }
}

pub(crate) fn resource_create_post<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::new(async move {
        gate::<R>(cx)?;
        if !R::can_create(cx) {
            return Err(forbidden().into());
        }
        let parts = parse_form_body(cx, body).await?;
        crate::csrf::verify(cx, &parts.values)?;
        // A create has no stored value to keep, so it stages no advisory
        // snapshot: a rejected file leaves its field empty beside the reason.
        let Submission {
            schema,
            mut values,
            mut errors,
            carried,
            ..
        } = prepare_submission::<R>(cx, parts, None).await?;
        // Framework-owned transaction, opened only after validation so that
        // `validate_async` loaders still run before it opens (see `crate::db`
        // pool discipline). The unique check and the write observe one snapshot
        // and commit atomically; dropping `tx` without commit rolls back.
        let mut db = db(cx);
        let mut tx = db.transaction().await.map_err(crate::db::unavailable)?;
        // App-side unique check over every `unique()`-marked input — the only
        // error layer until toasty exposes a unique-violation predicate
        // (upstream gap #117; never string-match driver error messages).
        for (name, errs) in
            check_unique::<R>(cx, &schema, &values, &HashMap::new(), &mut tx).await?
        {
            errors.entry(name).or_default().extend(errs);
        }
        if !errors.is_empty() {
            return rerender_invalid_form::<R>(
                cx,
                tx,
                format!("Create {}", R::navigation_label()),
                "Create",
                &values,
                &errors,
                &carried,
                None,
            )
            .await;
        }
        // Typed fields write their own spelling, not the browser's.
        schema.normalize_values(&mut values);
        // Attempt creation via Resource hook, inside the tx. The row it
        // returns is what `after_commit` names for this write — the
        // key is the database's to generate, so the row is the only place the
        // framework can learn it.
        let written = R::create_record(cx, values.clone(), &mut tx).await;
        commit_write::<R>(cx, tx, written, Committed::created, "Created", WRITE_CREATE).await
    })))
}

/// Edit page POST — validates, checks `can_view` + `can_update`, mutates via Update projection.
///
/// Requires both `can_view` and `can_update` (matching GET, deny-by-default):
/// a view-denied but writable record must not be mutable by direct POST.
pub(crate) fn resource_edit_post<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::new(async move {
        gate::<R>(cx)?;
        let parts = parse_form_body(cx, body).await?;
        crate::csrf::verify(cx, &parts.values)?;
        let id = topcoat::router::path_param_segment(cx, "id").to_string();
        // Advisory load on a pooled handle: feeds hydration and the
        // pre-validation file backfill below. The body is already parsed and
        // CSRF-verified, so the load never runs for a forged POST.
        // The authoritative load + policy check happens inside the framework
        // transaction; validation's `validate_async` loaders run before it
        // opens (see `crate::db` pool discipline).
        let mut db0 = db(cx);
        let advisory = find_by_key_narrowed::<R>(cx, &id, &mut db0).await?;
        if !R::can_view(cx, &advisory) {
            return Err(forbidden().into());
        }
        if !R::can_update(cx, &advisory) {
            return Err(forbidden().into());
        }
        let Submission {
            schema,
            mut values,
            mut errors,
            carried,
            current,
        } = prepare_submission::<R>(cx, parts, Some(advisory)).await?;
        // Authoritative load inside the framework transaction (#86):
        // policy is checked on this snapshot and the same record flows into
        // the write — never a silent re-load outside the checked snapshot.
        let mut db = db(cx);
        let mut tx = db.transaction().await.map_err(crate::db::unavailable)?;
        let record = find_by_key_narrowed::<R>(cx, &id, &mut tx).await?;
        if !R::can_view(cx, &record) {
            return Err(forbidden().into());
        }
        if !R::can_update(cx, &record) {
            return Err(forbidden().into());
        }
        for (name, errs) in check_unique::<R>(cx, &schema, &values, &current, &mut tx).await? {
            errors.entry(name).or_default().extend(errs);
        }
        if !errors.is_empty() {
            let public = R::public_url(cx, &record);
            return rerender_invalid_form::<R>(
                cx,
                tx,
                format!("Edit {}", R::navigation_label()),
                "Save",
                &values,
                &errors,
                &carried,
                public,
            )
            .await;
        }
        // Typed fields write their own spelling, not the browser's.
        schema.normalize_values(&mut values);
        let written = R::update_record(cx, record, values.clone(), &mut tx).await;
        commit_write::<R>(cx, tx, written, Committed::updated, "Updated", WRITE_UPDATE).await
    })))
}
#[cfg(test)]
mod tests {
    use toasty::Db;
    use topcoat::view::ViewExt;

    use super::*;
    use crate::{
        Panel,
        panel::test_support::{Dummy, dummy_table, panel_for, response_html},
        schema::{FileUpload, Schema, TextInput},
    };

    #[tokio::test]
    async fn edit_post_requires_can_view_as_well_as_can_update() {
        use std::collections::HashMap;

        use crate::resource::Resource;

        struct ViewDeniedResource;
        impl Resource for ViewDeniedResource {
            type Model = Dummy;
            fn slug() -> String {
                "dummies".to_string()
            }
            fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
                false
            }
            fn can_update(_cx: &Cx, _record: &Dummy) -> bool {
                true
            }
            async fn update_record(
                _cx: &Cx,
                record: Dummy,
                _values: HashMap<String, String>,
                _ex: &mut dyn toasty::Executor,
            ) -> Result<Dummy> {
                // Nothing to write in this test; a record fn returns the row it
                // wrote, so it hands back the one it was given.
                Ok(record)
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
        let url = format!("/admin/dummies/{}/edit", row.id);
        // GET already required both; POST must match.
        let get = router
            .handle(
                http::Request::builder()
                    .uri(&url)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(get.status(), http::StatusCode::FORBIDDEN);
        // Valid CSRF token still 403 on policy (not on CSRF).
        let token = uuid::Uuid::new_v4().to_string();
        let post = router
            .handle(
                http::Request::builder()
                    .uri(&url)
                    .method(http::Method::POST)
                    .header(
                        http::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .header(
                        http::header::COOKIE,
                        format!("{}={token}", crate::csrf::COOKIE_NAME),
                    )
                    .body(Body::from(format!("name=Ada&csrf_token={token}")))
                    .unwrap(),
            )
            .await;
        assert_eq!(
            post.status(),
            http::StatusCode::FORBIDDEN,
            "view-denied edit POST must not mutate"
        );
        // Missing token is 403 even before policy.
        let no_token = router
            .handle(
                http::Request::builder()
                    .uri(&url)
                    .method(http::Method::POST)
                    .header(
                        http::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .body(Body::from("name=Ada"))
                    .unwrap(),
            )
            .await;
        assert_eq!(no_token.status(), http::StatusCode::FORBIDDEN);
    }

    /// Record fns never see framework transport keys: the create POST carries
    /// `csrf_token` (and, for file schemas, `clear_<field>` and the
    /// `keep_<field>` candidate a re-rendered form adds), which the framework
    /// strips before `create_record`.
    #[tokio::test]
    async fn create_record_receives_no_transport_keys() {
        use std::sync::Mutex;

        use crate::schema::{FileUpload, Schema, TextInput};

        #[derive(Debug, toasty::Model, Clone)]
        struct Doc {
            #[key]
            #[auto]
            id: uuid::Uuid,
            path: String,
            title: String,
        }

        static RECEIVED: Mutex<Vec<Vec<String>>> = Mutex::new(Vec::new());
        struct CapturingResource;
        impl crate::resource::Resource for CapturingResource {
            type Model = Doc;
            fn slug() -> String {
                "docs".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_create(_cx: &Cx) -> bool {
                true
            }
            fn table(cx: &Cx) -> crate::resource::Table<Doc> {
                crate::resource::Table::r#for(cx)
                    .id(|d: &Doc| d.id.to_string())
                    .columns(crate::resource::TextColumn::r#for(
                        Doc::fields().title(),
                        |d: &Doc| d.title.clone(),
                    ))
            }
            fn form(_cx: &Cx) -> Schema {
                Schema::new((
                    TextInput::r#for(Doc::fields().title()),
                    FileUpload::r#for(Doc::fields().path()),
                ))
            }
            async fn create_record(
                _cx: &Cx,
                values: HashMap<String, String>,
                ex: &mut dyn toasty::Executor,
            ) -> topcoat::Result<Doc> {
                let mut keys = values.keys().cloned().collect::<Vec<_>>();
                keys.sort();
                RECEIVED.lock().unwrap().push(keys);
                // A create returns the row it wrote.
                toasty::create!(Doc {
                    path: values.get("path").cloned().unwrap_or_default(),
                    title: values.get("title").cloned().unwrap_or_default(),
                })
                .exec(&mut *ex)
                .await
                .map_err(|error| -> topcoat::Error { error.into() })
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Doc))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let router = panel_for::<CapturingResource>(db)
            .build()
            .expect("panel builds");
        let csrf = uuid::Uuid::new_v4().to_string();
        // `path` is a `FileUpload`, so it arrives as a file part;
        // `clear_path`, the client-typed `keep_path` candidate and
        // `csrf_token` are the transport keys under test.
        let boundary = "----TransportBoundary";
        let body = format!(
            "--{b}\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nx\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"path\"; filename=\"a.bin\"\r\nContent-Type: application/octet-stream\r\n\r\nBYTES\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"clear_path\"\r\n\r\n1\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"keep_path\"\r\n\r\njavascript:alert(1)\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"csrf_token\"\r\n\r\n{csrf}\r\n\
             --{b}--\r\n",
            b = boundary
        );
        let resp = router
            .handle(
                http::Request::builder()
                    .method(http::Method::POST)
                    .uri("/admin/docs/create")
                    .header(
                        http::header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .header(
                        http::header::COOKIE,
                        format!("{}={csrf}", crate::csrf::COOKIE_NAME),
                    )
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await;
        assert!(
            resp.status().is_redirection(),
            "create succeeds, got {} {}",
            resp.status(),
            String::from_utf8_lossy(
                &http_body_util::BodyExt::collect(resp.into_body())
                    .await
                    .unwrap()
                    .to_bytes()
            )
        );
        let received = RECEIVED.lock().unwrap();
        let keys = received.last().expect("create_record ran");
        assert!(
            !keys.contains(&"csrf_token".to_string())
                && !keys.contains(&"clear_path".to_string())
                && !keys.contains(&"keep_path".to_string()),
            "transport keys must be stripped before the record fn, got {keys:?}"
        );
        assert_eq!(keys.len(), 2, "declared fields only, got {keys:?}");
    }

    /// GH #229, create half: a write that fails at the driver surfaces the
    /// opaque mapping, never the driver's own text — the property
    /// `db.rs` pins for `unavailable`, one layer up and through the real
    /// create handler.
    #[tokio::test]
    async fn a_driver_create_failure_does_not_echo_driver_text() {
        use topcoat::{context::CxTestBuilder, cookie::CookieJarCell};

        use crate::{
            resource::Resource,
            schema::{Schema, TextInput},
        };

        struct WritingResource;
        impl Resource for WritingResource {
            type Model = Dummy;
            fn slug() -> String {
                "dummies".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_create(_cx: &Cx) -> bool {
                true
            }
            fn form(_cx: &Cx) -> Schema {
                Schema::new(TextInput::r#for(Dummy::fields().name()))
            }
            async fn create_record(
                _cx: &Cx,
                values: HashMap<String, String>,
                ex: &mut dyn toasty::Executor,
            ) -> Result<Dummy> {
                // The write the hook performs is the one that fails.
                toasty::create!(Dummy {
                    name: values.get("name").cloned().unwrap_or_default(),
                })
                .exec(&mut *ex)
                .await
                .map_err(Into::into)
            }
        }

        // Schema never pushed: the INSERT cannot run, so the failure is the
        // driver's own (the `unique_check_propagates_probe_errors` setup).
        let db = Db::builder()
            .models(toasty::models!(Dummy))
            .connect("sqlite::memory:")
            .await
            .unwrap();

        // Positive control: the same insert outside the handler really does
        // carry driver text, so the assertions below cannot pass vacuously.
        let mut raw = db.clone();
        let driver = toasty::create!(Dummy {
            name: "Ada".to_string(),
        })
        .exec(&mut raw)
        .await
        .expect_err("the table is missing")
        .to_string();
        drop(raw);
        assert!(
            driver.contains("no such table"),
            "the control must be a driver failure, got {driver:?}"
        );

        let token = uuid::Uuid::new_v4().to_string();
        let parts = http::Request::builder()
            .method(http::Method::POST)
            .uri("/admin/dummies/create")
            .header(
                http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .header(
                http::header::COOKIE,
                format!("{}={token}", crate::csrf::COOKIE_NAME),
            )
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let cx = CxTestBuilder::new()
            .app_context(db)
            .request_context(parts)
            .request_context(CookieJarCell::new())
            .build();

        let error = resource_create_post::<WritingResource>(
            &cx,
            Body::from(format!("name=Ada&csrf_token={token}")),
        )
        .first()
        .await
        .expect_err("the write must fail");

        let rendered = error.to_string();
        assert!(
            rendered.contains("database unavailable"),
            "the opaque message must survive, got {rendered:?}"
        );
        assert!(
            !rendered.contains(&driver) && !rendered.contains("no such table"),
            "driver text must not reach the response: the driver said {driver:?}, the response said {rendered:?}"
        );
    }

    /// GH #229, edit half: the update arm is the same seam as create's, and a
    /// write that fails at the driver must not echo the driver's text there
    /// either. The failing write is a unique violation the app-side check
    /// never saw — the case the arm's own comment names (upstream gap #117).
    ///
    /// The edit handler needs the `{id}` the router captures, so the test
    /// mounts it behind a route of its own and renders the error it returns —
    /// the body is exactly what a page would be handed.
    #[tokio::test]
    async fn a_driver_update_failure_does_not_echo_driver_text() {
        use topcoat::{
            cookie::RouterBuilderCookieExt,
            router::{RouteFn, RouteFuture, Router, response::IntoResponse},
        };

        use crate::{
            resource::Resource,
            schema::{Schema, TextInput},
        };

        // The hook's own write targets this model: its unique column is not
        // one the panel's form probes, so the duplicate is the driver's to
        // refuse.
        #[derive(Debug, toasty::Model, Clone)]
        struct Ghost {
            #[key]
            #[auto]
            id: uuid::Uuid,
            #[unique]
            name: String,
        }

        struct EditingResource;
        impl Resource for EditingResource {
            type Model = Dummy;
            fn slug() -> String {
                "dummies".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
                true
            }
            fn can_update(_cx: &Cx, _record: &Dummy) -> bool {
                true
            }
            fn form(_cx: &Cx) -> Schema {
                Schema::new(TextInput::r#for(Dummy::fields().name()))
            }
            async fn update_record(
                _cx: &Cx,
                record: Dummy,
                _values: HashMap<String, String>,
                ex: &mut dyn toasty::Executor,
            ) -> Result<Dummy> {
                // The write the hook performs is the one that fails: the name
                // is taken, and only the database knows it.
                toasty::create!(Ghost {
                    name: "taken".to_string(),
                })
                .exec(&mut *ex)
                .await?;
                Ok(record)
            }
        }

        /// Runs the edit handler under a route that captures `{id}`, and hands
        /// its error back as the body.
        fn edit_error(cx: &Cx, body: Body) -> RouteFuture<'_> {
            Box::pin(async move {
                let error = resource_edit_post::<EditingResource>(cx, body)
                    .first()
                    .await
                    .expect_err("the write must fail");
                error.to_string().into_response(cx)
            })
        }

        let mut db = Db::builder()
            .models(toasty::models!(Dummy, Ghost))
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
        toasty::create!(Ghost {
            name: "taken".to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();

        // Positive control: the hook's own write really does carry driver
        // text, so the assertions below cannot pass vacuously.
        let mut raw = db.clone();
        let driver = toasty::create!(Ghost {
            name: "taken".to_string(),
        })
        .exec(&mut raw)
        .await
        .expect_err("the name is taken")
        .to_string();
        drop(raw);
        assert!(
            driver.contains("UNIQUE constraint failed"),
            "the control must be a driver failure, got {driver:?}"
        );

        let router = Router::builder()
            .cookies()
            .app_context(db)
            .route(RouteFn::new(
                http::Method::POST,
                "/admin/capture/{id}",
                edit_error,
            ))
            .build();
        let token = uuid::Uuid::new_v4().to_string();
        let response = router
            .handle(
                http::Request::builder()
                    .method(http::Method::POST)
                    .uri(format!("/admin/capture/{}", row.id))
                    .header(
                        http::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .header(
                        http::header::COOKIE,
                        format!("{}={token}", crate::csrf::COOKIE_NAME),
                    )
                    .body(Body::from(format!("name=Ada&csrf_token={token}")))
                    .unwrap(),
            )
            .await;
        let rendered = String::from_utf8_lossy(
            &http_body_util::BodyExt::collect(response.into_body())
                .await
                .unwrap()
                .to_bytes(),
        )
        .to_string();

        assert!(
            rendered.contains("database unavailable"),
            "the opaque message must survive, got {rendered:?}"
        );
        assert!(
            !rendered.contains(&driver) && !rendered.contains("UNIQUE constraint failed"),
            "driver text must not reach the response: the driver said {driver:?}, the response said {rendered:?}"
        );
    }

    /// Post/Redirect/Get (#126): a mutation answers 303, the flash
    /// cookie rides the error response (Topcoat flushes `Set-Cookie` on `Err`,
    /// topcoat#408), and nothing rides the `Location` query. Following the
    /// redirect consumes the cookie, so a reload does not replay the toast.
    #[tokio::test]
    async fn mutation_redirect_carries_the_flash_cookie_instead_of_a_query() {
        use std::collections::HashMap;

        use crate::resource::Resource;

        const COOKIE_NAME: &str = crate::notification::COOKIE_NAME;

        struct NotifyingResource;
        impl Resource for NotifyingResource {
            type Model = Dummy;
            fn slug() -> String {
                "dummies".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_create(_cx: &Cx) -> bool {
                true
            }
            fn form(_cx: &Cx) -> crate::schema::Schema {
                // A real field, optional so the test's csrf-only POST still
                // passes validation — `Schema::empty()` is what GH #138's
                // build check refuses for a resource that allows create.
                crate::schema::Schema::new(
                    crate::schema::TextInput::r#for(Dummy::fields().name()).optional(),
                )
            }
            async fn create_record(
                _cx: &Cx,
                _values: HashMap<String, String>,
                ex: &mut dyn toasty::Executor,
            ) -> Result<Dummy> {
                // The row the write produced is what the handler needs back
                // so a test double writes a real one.
                toasty::create!(Dummy {
                    name: "created".to_string(),
                })
                .exec(&mut *ex)
                .await
                .map_err(|error| -> topcoat::Error { error.into() })
            }
            fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
                crate::resource::Table::r#for(cx)
                    .id(|r: &Dummy| r.id.to_string())
                    .columns(crate::resource::TextColumn::r#for(
                        Dummy::fields().name(),
                        |r: &Dummy| r.name.clone(),
                    ))
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Dummy))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let router = panel_for::<NotifyingResource>(db)
            .build()
            .expect("panel builds");
        let token = uuid::Uuid::new_v4().to_string();
        let resp = router
            .handle(
                http::Request::builder()
                    .uri("/admin/dummies/create")
                    .method(http::Method::POST)
                    .header(
                        http::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .header(
                        http::header::COOKIE,
                        format!("{}={token}", crate::csrf::COOKIE_NAME),
                    )
                    .body(Body::from(format!("csrf_token={token}")))
                    .unwrap(),
            )
            .await;
        assert_eq!(
            resp.status(),
            http::StatusCode::SEE_OTHER,
            "a completed mutation is a 303 Post/Redirect/Get"
        );
        let location = resp
            .headers()
            .get(http::header::LOCATION)
            .expect("the redirect names its target")
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            !location.contains("notification"),
            "the toast must not ride the query, got {location}"
        );
        let set_cookie = resp
            .headers()
            .get_all(http::header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find(|v| v.starts_with(&format!("{COOKIE_NAME}=")))
            .expect("the flash cookie flushes on the Err redirect")
            .to_string();
        assert!(
            set_cookie.contains("success") && set_cookie.contains("Created"),
            "the cookie carries the toast status and title: {set_cookie}"
        );
        assert!(
            set_cookie.contains("Secure") && set_cookie.contains("HttpOnly"),
            "the flushed cookie keeps the __Host- contract: {set_cookie}"
        );
    }

    /// GH #189 acceptance, through the real panel: two submits with an empty
    /// `unique()` field re-render inline and write nothing. Before the fix the
    /// first empty submit *succeeded* — it stored `""` — so the panel had
    /// already broken the promise its own unique index makes, and the second
    /// empty submit met the constraint instead of the form rule: 500 when the
    /// record fn stores the value as submitted, or a misleading "has already
    /// been taken" when it trims first.
    #[tokio::test]
    async fn two_empty_submits_on_a_unique_field_re_render_and_write_nothing() {
        use crate::{
            resource::{Resource, Table, TextColumn},
            schema::{Schema, TextInput},
        };

        #[derive(Debug, toasty::Model, Clone)]
        struct Subscriber {
            #[key]
            #[auto]
            id: uuid::Uuid,
            #[unique]
            email: String,
        }
        struct SubscriberResource;
        impl Resource for SubscriberResource {
            type Model = Subscriber;
            fn slug() -> String {
                "subscribers".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_create(_cx: &Cx) -> bool {
                true
            }
            fn table(cx: &Cx) -> Table<Subscriber> {
                Table::r#for(cx)
                    .id(|s: &Subscriber| s.id.to_string())
                    .columns(TextColumn::r#for(
                        Subscriber::fields().email(),
                        |s: &Subscriber| s.email.clone(),
                    ))
            }
            fn form(_cx: &Cx) -> Schema {
                // `.optional()` lets an empty submit probe instead of failing
                // on presence: uniqueness wins.
                Schema::new(
                    TextInput::r#for(Subscriber::fields().email())
                        .unique()
                        .optional(),
                )
            }
            async fn create_record(
                _cx: &Cx,
                values: HashMap<String, String>,
                ex: &mut dyn toasty::Executor,
            ) -> topcoat::Result<Subscriber> {
                // Writes what the panel would: the record fns trim, and the
                // framework's probe trims too, so the stored `""` is exactly
                // what the next probe looks for.
                toasty::create!(Subscriber {
                    email: values
                        .get("email")
                        .map(|v| v.trim().to_string())
                        .unwrap_or_default(),
                })
                .exec(ex)
                .await
                .map_err(|error| -> topcoat::Error { error.into() })
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Subscriber))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let router = panel_for::<SubscriberResource>(db.clone())
            .build()
            .expect("panel builds");

        let csrf = uuid::Uuid::new_v4().to_string();
        // `+` decodes to a space and an empty pair to `""`: both trim to an
        // empty submit, which the presence rule refuses and which must not
        // reach the database. Neither may write.
        for (attempt, submitted) in ["+", ""].into_iter().enumerate() {
            let attempt = attempt + 1;
            let resp = router
                .handle(
                    http::Request::builder()
                        .method(http::Method::POST)
                        .uri("/admin/subscribers/create")
                        .header(
                            http::header::CONTENT_TYPE,
                            "application/x-www-form-urlencoded",
                        )
                        .header(
                            http::header::COOKIE,
                            format!("{}={csrf}", crate::csrf::COOKIE_NAME),
                        )
                        .body(Body::from(format!("email={submitted}&csrf_token={csrf}")))
                        .unwrap(),
                )
                .await;
            assert_eq!(
                resp.status(),
                http::StatusCode::OK,
                "empty submit {attempt} must re-render, not redirect or fail"
            );
            let body = http_body_util::BodyExt::collect(resp.into_body())
                .await
                .unwrap()
                .to_bytes();
            let html = String::from_utf8_lossy(&body);
            assert!(
                html.contains("Email is required"),
                "empty submit {attempt} must carry the presence error, got {html}"
            );
        }

        let mut db_check = db;
        let stored = Subscriber::all().exec(&mut db_check).await.unwrap();
        assert!(
            stored.is_empty(),
            "two empty submits must write nothing, got {} rows",
            stored.len()
        );
    }

    /// `Uploader::holds` defaults to `false`, so a store that does not
    /// implement it cannot vouch for a carried path — a forged `keep_<field>`
    /// leaves the field empty and the create refuses.
    #[tokio::test]
    async fn a_forged_carry_is_refused_by_the_default_holds() {
        #[derive(Debug, toasty::Model, Clone)]
        struct Doc {
            #[key]
            #[auto]
            id: uuid::Uuid,
            title: String,
            path: String,
        }

        /// A store that implements only `store`: `holds` stays the default.
        struct NoHoldsUploader;

        impl crate::Uploader for NoHoldsUploader {
            async fn store(
                &self,
                _filename: &str,
                _bytes: &[u8],
            ) -> std::result::Result<String, String> {
                Ok("/uploads/stored.bin".to_string())
            }
        }

        struct DocResource;

        impl crate::resource::Resource for DocResource {
            type Model = Doc;

            fn slug() -> String {
                "docs".to_string()
            }

            fn can_view_any(_cx: &Cx) -> bool {
                true
            }

            fn can_create(_cx: &Cx) -> bool {
                true
            }

            fn table(cx: &Cx) -> crate::resource::Table<Doc> {
                crate::resource::Table::r#for(cx)
                    .id(|row: &Doc| row.id.to_string())
                    .columns(crate::resource::TextColumn::r#for(
                        Doc::fields().title(),
                        |row: &Doc| row.title.clone(),
                    ))
            }

            fn form(_cx: &Cx) -> Schema {
                Schema::new((
                    TextInput::r#for(Doc::fields().title()),
                    FileUpload::r#for(Doc::fields().path()),
                ))
            }

            async fn create_record(
                _cx: &Cx,
                values: HashMap<String, String>,
                ex: &mut dyn toasty::Executor,
            ) -> topcoat::Result<Doc> {
                toasty::create!(Doc {
                    title: values.get("title").cloned().unwrap_or_default(),
                    path: values.get("path").cloned().unwrap_or_default(),
                })
                .exec(&mut *ex)
                .await
                .map_err(|error| -> topcoat::Error { error.into() })
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Doc))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let router = Panel::new("admin")
            .app_context(db.clone())
            .uploads(NoHoldsUploader)
            .resource::<DocResource>()
            .auth(crate::Auth::disabled())
            .build()
            .expect("panel builds");

        let csrf = uuid::Uuid::new_v4().to_string();
        // A forged candidate with no file part: nothing stored the path.
        let response = router
            .handle(
                http::Request::builder()
                    .method(http::Method::POST)
                    .uri("/admin/docs/create")
                    .header(
                        http::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .header(
                        http::header::COOKIE,
                        format!("{}={csrf}", crate::csrf::COOKIE_NAME),
                    )
                    .body(Body::from(format!(
                        "title=Doc&keep_path=javascript:alert(1)&csrf_token={csrf}"
                    )))
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), 200, "the forged carry must re-render");
        let html = response_html(response).await;
        assert!(
            html.contains("Path is required"),
            "the forged carry must leave the field empty, got {html}"
        );
        let mut db_q = db.clone();
        assert!(
            Doc::all().exec(&mut db_q).await.unwrap().is_empty(),
            "a forged carry must not create a record"
        );
    }
}
