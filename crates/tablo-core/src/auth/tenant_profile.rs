//! The panel's tenant settings page: the [`TenantEditor`] trait that edits the tenant the request
//! acts for, and the `{prefix}/tenant/profile` routes.

use std::{
    any::{Any, TypeId},
    future::Future,
};

use topcoat::{
    context::Cx,
    router::{
        Body,
        error::{forbidden, not_found, see_other},
    },
    view::BoxView,
};

use super::{PanelUser, SignedIn, input, membership, signed, unauthenticated_error};
use crate::{
    BoxFuture,
    auth::realm::{SettingsPage, current_realm, hosted, panel_prefix},
    flash::set_notification,
    form::{FieldErrors, FormInput, FormValues},
    outcome::Notification,
    schema::Schema,
    tenancy::Membership,
    topcoat_compat::async_page,
};

/// How a panel lets the signed-in user edit the tenant they act for on its settings page,
/// installed with [`Auth::tenant_profile`](super::Auth::tenant_profile).
///
/// It mirrors a [`ProfileEditor`](super::ProfileEditor), for the request's
/// [`Membership`]: [`may_edit`](Self::may_edit) says who may open the page,
/// [`fill`](Self::fill) loads the form from the tenant, [`validate`](Self::validate) refuses a
/// submission field by field, and [`save`](Self::save) writes it. `validate` and `save` run in one
/// transaction the framework opens.
///
/// ```rust
/// # use tablo_core::{Membership, PanelUser, auth::TenantEditor};
/// # use topcoat::context::Cx;
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Team { #[key] id: uuid::Uuid, name: String }
/// # struct Staff { owner: bool }
/// # impl PanelUser for Staff {
/// #     fn user_id(&self) -> String { String::new() }
/// #     fn display_name(&self) -> &str { "" }
/// # }
/// #[derive(tablo_core::FormInput)]
/// struct TeamSettings {
///     name: String,
/// }
///
/// struct Teams;
///
/// impl TenantEditor for Teams {
///     type Input = TeamSettings;
///     type User = Staff;
///
///     fn may_edit(&self, _cx: &Cx, staff: &Staff, _team: &Membership) -> bool {
///         staff.owner
///     }
///
///     async fn fill(
///         &self,
///         _cx: &Cx,
///         _staff: &Staff,
///         team: &Membership,
///     ) -> topcoat::Result<TeamSettings> {
///         Ok(TeamSettings {
///             name: team.name.clone(),
///         })
///     }
///
///     async fn save(
///         &self,
///         _cx: &Cx,
///         _staff: &Staff,
///         team: &Membership,
///         settings: TeamSettings,
///         ex: &mut dyn toasty::Executor,
///     ) -> topcoat::Result<()> {
///         Team::filter_by_id(team.tenant)
///             .update()
///             .name(settings.name)
///             .exec(ex)
///             .await?;
///         Ok(())
///     }
/// }
/// ```
pub trait TenantEditor: Send + Sync + 'static {
    /// What the settings page asks for: the app's own
    /// [`#[derive(FormInput)]`](derive@crate::FormInput) struct.
    type Input: FormInput;

    /// The user who edits the tenant: the type the panel's
    /// [`Authenticator`](super::Authenticator) loads, which mounting checks.
    type User: PanelUser;

    /// Whether `user` may edit `tenant`, one of their memberships. Defaults to every member. A
    /// user who may not gets no link to the page, and the page answers them 403.
    fn may_edit(&self, _cx: &Cx, _user: &Self::User, _tenant: &Membership) -> bool {
        true
    }

    /// The form as the page first shows it, loaded for `tenant`. The page renders what
    /// [`FormInput::write`] writes.
    fn fill(
        &self,
        cx: &Cx,
        user: &Self::User,
        tenant: &Membership,
    ) -> impl Future<Output = topcoat::Result<Self::Input>> + Send;

    /// Refuse a submission [`save`](Self::save) should not receive: each error names an input
    /// field's key and renders under its control. Defaults to none. A key no control renders is
    /// a declaration error: the page would show no message.
    ///
    /// It runs inside the save's transaction `ex`, after the input parses and its controls' own
    /// rules pass.
    fn validate(
        &self,
        _cx: &Cx,
        _user: &Self::User,
        _tenant: &Membership,
        _input: &Self::Input,
        _ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = topcoat::Result<FieldErrors>> + Send {
        async { Ok(FieldErrors::new()) }
    }

    /// Write a validated `input` to `tenant` through the transaction `ex`, which commits when it
    /// returns `Ok`.
    fn save(
        &self,
        cx: &Cx,
        user: &Self::User,
        tenant: &Membership,
        input: Self::Input,
        ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = topcoat::Result<()>> + Send;
}

/// What a tenant settings submission came to.
pub(crate) enum Outcome {
    /// The committed change.
    Saved,
    /// The refused submission, its values, and why.
    Refused {
        values: FormValues,
        errors: FieldErrors,
    },
}

/// [`TenantEditor`] with its input and user types erased, as the panel keeps it.
pub(crate) trait DynTenantEditor: Send + Sync {
    /// The settings form.
    fn schema(&self) -> Schema;
    /// Whether `user` may edit `tenant`; `false` for a user of another type.
    fn may_edit(&self, cx: &Cx, user: &dyn PanelUser, tenant: &Membership) -> bool;
    /// The form's first values for `tenant`, or `None` for a user of another type.
    fn values<'a>(
        &'a self,
        cx: &'a Cx,
        user: &'a dyn PanelUser,
        tenant: &'a Membership,
    ) -> BoxFuture<'a, topcoat::Result<Option<FormValues>>>;
    /// Parses, checks, validates and saves one submission for `signed`'s user and `tenant`.
    fn submit<'a>(
        &'a self,
        cx: &'a Cx,
        signed: &'a SignedIn,
        tenant: &'a Membership,
        values: FormValues,
    ) -> BoxFuture<'a, topcoat::Result<Outcome>>;
    /// The [`TypeId`] and name of [`TenantEditor::User`], which mounting compares with the
    /// authenticator's.
    fn user_type(&self) -> (TypeId, &'static str);
}

impl<E: TenantEditor> DynTenantEditor for E {
    fn schema(&self) -> Schema {
        E::Input::schema()
    }

    fn may_edit(&self, cx: &Cx, user: &dyn PanelUser, tenant: &Membership) -> bool {
        let user: &dyn Any = user;
        user.downcast_ref::<E::User>()
            .is_some_and(|user| TenantEditor::may_edit(self, cx, user, tenant))
    }

    fn values<'a>(
        &'a self,
        cx: &'a Cx,
        user: &'a dyn PanelUser,
        tenant: &'a Membership,
    ) -> BoxFuture<'a, topcoat::Result<Option<FormValues>>> {
        Box::pin(async move {
            let user: &dyn Any = user;
            let Some(user) = user.downcast_ref::<E::User>() else {
                return Ok(None);
            };
            let mut values = FormValues::new();
            self.fill(cx, user, tenant)
                .await
                .map_err(write_failure)?
                .write(&mut values);
            Ok(Some(values))
        })
    }

    fn submit<'a>(
        &'a self,
        cx: &'a Cx,
        signed: &'a SignedIn,
        tenant: &'a Membership,
        mut values: FormValues,
    ) -> BoxFuture<'a, topcoat::Result<Outcome>> {
        Box::pin(async move {
            let any: &dyn Any = &*signed.user;
            let user = any
                .downcast_ref::<E::User>()
                .ok_or_else(|| topcoat::Error::from(forbidden()))?;
            let schema = E::Input::schema();
            let mut stored = FormValues::new();
            self.fill(cx, user, tenant)
                .await
                .map_err(write_failure)?
                .write(&mut stored);
            // A control the page renders disabled posts nothing the user may change: it keeps
            // what `fill` wrote, whatever a crafted POST carries.
            for key in schema.disabled_keys(true) {
                match stored.get(key) {
                    Some(value) => values.insert(key, value.clone()),
                    None => values.remove(key),
                };
            }
            schema.keep_unedited(&mut values, &stored, &crate::time_zone(cx));
            let input =
                match input::parse::<E::Input>(cx, &values, &stored, "tenant settings").await? {
                    Ok(input) => input,
                    Err(errors) => return Ok(Outcome::Refused { values, errors }),
                };
            let mut db = crate::db::db(cx)?;
            let mut tx = db.transaction().await.map_err(crate::error::unavailable)?;
            let errors = self
                .validate(cx, user, tenant, &input, &mut tx)
                .await
                .map_err(write_failure)?;
            input::placed(&schema, &errors, "the tenant editor", "tenant settings")?;
            if !errors.is_empty() {
                return Ok(Outcome::Refused { values, errors });
            }
            self.save(cx, user, tenant, input, &mut tx)
                .await
                .map_err(write_failure)?;
            tx.commit().await.map_err(crate::error::unavailable)?;
            Ok(Outcome::Saved)
        })
    }

    fn user_type(&self) -> (TypeId, &'static str) {
        (TypeId::of::<E::User>(), std::any::type_name::<E::User>())
    }
}

