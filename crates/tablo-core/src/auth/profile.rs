//! The panel's profile page: the [`ProfileEditor`] trait that edits the signed-in account, the
//! shipped [`Profile`] form, and the `{prefix}/profile` routes.

use std::{
    any::{Any, TypeId},
    collections::HashSet,
    future::Future,
};

use topcoat::{
    context::Cx,
    router::{
        Body,
        error::{forbidden, not_found, see_other, too_many_requests},
        request::client_ip,
    },
    session,
    view::BoxView,
};

use super::{
    PanelUser, SignedIn, infrastructure_failure, panel_root, registration::new_password_errors,
    session::record, signed, unauthenticated_error,
};
use crate::{
    BoxFuture,
    form::{FieldError, FieldErrors, FormInput, FormValues, parse_password, parse_scalar},
    notification::{Notification, set_notification},
    panel::{FormChrome, panel_prefix, render_form_page, state::current},
    schema::{Field, Schema},
    topcoat_compat::async_page,
};

/// How a panel lets the signed-in user edit their own account on its profile page, installed
/// with [`Auth::profile`](super::Auth::profile).
///
/// It mirrors a [`Registrar`](super::Registrar): [`Input`](Self::Input) is the form the page
/// shows, [`fill`](Self::fill) seeds it from the account, [`validate`](Self::validate) refuses a
/// submission field by field, and [`save`](Self::save) writes it. Both run in one transaction
/// the framework opens.
///
/// The shipped [`PasswordAuth`](super::PasswordAuth) edits an [`AdminUser`](super::AdminUser)
/// through a [`Profile`]; an app with its own user table implements it beside its
/// [`Authenticator`](super::Authenticator), for the same user type:
///
/// ```rust
/// # use tablo_core::{PanelUser, auth::{ProfileEditor, ProfileUpdate}};
/// # use topcoat::context::Cx;
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Staff {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     name: String,
/// # }
/// # impl PanelUser for Staff {
/// #     fn user_id(&self) -> String { self.id.to_string() }
/// #     fn display_name(&self) -> &str { &self.name }
/// # }
/// #[derive(tablo_core::FormInput)]
/// struct Rename {
///     name: String,
/// }
///
/// struct StaffProfile;
///
/// impl ProfileEditor for StaffProfile {
///     type Input = Rename;
///     type User = Staff;
///
///     fn fill(&self, _cx: &Cx, staff: &Staff) -> Rename {
///         Rename {
///             name: staff.name.clone(),
///         }
///     }
///
///     async fn save(
///         &self,
///         _cx: &Cx,
///         staff: &Staff,
///         rename: Rename,
///         ex: &mut dyn toasty::Executor,
///     ) -> topcoat::Result<ProfileUpdate> {
///         Staff::filter(Staff::fields().id().eq(staff.id))
///             .update()
///             .name(rename.name)
///             .exec(ex)
///             .await?;
///         Ok(ProfileUpdate::Details)
///     }
/// }
/// ```
pub trait ProfileEditor: Send + Sync + 'static {
    /// What the profile page asks for: [`Profile`], or the app's own
    /// [`#[derive(FormInput)]`](derive@crate::FormInput) struct.
    type Input: FormInput;

    /// The account the page edits: the type the panel's [`Authenticator`](super::Authenticator)
    /// loads, which mounting checks.
    type User: PanelUser;

    /// The form as the page first shows it, from the signed-in `user`. The page renders what
    /// [`FormInput::write`] writes, so an input that never writes its passwords shows them
    /// empty.
    fn fill(&self, cx: &Cx, user: &Self::User) -> Self::Input;

    /// Refuse a submission [`save`](Self::save) should not receive, such as a wrong current
    /// password: each error names an input field's key and renders under its control. Defaults
    /// to none. A key no control renders is a declaration error: the page would show no message.
    ///
    /// It runs inside the save's transaction `ex`, after the input parses and its controls' own
    /// rules pass.
    fn validate(
        &self,
        _cx: &Cx,
        _user: &Self::User,
        _input: &Self::Input,
        _ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = topcoat::Result<FieldErrors>> + Send {
        async { Ok(FieldErrors::new()) }
    }

    /// Write a validated `input` to `user`'s account through the transaction `ex`, which commits
    /// when it returns `Ok`, and say whether a credential changed.
    fn save(
        &self,
        cx: &Cx,
        user: &Self::User,
        input: Self::Input,
        ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = topcoat::Result<ProfileUpdate>> + Send;
}

/// What a [`ProfileEditor::save`] changed, which decides what happens to the user's sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileUpdate {
    /// Nothing the user signs in with: every session stays signed in.
    Details,
    /// Something the user signs in with, such as a password: every other session of the user is
    /// revoked, and the browser that saved gets a new session.
    Credentials,
}

/// What a profile submission came to.
pub(crate) enum Outcome {
    /// The committed change.
    Saved(ProfileUpdate),
    /// The refused submission, its values, and why. The page renders a password control empty.
    Refused {
        values: FormValues,
        errors: FieldErrors,
    },
}

/// [`ProfileEditor`] with its input and user types erased, as the panel keeps it.
pub(crate) trait DynProfileEditor: Send + Sync {
    /// The profile form.
    fn schema(&self) -> Schema;
    /// The form's first values for `user`, or `None` for a user of another type.
    fn values(&self, cx: &Cx, user: &dyn PanelUser) -> Option<FormValues>;
    /// Parses, checks, validates and saves one submission for `user`.
    fn submit<'a>(
        &'a self,
        cx: &'a Cx,
        user: &'a dyn PanelUser,
        values: FormValues,
    ) -> BoxFuture<'a, topcoat::Result<Outcome>>;
    /// The [`TypeId`] and name of [`ProfileEditor::User`], which mounting compares with the
    /// authenticator's.
    fn user_type(&self) -> (TypeId, &'static str);
}

impl<E: ProfileEditor> DynProfileEditor for E {
    fn schema(&self) -> Schema {
        E::Input::schema()
    }

    fn values(&self, cx: &Cx, user: &dyn PanelUser) -> Option<FormValues> {
        let user: &dyn Any = user;
        let user = user.downcast_ref::<E::User>()?;
        let mut values = FormValues::new();
        self.fill(cx, user).write(&mut values);
        Some(values)
    }

    fn submit<'a>(
        &'a self,
        cx: &'a Cx,
        user: &'a dyn PanelUser,
        values: FormValues,
    ) -> BoxFuture<'a, topcoat::Result<Outcome>> {
        Box::pin(async move {
            let any: &dyn Any = user;
            let user = any
                .downcast_ref::<E::User>()
                .ok_or_else(|| topcoat::Error::from(forbidden()))?;
            let schema = E::Input::schema();
            let mut errors = FieldErrors::new();
            let parsed = match E::Input::parse(cx, &values) {
                Ok(parsed) => Some(parsed),
                Err(refused) => {
                    for error in refused {
                        errors.push(error);
                    }
                    None
                }
            };
            let mut stored = FormValues::new();
            self.fill(cx, user).write(&mut stored);
            schema
                .check_controls(cx, &values, &stored, &mut errors)
                .await;
            let refused = |errors| Outcome::Refused {
                values: values.clone(),
                errors,
            };
            if let Some(key) = schema.unplaced_error(&errors) {
                return Err(crate::error::declaration(format!(
                    "the profile input refused key `{key}`, which no profile control renders"
                )));
            }
            let Some(input) = parsed.filter(|_| errors.is_empty()) else {
                return Ok(refused(errors));
            };
            let mut db = crate::db::db(cx);
            let mut tx = db.transaction().await.map_err(infrastructure_failure)?;
            let errors = self.validate(cx, user, &input, &mut tx).await?;
            if let Some(key) = schema.unplaced_error(&errors) {
                return Err(crate::error::declaration(format!(
                    "the profile editor refused key `{key}`, which no profile control renders"
                )));
            }
            if !errors.is_empty() {
                return Ok(refused(errors));
            }
            let update = self.save(cx, user, input, &mut tx).await?;
            tx.commit().await.map_err(infrastructure_failure)?;
            Ok(Outcome::Saved(update))
        })
    }

    fn user_type(&self) -> (TypeId, &'static str) {
        (TypeId::of::<E::User>(), std::any::type_name::<E::User>())
    }
}

