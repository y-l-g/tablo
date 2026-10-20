//! The panel's tenant registration page: the [`TenantRegistrar`] trait that creates a tenant for
//! the signed-in user, the [`TenantRegistration`] a panel installs it with, and the
//! `{prefix}/register-tenant` routes.

use std::{
    any::{Any, TypeId},
    future::Future,
};

use topcoat::{
    context::Cx,
    icon::icon,
    router::{
        Body, RouteFuture,
        error::{forbidden, not_found, see_other},
        response::{IntoResponse, Response},
    },
    view::{ViewExt, attributes, view},
};

use super::{
    PanelUser, SignedIn, infrastructure_failure, input,
    login::logout_url,
    page::{AuthPage, render, switch_link},
    panel_root,
    session::select_tenant,
    signed, unauthenticated_error,
};
use crate::{
    BoxFuture,
    form::{FieldErrors, FormInput, FormValues},
    notification::{Notification, set_notification},
    panel::{
        Panel, panel_prefix,
        state::{PanelState, current},
    },
    schema::{Schema, Source},
    tenancy::Membership,
};

/// How a panel lets the signed-in user create a tenant to act for, installed with a
/// [`TenantRegistration`].
///
/// It mirrors a [`Registrar`](super::Registrar): [`Input`](Self::Input) is the form the page asks
/// for, [`validate`](Self::validate) refuses a submission field by field, and
/// [`register`](Self::register) writes the tenant and the user's membership of it. Both run in
/// one transaction the framework opens. The next request reloads the user, whose
/// [`tenants`](PanelUser::tenants) must then list the new one.
///
/// ```rust
/// # use tablo_core::{Membership, PanelUser, auth::TenantRegistrar};
/// # use topcoat::context::Cx;
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Team { #[key] #[auto] id: uuid::Uuid, name: String }
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Seat { #[key] #[auto] id: uuid::Uuid, staff_id: uuid::Uuid, team_id: uuid::Uuid }
/// # struct Staff { id: uuid::Uuid, teams: Vec<Membership> }
/// # impl PanelUser for Staff {
/// #     fn user_id(&self) -> String { self.id.to_string() }
/// #     fn display_name(&self) -> &str { "" }
/// #     fn tenants(&self) -> &[Membership] { &self.teams }
/// # }
/// #[derive(tablo_core::FormInput)]
/// struct NewTeam {
///     name: String,
/// }
///
/// struct Teams;
///
/// impl TenantRegistrar for Teams {
///     type Input = NewTeam;
///     type User = Staff;
///
///     async fn register(
///         &self,
///         _cx: &Cx,
///         staff: &Staff,
///         team: NewTeam,
///         ex: &mut dyn toasty::Executor,
///     ) -> topcoat::Result<Membership> {
///         let team = toasty::create!(Team { name: team.name })
///             .exec(&mut *ex)
///             .await?;
///         toasty::create!(Seat {
///             staff_id: staff.id,
///             team_id: team.id,
///         })
///         .exec(ex)
///         .await?;
///         Ok(Membership::new(team.id, team.name))
///     }
/// }
/// ```
pub trait TenantRegistrar: Send + Sync + 'static {
    /// What the page asks for: the app's own
    /// [`#[derive(FormInput)]`](derive@crate::FormInput) struct.
    type Input: FormInput;

    /// The user who registers the tenant: the type the panel's
    /// [`Authenticator`](super::Authenticator) loads, which mounting checks.
    type User: PanelUser;

    /// Refuse a submission [`register`](Self::register) should not receive, such as a name
    /// another tenant holds: each error names an input field's key and renders under its
    /// control. Defaults to none. A key no control renders is a declaration error: the page
    /// would show no message.
    ///
    /// It runs inside the registration's transaction `ex`, after the input parses and its
    /// controls' own rules pass.
    fn validate(
        &self,
        _cx: &Cx,
        _user: &Self::User,
        _input: &Self::Input,
        _ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = topcoat::Result<FieldErrors>> + Send {
        async { Ok(FieldErrors::new()) }
    }

    /// Create the tenant from a validated `input`, with `user` a member of it, through the
    /// transaction `ex`, which commits when it returns `Ok`; answer the membership the request's
    /// session then acts under.
    fn register(
        &self,
        cx: &Cx,
        user: &Self::User,
        input: Self::Input,
        ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = topcoat::Result<Membership>> + Send;
}

/// A panel's tenant registration page, installed with
/// [`Auth::tenant_registration`](super::Auth::tenant_registration): the [`TenantRegistrar`] that
/// creates tenants, and whether a user who acts for none must create one first.
///
/// ```rust
/// # use tablo_core::{Auth, Membership, PanelUser, auth::{Authenticator, TenantRegistrar, TenantRegistration}};
/// # use topcoat::context::Cx;
/// # struct Staff;
/// # impl PanelUser for Staff {
/// #     fn user_id(&self) -> String { String::new() }
/// #     fn display_name(&self) -> &str { "" }
/// # }
/// # struct StaffAuth;
/// # impl Authenticator for StaffAuth {
/// #     type User = Staff;
/// #     async fn verify(&self, _: &Cx, _: &str, _: &str) -> topcoat::Result<Option<Staff>> { Ok(None) }
/// #     async fn find_by_id(&self, _: &Cx, _: &str) -> topcoat::Result<Option<Staff>> { Ok(None) }
/// # }
/// # #[derive(tablo_core::FormInput)]
/// # struct NewTeam { name: String }
/// # impl TenantRegistrar for StaffAuth {
/// #     type Input = NewTeam;
/// #     type User = Staff;
/// #     async fn register(&self, _: &Cx, _: &Staff, _: NewTeam, _: &mut dyn toasty::Executor)
/// #         -> topcoat::Result<Membership> { unimplemented!() }
/// # }
/// let auth = Auth::custom(StaffAuth).tenant_registration(TenantRegistration::new(StaffAuth).required());
/// # let _ = auth;
/// ```
pub struct TenantRegistration {
    registrar: Box<dyn DynTenantRegistrar>,
    required: bool,
}

impl TenantRegistration {
    /// Registers tenants through `registrar`. A user who acts for no tenant still reaches the
    /// panel, and its resources that need a tenant answer 403; see [`required`](Self::required).
    #[must_use]
    pub fn new(registrar: impl TenantRegistrar) -> Self {
        Self {
            registrar: Box::new(registrar),
            required: false,
        }
    }

    /// Sends a signed-in user who acts for no tenant to the registration page before anything
    /// else: a page answers with a redirect to it and any other request with 403, as for an
    /// unverified address, which comes first. Leave it off where some users, such as operators,
    /// act for no tenant on purpose.
    #[must_use]
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub(crate) fn registrar(&self) -> &dyn DynTenantRegistrar {
        &*self.registrar
    }
}

impl std::fmt::Debug for TenantRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenantRegistration")
            .field("required", &self.required)
            .finish_non_exhaustive()
    }
}

/// What a tenant registration came to.
pub(crate) enum Outcome {
    /// The committed tenant, as the user's membership.
    Registered(Membership),
    /// The refused submission, its values, and why.
    Refused {
        values: FormValues,
        errors: FieldErrors,
    },
}

/// [`TenantRegistrar`] with its input and user types erased, as the panel keeps it.
pub(crate) trait DynTenantRegistrar: Send + Sync {
    /// The registration form.
    fn schema(&self) -> Schema;
    /// Parses, checks, validates and registers one submission for `signed`'s user.
    fn register<'a>(
        &'a self,
        cx: &'a Cx,
        signed: &'a SignedIn,
        values: FormValues,
    ) -> BoxFuture<'a, topcoat::Result<Outcome>>;
    /// The [`TypeId`] and name of [`TenantRegistrar::User`], which mounting compares with the
    /// authenticator's.
    fn user_type(&self) -> (TypeId, &'static str);
}

impl<R: TenantRegistrar> DynTenantRegistrar for R {
    fn schema(&self) -> Schema {
        R::Input::schema()
    }

    fn register<'a>(
        &'a self,
        cx: &'a Cx,
        signed: &'a SignedIn,
        values: FormValues,
    ) -> BoxFuture<'a, topcoat::Result<Outcome>> {
        Box::pin(async move {
            let any: &dyn Any = &*signed.user;
            let user = any
                .downcast_ref::<R::User>()
                .ok_or_else(|| topcoat::Error::from(forbidden()))?;
            let input =
                match input::parse::<R::Input>(cx, &values, &FormValues::new(), "tenant").await? {
                    Ok(input) => input,
                    Err(errors) => return Ok(Outcome::Refused { values, errors }),
                };
            let mut db = crate::db::db(cx);
            let mut tx = db.transaction().await.map_err(infrastructure_failure)?;
            let errors = self
                .validate(cx, user, &input, &mut tx)
                .await
                .map_err(infrastructure_failure)?;
            input::placed(
                &R::Input::schema(),
                &errors,
                "the tenant registrar",
                "tenant",
            )?;
            if !errors.is_empty() {
                return Ok(Outcome::Refused { values, errors });
            }
            let membership = TenantRegistrar::register(self, cx, user, input, &mut tx)
                .await
                .map_err(infrastructure_failure)?;
            tx.commit().await.map_err(infrastructure_failure)?;
            Ok(Outcome::Registered(membership))
        })
    }

    fn user_type(&self) -> (TypeId, &'static str) {
        (TypeId::of::<R::User>(), std::any::type_name::<R::User>())
    }
}