/// A failed tenant read or write: a database failure becomes the opaque outage, logged; an
/// editor's own error keeps its mapping.
fn write_failure(error: topcoat::Error) -> topcoat::Error {
    crate::error::driver_failure(error, crate::error::DATABASE_UNAVAILABLE)
}

/// The hidden field naming the tenant the form was filled from: a tab opened before another
/// switched tenants must not save over the new one.
const TENANT_FIELD: &str = "editing_tenant";

/// The keys a tenant settings POST carries besides its input, which no input field may post.
pub(crate) const RESERVED_KEYS: [&str; 2] = [crate::csrf::FIELD_NAME, TENANT_FIELD];

/// Where the panel's tenant settings page lives: `{prefix}/tenant/profile`.
pub(crate) fn tenant_profile_url(cx: &Cx) -> String {
    format!("{}/tenant/profile", panel_prefix(cx))
}

/// Whether the request's signed-in user may open the tenant settings page for the tenant they act
/// for.
pub(crate) fn offered(cx: &Cx) -> bool {
    editing(cx).is_ok()
}

/// The signed-in user, the panel's editor and the membership the request acts under, or the
/// answer to a request without them: 404 without an editor, 403 without a tenant the user may
/// edit.
fn editing(cx: &Cx) -> topcoat::Result<(&SignedIn, &dyn DynTenantEditor, &Membership)> {
    let signed = signed(cx).ok_or_else(|| unauthenticated_error(cx))?;
    let editor = current_realm(cx)
        .and_then(|panel| panel.auth.tenant_editor())
        .ok_or_else(not_found)?;
    let tenant = membership(cx)
        .filter(|tenant| editor.may_edit(cx, signed.user.as_ref(), tenant))
        .ok_or_else(forbidden)?;
    Ok((signed, editor, tenant))
}