/// The profile form [`PasswordAuth`](super::PasswordAuth) edits an
/// [`AdminUser`](super::AdminUser) through, and a custom [`ProfileEditor`] can take as its
/// input: a name, an email address, an optional new password typed twice, and the current
/// password.
#[derive(Clone, PartialEq, Eq)]
pub struct Profile {
    /// The display name, posted as `name`.
    pub name: String,
    /// The email address the account signs in with, posted as `email`.
    pub email: String,
    /// The new password, posted as `password`; `None` when left blank, which keeps the current
    /// one.
    pub password: Option<String>,
    /// The new password typed again, posted as `password_confirmation`.
    pub password_confirmation: String,
    /// The password the account signs in with today, posted as `current_password`; `None` when
    /// left blank.
    pub current_password: Option<String>,
}

impl Profile {
    /// The refusals [`new_password_errors`] makes of a new password; none when the profile keeps
    /// the current one.
    pub fn password_errors(&self) -> FieldErrors {
        match &self.password {
            Some(password) => new_password_errors(password, &self.password_confirmation),
            None => FieldErrors::new(),
        }
    }
}

/// Redacts the passwords, so a profile logged with `?` writes none to the log.
impl std::fmt::Debug for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Profile")
            .field("name", &self.name)
            .field("email", &self.email)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("password_confirmation", &"<redacted>")
            .field(
                "current_password",
                &self.current_password.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl FormInput for Profile {
    fn schema() -> Schema {
        let required = crate::form::required_input;
        Schema::new((
            required(Field::text_input::<String>("name").label("Name")),
            required(Field::text_input::<String>("email").email().label("Email")),
            Field::text_input::<String>(PASSWORD_FIELD)
                .password()
                .label("New password")
                .help("Leave blank to keep the current password."),
            Field::text_input::<String>(CONFIRMATION_FIELD)
                .password()
                .label("Confirm new password"),
            Field::text_input::<String>(CURRENT_PASSWORD_FIELD)
                .password()
                .label("Current password")
                .help("Required to change your email or password."),
        ))
    }

    fn parse(_cx: &Cx, values: &FormValues) -> Result<Self, Vec<FieldError>> {
        let mut errors = Vec::new();
        let mut keep = |read: Result<String, FieldError>| read.map_err(|error| errors.push(error));
        let name = keep(parse_scalar::<String>("name", values, None));
        let email = keep(parse_scalar::<String>("email", values, None));
        // A blank password is no password; any other keeps every character it was typed with.
        let mut optional = |key| {
            keep(parse_password(key, values, Some(String::new())))
                .ok()
                .filter(|password| !password.trim().is_empty())
        };
        let password = optional(PASSWORD_FIELD);
        let password_confirmation = optional(CONFIRMATION_FIELD).unwrap_or_default();
        let current_password = optional(CURRENT_PASSWORD_FIELD);
        match (name, email) {
            (Ok(name), Ok(email)) if errors.is_empty() => Ok(Self {
                name,
                email,
                password,
                password_confirmation,
                current_password,
            }),
            _ => Err(errors),
        }
    }

    fn write(&self, out: &mut FormValues) {
        out.insert("name".to_string(), self.name.clone());
        out.insert("email".to_string(), self.email.clone());
    }
}

/// The key [`Profile`]'s new password posts under.
const PASSWORD_FIELD: &str = "password";
/// The key [`Profile`]'s repeated new password posts under.
const CONFIRMATION_FIELD: &str = "password_confirmation";
/// The key [`Profile`]'s current password posts under.
pub(crate) const CURRENT_PASSWORD_FIELD: &str = "current_password";

/// The keys a profile POST carries besides its input, which no input field may post.
pub(crate) const RESERVED_KEYS: [&str; 1] = [crate::csrf::FIELD_NAME];

/// Where the panel's profile page lives: `{prefix}/profile`.
pub(crate) fn profile_url(cx: &Cx) -> String {
    format!("{}/profile", panel_prefix(cx))
}

/// Whether the request's panel installs a [`ProfileEditor`].
pub(crate) fn offered(cx: &Cx) -> bool {
    current(cx).is_some_and(|panel| panel.auth.profile_editor().is_some())
}

/// The signed-in user and the panel's editor, or the answer to a request without them.
fn editing(cx: &Cx) -> topcoat::Result<(&SignedIn, &dyn DynProfileEditor)> {
    let signed = signed(cx).ok_or_else(|| unauthenticated_error(cx))?;
    let editor = signed.panel.auth.profile_editor().ok_or_else(not_found)?;
    Ok((signed, editor))
}

/// `GET {prefix}/profile` — the profile form, filled from the signed-in account.
pub(crate) fn profile_page(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let (signed, editor) = editing(cx)?;
        let values = editor
            .values(cx, signed.user.as_ref())
            .ok_or_else(forbidden)?;
        crate::csrf::ensure_token(cx);
        render(cx, editor, &values, &FieldErrors::new()).await
    })
}

