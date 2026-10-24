//! The panel's profile page: the [`ProfileEditor`] trait that edits the signed-in account, the
//! shipped [`Profile`] form, and the `{prefix}/profile` routes.

use std::{
    any::{Any, TypeId},
    future::Future,
};

use topcoat::{
    context::{Cx, try_request_context},
    router::{
        Body,
        error::{forbidden, not_found, see_other, too_many_requests},
        request::client_ip,
    },
    session,
    view::BoxView,
};

use super::{
    PanelUser, SignedIn, email_change::Requested, input, registration::new_password_errors,
    session::replace_sessions, signed, unauthenticated_error,
};
use crate::{
    BoxFuture,
    auth::realm::{SettingsPage, current_realm, hosted, panel_prefix},
    flash::set_notification,
    form::{FieldError, FieldErrors, FormInput, FormValues, parse_password, parse_scalar},
    outcome::Notification,
    schema::{Field, Schema},
    topcoat_compat::async_page,
};

/// How a panel lets the signed-in user edit their own account on its profile page, installed
/// with [`Auth::profile`](super::Auth::profile).
///
/// It mirrors a [`Registrar`](super::Registrar): [`Input`](Self::Input) is the form the page
/// shows, [`fill`](Self::fill) seeds it from the account, [`validate`](Self::validate) refuses a
/// submission field by field, and [`save`](Self::save) writes it. `validate` and `save` run in one
/// transaction the framework opens.
///
/// The shipped [`PasswordAuth`](super::PasswordAuth) edits an [`AdminUser`](super::AdminUser)
/// through a [`Profile`]; an app with its own user table implements it beside its
/// [`Authenticator`](super::Authenticator), for the same user type:
///
/// ```rust
/// # use tablo_core::{PanelUser, ProfileEditor, ProfileUpdate};
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
#[non_exhaustive]
pub enum ProfileUpdate {
    /// Nothing the user signs in with: every session stays signed in.
    Details,
    /// Something the user signs in with, such as a password: the user's sessions and reset links
    /// end on every panel whose authenticator loads the same user type, in the transaction that
    /// saves, and the browser that saved gets a new session under the tenant it acted for.
    Credentials,
}

/// What a profile submission came to.
pub(crate) enum Outcome {
    /// The committed change, its sessions replaced when a credential changed; `email_pending`
    /// when it asked a new address to confirm itself.
    Saved { email_pending: bool },
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
    /// Parses, checks, validates and saves one submission for `signed`'s user, replacing their
    /// sessions in the same transaction when a credential changed.
    fn submit<'a>(
        &'a self,
        cx: &'a Cx,
        signed: &'a SignedIn,
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
        signed: &'a SignedIn,
        mut values: FormValues,
    ) -> BoxFuture<'a, topcoat::Result<Outcome>> {
        Box::pin(async move {
            let any: &dyn Any = &*signed.user;
            let user = any
                .downcast_ref::<E::User>()
                .ok_or_else(|| topcoat::Error::from(forbidden()))?;
            let schema = E::Input::schema();
            let mut stored = FormValues::new();
            self.fill(cx, user).write(&mut stored);
            // A control the page renders disabled posts nothing the user may change: it keeps
            // what `fill` wrote, whatever a crafted POST carries.
            for key in schema.disabled_keys(true) {
                match stored.get(key) {
                    Some(value) => values.insert(key, value.clone()),
                    None => values.remove(key),
                };
            }
            schema.keep_unedited(&mut values, &stored, &crate::time_zone(cx));
            let input = match input::parse::<E::Input>(cx, &values, &stored, "profile").await? {
                Ok(input) => input,
                Err(errors) => return Ok(Outcome::Refused { values, errors }),
            };
            let mut db = crate::db::db(cx)?;
            let mut tx = db.transaction().await.map_err(crate::error::unavailable)?;
            let errors = self
                .validate(cx, user, &input, &mut tx)
                .await
                .map_err(write_failure)?;
            input::placed(&schema, &errors, "the profile editor", "profile")?;
            if !errors.is_empty() {
                return Ok(Outcome::Refused { values, errors });
            }
            let update = self
                .save(cx, user, input, &mut tx)
                .await
                .map_err(write_failure)?;
            if update == ProfileUpdate::Credentials {
                // The new credential and the end of the sessions the old one opened commit
                // together, or neither does. The token only reaches the browser with the response.
                let session = session::start(cx).await?;
                replace_sessions(cx, &mut tx, signed, TypeId::of::<E::User>(), &session).await?;
            }
            tx.commit().await.map_err(crate::error::unavailable)?;
            let email_pending =
                try_request_context::<Requested>(cx).is_some_and(|requested| requested.mail(cx));
            Ok(Outcome::Saved { email_pending })
        })
    }

    fn user_type(&self) -> (TypeId, &'static str) {
        (TypeId::of::<E::User>(), std::any::type_name::<E::User>())
    }
}