/// The keys a tenant registration POST carries besides its input, which no input field may post.
pub(crate) const RESERVED_KEYS: [&str; 1] = [crate::csrf::FIELD_NAME];

/// Where the panel's tenant registration page lives: `{prefix}/register-tenant`.
pub(crate) fn register_tenant_url(cx: &Cx) -> String {
    format!("{}/register-tenant", panel_prefix(cx))
}

/// Whether the request's panel installs a [`TenantRegistration`].
pub(crate) fn offered(cx: &Cx) -> bool {
    current(cx).is_some_and(|panel| panel.auth.tenant_registration_page().is_some())
}

/// Whether `user` must register a tenant before `panel` serves them anything else: the panel
/// requires one, and the user acts for none.
pub(super) fn pending(panel: &PanelState, user: &dyn PanelUser) -> bool {
    panel
        .auth
        .tenant_registration_page()
        .is_some_and(|registration| registration.required)
        && user.tenants().is_empty()
}

/// The signed-in user and the panel's registrar, or the answer to a request without them.
fn registering(cx: &Cx) -> topcoat::Result<(&SignedIn, &dyn DynTenantRegistrar)> {
    let signed = signed(cx).ok_or_else(|| unauthenticated_error(cx))?;
    let registration = signed
        .panel
        .auth
        .tenant_registration_page()
        .ok_or_else(not_found)?;
    Ok((signed, registration.registrar()))
}