/// `POST {prefix}/profile` — save the profile, then land back on the page.
pub(crate) fn profile_post(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let (signed, editor) = editing(cx)?;
        let form = crate::panel::parse_form_body(cx, body).await?;
        crate::csrf::verify(cx, form.last(crate::csrf::FIELD_NAME))?;
        let values = editor.schema().read_input(&form.pairs, &RESERVED_KEYS)?;
        let user_id = signed.user.user_id();
        let client = client_ip(cx);
        // A stolen session must not try passwords against the current-password check at will.
        let throttle = signed.panel.auth.throttle_profile();
        if !throttle.attempt(&user_id, client) {
            tracing::warn!(panel = %signed.panel.prefix, client = ?client, "profile save throttled");
            return Err(too_many_requests(throttle.window_secs()).into());
        }
        let update = match editor.submit(cx, signed.user.as_ref(), values).await? {
            Outcome::Saved(update) => update,
            Outcome::Refused { values, errors } => {
                return render(cx, editor, &values, &errors).await;
            }
        };
        if update == ProfileUpdate::Credentials {
            // Every session the old credential opened ends; this browser gets a new one, as a
            // login would issue.
            super::revoke_sessions_for_user(cx, &user_id).await?;
            let session = session::start(cx).await?;
            record(cx, &session, signed.user.as_ref(), &signed.panel).await?;
        }
        set_notification(cx, Notification::success("Profile saved"));
        Err(see_other(profile_url(cx)).into())
    })
}

/// The profile page in the panel's shell: the editor's form with `values` and `errors`, saved to
/// the page itself, cancelled to the panel's home.
async fn render<'a>(
    cx: &'a Cx,
    editor: &dyn DynProfileEditor,
    values: &FormValues,
    errors: &FieldErrors,
) -> topcoat::Result<BoxView<'a>> {
    render_form_page(
        cx,
        &editor.schema(),
        FormChrome::profile(panel_root(cx)),
        values,
        errors,
        &HashSet::new(),
    )
    .await
}

#[cfg(test)]
mod tests;