/// A failed profile write: a database failure becomes the opaque outage, logged; an editor's own
/// error keeps its mapping.
fn write_failure(error: topcoat::Error) -> topcoat::Error {
    crate::error::driver_failure(error, crate::error::DATABASE_UNAVAILABLE)
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
            required(Field::text_input::<String>("name").label(crate::text!("tablo-auth-name"))),
            required(
                Field::text_input::<String>("email")
                    .email()
                    .label(crate::text!("tablo-auth-email")),
            ),
            Field::text_input::<String>(PASSWORD_FIELD)
                .password()
                .label(crate::text!("tablo-auth-new-password"))
                .help(crate::text!("tablo-profile-password-help")),
            Field::text_input::<String>(CONFIRMATION_FIELD)
                .password()
                .label(crate::text!("tablo-auth-confirm-new-password")),
            Field::text_input::<String>(CURRENT_PASSWORD_FIELD)
                .current_password()
                .label(crate::text!("tablo-auth-current-password"))
                .help(crate::text!("tablo-profile-current-password-help")),
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
    current_realm(cx).is_some_and(|panel| panel.auth.profile_editor().is_some())
}

/// The signed-in user and the panel's editor, or the answer to a request without them.
fn editing(cx: &Cx) -> topcoat::Result<(&SignedIn, &dyn DynProfileEditor)> {
    let signed = signed(cx).ok_or_else(|| unauthenticated_error(cx))?;
    let editor = signed.realm.auth.profile_editor().ok_or_else(not_found)?;
    Ok((signed, editor))
}

/// `GET {prefix}/profile` — the profile form, filled from the signed-in account.
pub(super) fn profile_page(cx: &Cx, _body: Body) -> BoxView<'_> {
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
pub(super) fn profile_post(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let (signed, editor) = editing(cx)?;
        let form = crate::auth::input::read_form(cx, body).await?;
        crate::csrf::verify(cx, form.last(crate::csrf::FIELD_NAME))?;
        let input_schema = editor.schema();
        let values = input_schema.read_input(cx, &form.pairs, &RESERVED_KEYS)?;
        // A stolen session must not try passwords against the current-password check at will.
        // Counted per user alone, from any address: a lockout only holds back profile saves,
        // never a sign-in.
        let throttle = signed.realm.auth.throttle_profile();
        if !throttle.attempt(&signed.user.user_id(), None) {
            tracing::warn!(
                panel = %signed.realm.prefix,
                client = ?client_ip(cx),
                "profile save throttled"
            );
            return Err(too_many_requests(throttle.window_secs()).into());
        }
        // An email change the editor asks to confirm is mailed once the save commits.
        let saving = cx.with(Requested::default());
        let email_pending = match editor.submit(&saving, signed, values).await? {
            Outcome::Refused { values, errors } => {
                return render(cx, editor, &values, &errors).await;
            }
            Outcome::Saved { email_pending } => email_pending,
        };
        let saved = Notification::success(crate::t!(cx, "tablo-profile-saved"));
        set_notification(
            cx,
            if email_pending {
                saved.description(crate::t!(cx, "tablo-profile-email-pending"))
            } else {
                saved
            },
        );
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
    let schema = editor.schema();
    let page = SettingsPage {
        title: crate::t!(cx, "tablo-shell-profile"),
        schema: &schema,
        hidden: None,
        values,
        errors,
    };
    hosted(cx)?.settings_page(cx, page).await
}

/// The mistakes in a panel's [`Auth::profile`](super::Auth::profile): a profile without auth, an
/// editor whose user is not the authenticator's, and a profile form field the page cannot serve.
pub(super) fn declaration_errors(auth: &super::Auth) -> Vec<crate::DeclarationErrorKind> {
    use super::input::FieldFault;
    use crate::{DeclarationErrorKind, declaration::ProfileFault};

    let Some(editor) = auth.profile_editor.as_deref() else {
        return Vec::new();
    };
    let Some(authenticator) = auth.authenticator() else {
        return vec![DeclarationErrorKind::Profile(ProfileFault::AuthDisabled)];
    };
    let mut kinds = Vec::new();
    let (user, user_name) = editor.user_type();
    if user != authenticator.user_type() {
        kinds.push(DeclarationErrorKind::Profile(ProfileFault::UserType {
            editor: user_name,
        }));
    }
    let schema = editor.schema();
    kinds.extend(schema.declaration_errors());
    kinds.extend(schema.empty_choices());
    kinds.extend(
        input::field_faults(&schema, &RESERVED_KEYS).map(|(fault, name)| {
            DeclarationErrorKind::Profile(match fault {
                FieldFault::Reserved => ProfileFault::ReservedField(name),
                FieldFault::File => ProfileFault::FileField(name),
                FieldFault::Relationship => ProfileFault::RelationshipField(name),
            })
        }),
    );
    kinds
}

#[cfg(test)]
mod tests;