/// `GET {prefix}/register-tenant` — the standalone tenant registration page.
pub(super) fn register_tenant_page(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let (signed, registrar) = registering(cx)?;
        page_response(
            cx,
            signed,
            registrar,
            &FormValues::new(),
            &FieldErrors::new(),
        )
        .await
    })
}

/// `POST {prefix}/register-tenant` — create the tenant, act for it, then land on the panel root.
pub(super) fn register_tenant_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let (signed, registrar) = registering(cx)?;
        let form = crate::panel::parse_form_body(cx, body).await?;
        crate::csrf::verify(cx, form.last(crate::csrf::FIELD_NAME))?;
        let input_schema = registrar.schema();
        let values = input_schema.read_input(cx, &form.pairs, &RESERVED_KEYS)?;
        let membership = match registrar.register(cx, signed, values).await? {
            Outcome::Registered(membership) => membership,
            Outcome::Refused { values, errors } => {
                return page_response(cx, signed, registrar, &values, &errors).await;
            }
        };
        select_tenant(cx, membership.tenant).await?;
        set_notification(
            cx,
            Notification::success("Tenant created")
                .description(format!("You now act for {}.", membership.name)),
        );
        see_other(panel_root(cx)).into_response(cx)
    })
}

/// The registration page: 422 for a refused submission, else 200. A user who acts for a tenant
/// already may go back to the panel; one who acts for none may only sign out.
async fn page_response(
    cx: &Cx,
    signed: &SignedIn,
    registrar: &dyn DynTenantRegistrar,
    values: &FormValues,
    errors: &FieldErrors,
) -> topcoat::Result<Response> {
    let csrf = crate::csrf::ensure_token(cx);
    let action = register_tenant_url(cx);
    let schema = registrar.schema();
    let fields = schema.render(cx, Source::form(values, errors)).await?;
    let first = signed.user.tenants().is_empty();
    let logout = logout_url(cx);
    let form = view! {
        cx =>
        <div class="flex flex-col gap-3">
            <form method="post" action=(action) class="flex flex-col gap-5">
                (crate::csrf::field(cx, &csrf))
                (fields)
                tablo_ui::button(
                    variant: tablo_ui::ButtonVariant::Primary,
                    attrs: attributes! { type="submit" class="w-full" },
                    "Create tenant"
                )
            </form>
            if first {
                <form method="post" action=(logout)>
                    (crate::csrf::field(cx, &csrf))
                    tablo_ui::button(
                        variant: tablo_ui::ButtonVariant::Outline,
                        attrs: attributes! { type="submit" class="w-full" },
                        icon(data: tablo_ui::icons::LOG_OUT)
                        "Sign out"
                    )
                </form>
            }
        </div>
    }
    .boxed();
    let root = panel_root(cx);
    let brand = Panel::brand_name(cx);
    let page = render(
        cx,
        AuthPage {
            title: "Create a tenant",
            description: if first {
                format!("Create the tenant you work in on {brand}.")
            } else {
                format!("Create another tenant to work in on {brand}.")
            },
            status: (!errors.is_empty()).then_some(http::StatusCode::UNPROCESSABLE_ENTITY),
            form,
            switch: (!first).then(|| switch_link(cx, "Changed your mind?", &root, "Go back")),
            note: None,
        },
    )
    .await?;
    page.single().await?.into_response(cx)
}

