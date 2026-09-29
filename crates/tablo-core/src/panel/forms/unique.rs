//! App-side uniqueness probe over `unique()`-marked text inputs.

use std::collections::HashMap;

use topcoat::{Result, context::Cx};

use crate::resource::Resource;

/// App-side uniqueness check over the form's `unique()`-marked text inputs.
///
/// Generic over every marked field. Queries through the tenant-scoped query and
/// returns `field_name → ["<Label> has already been taken"]` per duplicated
/// value. `current` holds the record's own hydrated values on edit: a field
/// whose submitted value normalises to the same stored value belongs to this
/// record and is skipped, so a typed field's re-spelled equivalent is not a
/// duplicate.
///
/// Empty submits are never probed: a `unique()` field is required (see
/// [`crate::schema::TextInput::unique`]), so `validate` has already answered
/// `"<Label> is required"` and this check has nothing left to say.
///
/// The probe binds the leaf's own type: a typed field parses the submission and
/// compares the parsed value, so a value unique as text but not as its declared
/// type is still refused.
///
/// Known limits (upstream gap #117): races with concurrent inserts, and a
/// `unique()` field whose index carries components outside the tenant-scoped
/// query's scope is not checked exactly — a composite index such as
/// `#[unique(tenant_id, email)]` on a tenant-scoped resource is. `unique`
/// exists on `TextInput` only.
pub(super) async fn check_unique<R: Resource>(
    cx: &Cx,
    schema: &crate::schema::Schema,
    values: &HashMap<String, String>,
    current: &HashMap<String, String>,
    ex: &mut dyn toasty::Executor,
) -> Result<HashMap<String, Vec<String>>, topcoat::Error> {
    let mut errors: HashMap<String, Vec<String>> = HashMap::new();
    // Groups the submission leaves out are not checked:
    // `validate` treats an all-empty repeater group and a hidden variant group
    // as untouched through the same classification, so a stored value must not
    // flag a group the user never saw.
    let skip = schema.absent_fields(values);
    for (name, input) in schema.text_inputs() {
        if !input.is_unique() || skip.contains(&name) {
            continue;
        }
        let Some(submitted) = values.get(&name).map(|s| s.trim().to_string()) else {
            continue;
        };
        // Empty values are never probed: a `unique` field is
        // required, so validation has already refused this submit — and `""` is
        // still a value the framework stores (never NULL), so a probe
        // would only rediscover the constraint the form just enforced.
        if submitted.is_empty() {
            continue;
        }
        // Unchanged on edit → this record's own value, not a duplicate. Both
        // sides normalise through the leaf's own rule: a typed
        // field's re-spelled equivalent — `01` for `1`, an upper-case UUID for
        // its lower-case form — is the same value, so the probe is skipped. A
        // text comparison would call it changed, probe this record's own row
        // and refuse the save.
        let unchanged = current.get(&name).is_some_and(|kept| {
            matches!(
                (input.normalize(kept), input.normalize(&submitted)),
                (Ok(kept), Ok(submitted)) if kept == submitted
            )
        });
        if unchanged {
            continue;
        }
        // The leaf's own binding: a typed field parses the
        // submission first, so the probe compares the value the record will
        // store rather than its spelling. A typed submission that does not
        // parse has no value to compare — validation refused it first.
        let Some(filter) = input.eq_filter::<R::Model>(&submitted) else {
            continue;
        };
        // Inside the handler's tx: the check observes the same
        // snapshot as the write that follows. A failing probe fails the
        // submit — swallowing it would write past a check that
        // never ran. The probe runs through the tenant-scoped query and reads
        // only the record's own columns, so it passes an empty include set.
        let rows =
            crate::resource::scoped_query_with::<R>(cx, &crate::resource::IncludeNeeds::default())?
                .filter(filter)
                .limit(1)
                .exec(&mut *ex)
                .await
                .map_err(crate::db::unavailable)?;
        if !rows.is_empty() {
            errors.insert(
                name,
                vec![format!("{} has already been taken", input.label_str())],
            );
        }
    }
    Ok(errors)
}
#[cfg(test)]
mod tests {
    use toasty::Db;
    use topcoat::router::Body;

    use super::*;
    use crate::panel::test_support::{Tagged, TaggedResource, panel_for, response_html};