/// `GET {prefix}/tenant/profile` — the settings form, filled from the tenant.
pub(super) fn tenant_profile_page(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let (signed, editor, tenant) = editing(cx)?;
        let values = editor
            .values(cx, signed.user.as_ref(), tenant)
            .await?
            .ok_or_else(forbidden)?;
        crate::csrf::ensure_token(cx);
        render(cx, editor, tenant, &values, &FieldErrors::new()).await
    })
}

/// `POST {prefix}/tenant/profile` — save the settings, then land back on the page.
pub(super) fn tenant_profile_post(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let (signed, editor, tenant) = editing(cx)?;
        let form = crate::auth::input::read_form(cx, body).await?;
        crate::csrf::verify(cx, form.last(crate::csrf::FIELD_NAME))?;
        if form.last(TENANT_FIELD) != Some(tenant.tenant.to_string().as_str()) {
            set_notification(
                cx,
                Notification::warning(crate::t!(cx, "tablo-tenant-not-saved")).description(
                    crate::t!(cx, "tablo-tenant-switched", tenant = tenant.name.clone()),
                ),
            );
            return Err(see_other(tenant_profile_url(cx)).into());
        }
        let input_schema = editor.schema();
        let values = input_schema.read_input(cx, &form.pairs, &RESERVED_KEYS)?;
        if let Outcome::Refused { values, errors } =
            editor.submit(cx, signed, tenant, values).await?
        {
            return render(cx, editor, tenant, &values, &errors).await;
        }
        set_notification(
            cx,
            Notification::success(crate::t!(cx, "tablo-tenant-saved")),
        );
        Err(see_other(tenant_profile_url(cx)).into())
    })
}

/// The settings page in the panel's shell: the editor's form for `tenant` with `values` and
/// `errors`, saved to the page itself, cancelled to the panel's home.
async fn render<'a>(
    cx: &'a Cx,
    editor: &dyn DynTenantEditor,
    tenant: &Membership,
    values: &FormValues,
    errors: &FieldErrors,
) -> topcoat::Result<BoxView<'a>> {
    let schema = editor.schema();
    let page = SettingsPage {
        title: crate::t!(cx, "tablo-shell-tenant-settings"),
        schema: &schema,
        hidden: Some((TENANT_FIELD, tenant.tenant.to_string())),
        values,
        errors,
    };
    hosted(cx)?.settings_page(cx, page).await
}

/// The mistakes in a panel's [`Auth::tenant_profile`](super::Auth::tenant_profile): a settings
/// page without auth, an editor whose user is not the authenticator's, and a form field the page
/// cannot serve.
pub(super) fn declaration_errors(auth: &super::Auth) -> Vec<crate::DeclarationErrorKind> {
    use super::input::FieldFault;
    use crate::{DeclarationErrorKind, declaration::TenantProfileFault as Fault};

    let Some(editor) = auth.tenant_editor.as_deref() else {
        return Vec::new();
    };
    let Some(authenticator) = auth.authenticator() else {
        return vec![DeclarationErrorKind::TenantProfile(Fault::AuthDisabled)];
    };
    let mut kinds = Vec::new();
    let (user, user_name) = editor.user_type();
    if user != authenticator.user_type() {
        kinds.push(DeclarationErrorKind::TenantProfile(Fault::UserType {
            editor: user_name,
        }));
    }
    let schema = editor.schema();
    kinds.extend(schema.declaration_errors());
    kinds.extend(schema.empty_choices());
    kinds.extend(
        input::field_faults(&schema, &RESERVED_KEYS).map(|(fault, name)| {
            DeclarationErrorKind::TenantProfile(match fault {
                FieldFault::Reserved => Fault::ReservedField(name),
                FieldFault::File => Fault::FileField(name),
                FieldFault::Relationship => Fault::RelationshipField(name),
            })
        }),
    );
    kinds
}