/// The mistakes in a panel's [`Auth::tenant_registration`](super::Auth::tenant_registration): a
/// registration without auth, a registrar whose user is not the authenticator's, and a form field
/// the page cannot serve.
pub(super) fn declaration_errors(auth: &super::Auth) -> Vec<crate::DeclarationErrorKind> {
    use super::input::FieldFault;
    use crate::{DeclarationErrorKind, declaration::TenantRegistrationFault as Fault};

    let Some(registrar) = auth
        .tenant_registration
        .as_ref()
        .map(TenantRegistration::registrar)
    else {
        return Vec::new();
    };
    let Some(authenticator) = auth.authenticator() else {
        return vec![DeclarationErrorKind::TenantRegistration(
            Fault::AuthDisabled,
        )];
    };
    let mut kinds = Vec::new();
    let (user, user_name) = registrar.user_type();
    if user != authenticator.user_type() {
        kinds.push(DeclarationErrorKind::TenantRegistration(Fault::UserType {
            registrar: user_name,
        }));
    }
    let schema = registrar.schema();
    kinds.extend(schema.declaration_errors());
    kinds.extend(schema.empty_choices());
    kinds.extend(
        input::field_faults(&schema, &RESERVED_KEYS).map(|(fault, name)| {
            DeclarationErrorKind::TenantRegistration(match fault {
                FieldFault::Reserved => Fault::ReservedField(name),
                FieldFault::File => Fault::FileField(name),
                FieldFault::Relationship => Fault::RelationshipField(name),
            })
        }),
    );
    kinds
}