    #[tokio::test]
    async fn unique_check_flags_duplicates_for_marked_fields() {
        use topcoat::context::CxTestBuilder;

        use crate::schema::{Schema, TextInput};

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
            type Form = crate::NoForm<Self::Model>;

            fn table(_cx: &Cx) -> crate::resource::Table<Subscriber> {
                crate::resource::Table::new(
                    |r: &Subscriber| r.id.to_string(),
                    crate::resource::TextColumn::r#for(
                        Subscriber::fields().email(),
                        |r: &Subscriber| r.email.clone(),
                    ),
                )
            }
        }

        let mut db = Db::builder()
            .models(toasty::models!(Subscriber))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        toasty::create!(Subscriber { email: "a@b.c" })
            .exec(&mut db)
            .await
            .unwrap();
        let cx = CxTestBuilder::new().app_context(db).build();
        let mut ex = crate::db::db(&cx);

        let schema = Schema::new(TextInput::r#for(Subscriber::fields().email()).unique());
        let mut values = HashMap::new();
        values.insert("email".to_string(), "a@b.c".to_string());

        // Create: duplicate → inline error on the field, label-derived.
        let errors =
            check_unique::<SubscriberResource>(&cx, &schema, &values, &HashMap::new(), &mut ex)
                .await
                .unwrap();
        assert_eq!(
            errors.get("email"),
            Some(&vec!["Email has already been taken".to_string()]),
            "duplicate must be flagged, got {errors:?}"
        );

        // Fresh value → no error.
        let mut fresh = HashMap::new();
        fresh.insert("email".to_string(), "other@b.c".to_string());
        let errors =
            check_unique::<SubscriberResource>(&cx, &schema, &fresh, &HashMap::new(), &mut ex)
                .await
                .unwrap();
        assert!(errors.is_empty(), "fresh value must pass, got {errors:?}");

        // Edit: the record's own unchanged value is not a duplicate.
        let mut current = HashMap::new();
        current.insert("email".to_string(), "a@b.c".to_string());
        let errors = check_unique::<SubscriberResource>(&cx, &schema, &values, &current, &mut ex)
            .await
            .unwrap();
        assert!(
            errors.is_empty(),
            "own unchanged value must be skipped, got {errors:?}"
        );

        // Edit: changed to someone else's value → flagged again.
        let mut changed_current = HashMap::new();
        changed_current.insert("email".to_string(), "old@b.c".to_string());
        let errors =
            check_unique::<SubscriberResource>(&cx, &schema, &values, &changed_current, &mut ex)
                .await
                .unwrap();
        assert_eq!(
            errors.get("email"),
            Some(&vec!["Email has already been taken".to_string()]),
            "changed-to-duplicate must be flagged, got {errors:?}"
        );

        // Empty submits are never probed: a `unique` field is
        // required, so validation has already refused the submit — on a field
        // whose `.optional()` was overridden, too, in either call order.
        let mut empty = HashMap::new();
        empty.insert("email".to_string(), "   ".to_string());
        let optional_schema = Schema::new(
            TextInput::r#for(Subscriber::fields().email())
                .optional()
                .unique(),
        );
        let errors = check_unique::<SubscriberResource>(
            &cx,
            &optional_schema,
            &empty,
            &HashMap::new(),
            &mut ex,
        )
        .await
        .unwrap();
        assert!(
            errors.is_empty(),
            "an empty unique submit must not be probed, got {errors:?}"
        );
    }

    /// GH #189, at the layer below the handler: an explicitly `unique()` field
    /// is required even when `.optional()` follows it, validation says so, and
    /// the probe stays out of the empty case. What the two submits *write* is
    /// pinned end to end by
    /// [`two_empty_submits_on_a_unique_field_re_render_and_write_nothing`].
    #[tokio::test]
    async fn unique_field_is_required_however_it_is_marked() {
        use topcoat::context::CxTestBuilder;

        use crate::schema::{Schema, TextInput};

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
            type Form = crate::NoForm<Self::Model>;

            fn table(_cx: &Cx) -> crate::resource::Table<Subscriber> {
                crate::resource::Table::new(
                    |r: &Subscriber| r.id.to_string(),
                    crate::resource::TextColumn::r#for(
                        Subscriber::fields().email(),
                        |r: &Subscriber| r.email.clone(),
                    ),
                )
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Subscriber))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let cx = CxTestBuilder::new().app_context(db.clone()).build();
        let mut ex = crate::db::db(&cx);

        // Declared `.optional()` and still required: uniqueness implies
        // presence, so the declaration cannot promise an empty value the index
        // refuses to hold twice.
        let schema = Schema::new(
            TextInput::r#for(Subscriber::fields().email())
                .unique()
                .optional(),
        );
        let mut first = HashMap::new();
        first.insert("email".to_string(), "   ".to_string());
        assert_eq!(
            schema.validate(&first).get("email"),
            Some(&vec!["Email is required".to_string()]),
            "an empty unique field must fail validation as required"
        );

        // Validation owns the empty case, so the probe adds nothing and no
        // query runs — this is what keeps the second empty submit off the
        // unique index.
        let errors =
            check_unique::<SubscriberResource>(&cx, &schema, &first, &HashMap::new(), &mut ex)
                .await
                .unwrap();
        assert!(
            errors.is_empty(),
            "an empty unique submit must not be probed, got {errors:?}"
        );

        // The submit never reaches the write, so the stored table stays empty
        // and the second empty submit cannot collide with the first.
        let mut db_check = db;
        let stored = Subscriber::all().exec(&mut db_check).await.unwrap();
        assert!(
            stored.is_empty(),
            "an empty unique submit must not write, got {} rows",
            stored.len()
        );
    }

    /// uniqueness comes from the lens as well as the builder
    /// (`#[unique]` → `lens_field_unique`), so a field that was never marked by
    /// hand is required too — the rule is a property of the field, not of the
    /// declaration style.
    #[tokio::test]
    async fn lens_derived_unique_is_required_without_a_unique_call() {
        use topcoat::context::CxTestBuilder;

        use crate::schema::{Schema, TextInput};

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
            type Form = crate::NoForm<Self::Model>;

            fn table(_cx: &Cx) -> crate::resource::Table<Subscriber> {
                crate::resource::Table::new(
                    |r: &Subscriber| r.id.to_string(),
                    crate::resource::TextColumn::r#for(
                        Subscriber::fields().email(),
                        |r: &Subscriber| r.email.clone(),
                    ),
                )
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Subscriber))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let cx = CxTestBuilder::new().app_context(db).build();
        let mut ex = crate::db::db(&cx);

        let input = TextInput::r#for(Subscriber::fields().email());
        assert!(
            input.is_unique(),
            "the index must be recognized without a `.unique()` call (GH #183)"
        );
        assert!(input.is_required(), "derived uniqueness implies presence");

        let schema = Schema::new(input);
        let mut empty = HashMap::new();
        empty.insert("email".to_string(), "".to_string());
        assert_eq!(
            schema.validate(&empty).get("email"),
            Some(&vec!["Email is required".to_string()]),
            "an empty submit must be refused inline, not probed"
        );
        let errors =
            check_unique::<SubscriberResource>(&cx, &schema, &empty, &HashMap::new(), &mut ex)
                .await
                .unwrap();
        assert!(
            errors.is_empty(),
            "validation owns the empty case; the probe must add nothing, got {errors:?}"
        );
    }

    #[tokio::test]
    async fn unique_check_propagates_probe_errors() {
        use topcoat::context::CxTestBuilder;

        use crate::schema::{Schema, TextInput};

        #[derive(Debug, toasty::Model, Clone)]
        struct Probe {
            #[key]
            #[auto]
            id: uuid::Uuid,
            #[unique]
            email: String,
        }
        struct ProbeResource;
        impl Resource for ProbeResource {
            type Model = Probe;
            type Form = crate::NoForm<Self::Model>;

            fn table(_cx: &Cx) -> crate::resource::Table<Probe> {
                crate::resource::Table::new(
                    |r: &Probe| r.id.to_string(),
                    crate::resource::TextColumn::r#for(Probe::fields().email(), |r: &Probe| {
                        r.email.clone()
                    }),
                )
            }
        }

        // Schema never pushed: the probe query cannot run, so the check must
        // fail the submit instead of silently passing it.
        let db = Db::builder()
            .models(toasty::models!(Probe))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let cx = CxTestBuilder::new().app_context(db).build();
        let mut ex = crate::db::db(&cx);

        let schema = Schema::new(TextInput::r#for(Probe::fields().email()).unique());
        let mut values = HashMap::new();
        values.insert("email".to_string(), "a@b.c".to_string());
        let result =
            check_unique::<ProbeResource>(&cx, &schema, &values, &HashMap::new(), &mut ex).await;
        assert!(
            result.is_err(),
            "a failing probe must fail the submit, got {result:?}"
        );
    }

    #[tokio::test]
    async fn unique_check_ignores_absent_repeater_groups() {
        use topcoat::context::CxTestBuilder;

        use crate::schema::{Repeater, Schema, TextInput};

        #[derive(Debug, toasty::Model, Clone)]
        struct Nicknamed {
            #[key]
            #[auto]
            id: uuid::Uuid,
            #[unique]
            nickname: String,
        }
        struct TaggedResource;
        impl Resource for TaggedResource {
            type Model = Nicknamed;
            type Form = crate::NoForm<Self::Model>;

            fn table(_cx: &Cx) -> crate::resource::Table<Nicknamed> {
                crate::resource::Table::new(
                    |r: &Nicknamed| r.id.to_string(),
                    crate::resource::TextColumn::r#for(
                        Nicknamed::fields().nickname(),
                        |r: &Nicknamed| r.nickname.clone(),
                    ),
                )
            }
        }

        let mut db = Db::builder()
            .models(toasty::models!(Nicknamed))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        toasty::create!(Nicknamed {
            nickname: "".to_string()
        })
        .exec(&mut db)
        .await
        .unwrap();
        let cx = CxTestBuilder::new().app_context(db).build();
        let mut ex = crate::db::db(&cx);

        let schema = Schema::new(
            Repeater::new("Tags").schema(
                TextInput::r#for(Nicknamed::fields().nickname())
                    .unique()
                    .optional(),
            ),
        );

        // Absent group (all-inner-empty) with a stored `""`: validation calls
        // it clean, so the unique check must agree.
        let mut absent = HashMap::new();
        absent.insert("nickname".to_string(), "".to_string());
        assert!(
            schema.validate(&absent).is_empty(),
            "absent group must validate clean"
        );
        let errors =
            check_unique::<TaggedResource>(&cx, &schema, &absent, &HashMap::new(), &mut ex)
                .await
                .unwrap();
        assert!(
            errors.is_empty(),
            "absent group must not be unique-checked, got {errors:?}"
        );

        // Present group still checks: a taken value flags inline.
        let mut present = HashMap::new();
        present.insert("nickname".to_string(), "taken".to_string());
        toasty::create!(Nicknamed {
            nickname: "taken".to_string()
        })
        .exec(&mut ex)
        .await
        .unwrap();
        let errors =
            check_unique::<TaggedResource>(&cx, &schema, &present, &HashMap::new(), &mut ex)
                .await
                .unwrap();
        assert_eq!(
            errors.get("nickname"),
            Some(&vec!["Nickname has already been taken".to_string()]),
            "present group must still be unique-checked, got {errors:?}"
        );
    }

    /// the app-side unique probe binds the leaf's declared type. The
    /// stored token's canonical spelling is lower case, so an upper-case
    /// submission is a different string and the same `Uuid`: a text comparison
    /// finds no duplicate — and on this non-text column it cannot run at all —
    /// while the typed comparison refuses the submit.
    #[tokio::test]
    async fn a_typed_unique_field_probes_the_declared_type() {
        const TOKEN: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";

        let db = Db::builder()
            .models(toasty::models!(Tagged))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let mut db_q = db.clone();
        toasty::create!(Tagged {
            name: "one".to_string(),
            token: uuid::Uuid::parse_str(TOKEN).unwrap(),
        })
        .exec(&mut db_q)
        .await
        .unwrap();
        let router = panel_for::<TaggedResource>(db.clone())
            .build()
            .expect("panel builds");

        let csrf = uuid::Uuid::new_v4().to_string();
        let request = |body: String| {
            http::Request::builder()
                .method(http::Method::POST)
                .uri("/admin/tagged/create")
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    http::header::COOKIE,
                    format!("{}={csrf}", crate::csrf::COOKIE_NAME),
                )
                .body(Body::from(body))
                .unwrap()
        };

        // The upper-case spelling is not the stored one, so a text probe sees
        // no duplicate; the typed probe sees the same `Uuid`.
        let response = router
            .handle(request(format!(
                "name=two&token={}&csrf_token={csrf}",
                TOKEN.to_uppercase()
            )))
            .await;
        assert_eq!(
            response.status(),
            200,
            "the duplicate must re-render, not create"
        );
        let html = response_html(response).await;
        assert!(
            html.contains("Token has already been taken"),
            "the typed probe must see the duplicate, got {html}"
        );
        let mut db_q = db.clone();
        assert_eq!(
            Tagged::all().exec(&mut db_q).await.unwrap().len(),
            1,
            "a refused create writes nothing"
        );

        // The other direction: a genuinely different token still creates.
        let response = router
            .handle(request(format!(
                "name=two&token=3f8fad5b-d9cb-469f-a165-70867728950e&csrf_token={csrf}"
            )))
            .await;
        assert_eq!(
            response.status(),
            303,
            "a distinct value must create, not flag a duplicate"
        );
        let mut db_q = db.clone();
        assert_eq!(
            Tagged::all().exec(&mut db_q).await.unwrap().len(),
            2,
            "the accepted create writes its row"
        );
    }

    /// the edit exclusion normalises both sides through the leaf's own
    /// rule, so a re-spelled equivalent of the record's own value is that value
    /// and the save succeeds; another record's value still refuses.
    #[tokio::test]
    async fn a_typed_unique_field_skips_the_records_own_value_on_edit() {
        const MINE: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";
        const THEIRS: &str = "3f8fad5b-d9cb-469f-a165-70867728950e";

        let db = Db::builder()
            .models(toasty::models!(Tagged))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let mut db_q = db.clone();
        let mine = toasty::create!(Tagged {
            name: "mine".to_string(),
            token: uuid::Uuid::parse_str(MINE).unwrap(),
        })
        .exec(&mut db_q)
        .await
        .unwrap();
        toasty::create!(Tagged {
            name: "theirs".to_string(),
            token: uuid::Uuid::parse_str(THEIRS).unwrap(),
        })
        .exec(&mut db_q)
        .await
        .unwrap();
        let router = panel_for::<TaggedResource>(db.clone())
            .build()
            .expect("panel builds");

        let csrf = uuid::Uuid::new_v4().to_string();
        let url = format!("/admin/tagged/{}/edit", mine.id);
        let edit = |token: &str| {
            http::Request::builder()
                .method(http::Method::POST)
                .uri(&url)
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    http::header::COOKIE,
                    format!("{}={csrf}", crate::csrf::COOKIE_NAME),
                )
                .body(Body::from(format!(
                    "name=mine&token={token}&csrf_token={csrf}"
                )))
                .unwrap()
        };

        // The record's own token, re-spelled: the same value, so the save
        // succeeds instead of probing this record's own row.
        let response = router.handle(edit(&MINE.to_uppercase())).await;
        assert!(
            response.status().is_redirection(),
            "re-spelling the record's own value must save, got {} {}",
            response.status(),
            response_html(response).await
        );
        let mut db_q = db.clone();
        let saved = Tagged::filter(Tagged::fields().id().eq(mine.id))
            .first()
            .exec(&mut db_q)
            .await
            .unwrap()
            .expect("the edited record");
        assert_eq!(
            saved.token,
            uuid::Uuid::parse_str(MINE).unwrap(),
            "the re-spelled value is stored canonically"
        );

        // Another record holds the submitted token: refused, nothing written.
        let response = router.handle(edit(THEIRS)).await;
        assert_eq!(
            response.status(),
            200,
            "another record's value must refuse the edit"
        );
        let html = response_html(response).await;
        assert!(
            html.contains("Token has already been taken"),
            "the typed probe must see the other record, got {html}"
        );
        let mut db_q = db.clone();
        let unchanged = Tagged::filter(Tagged::fields().id().eq(mine.id))
            .first()
            .exec(&mut db_q)
            .await
            .unwrap()
            .expect("the refused record");
        assert_eq!(
            unchanged.token,
            uuid::Uuid::parse_str(MINE).unwrap(),
            "a refused edit writes nothing"
        );
    }
}
