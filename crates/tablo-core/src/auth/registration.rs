//! The panel's sign-up page: the [`Registrar`] trait that creates accounts, the shipped
//! [`SignUp`] form, and the `{prefix}/register` routes.

use std::{any::TypeId, collections::HashMap, future::Future, pin::Pin, sync::Arc};

use topcoat::{
    context::Cx,
    router::{
        Body, RouteFuture,
        request::client_ip,
        response::{IntoResponse, Response},
    },
    session,
    view::{BoxView, ViewExt},
};

use super::{
    PanelUser, infrastructure_failure,
    login::{NEXT_FIELD, login_url, next_from_query, safe_next},
    panel_root,
    session::{delete_session, record},
};
use crate::{
    ActionInput,
    form::{FieldError, FieldErrors, parse_password, parse_scalar},
    notification::{Notification, set_notification},
    panel::{Panel, panel_prefix, state::current},
    schema::{Field, Schema, Source},
};

/// How a panel creates accounts from its sign-up page, installed with
/// [`Auth::registration`](super::Auth::registration).
///
/// It mirrors an [`Action`](crate::Action) with input: [`Input`](Self::Input) is the form the
/// page asks for, [`validate`](Self::validate) refuses a submission field by field, and
/// [`register`](Self::register) writes the account. Both run in one transaction the framework
/// opens, so a uniqueness probe in `validate` sees what `register` writes against.
///
/// The shipped [`PasswordAuth`](super::PasswordAuth) registers an
/// [`AdminUser`](super::AdminUser) from a [`SignUp`]; an app with its own user table implements
/// it beside its [`Authenticator`](super::Authenticator), for the same user type:
///
/// ```rust
/// # use tablo_core::{FieldErrors, PanelUser, auth::{Registrar, SignUp, hash_password}};
/// # use topcoat::context::Cx;
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Staff {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     #[unique] email: String,
/// #     password_hash: String,
/// #     name: String,
/// # }
/// # impl PanelUser for Staff {
/// #     fn user_id(&self) -> String { self.id.to_string() }
/// #     fn display_name(&self) -> &str { &self.name }
/// # }
/// struct StaffAuth;
///
/// impl Registrar for StaffAuth {
///     type Input = SignUp;
///     type User = Staff;
///
///     async fn validate(
///         &self,
///         _cx: &Cx,
///         sign_up: &SignUp,
///         ex: &mut dyn toasty::Executor,
///     ) -> topcoat::Result<FieldErrors> {
///         let mut errors = sign_up.password_errors();
///         let taken = Staff::filter_by_email(&sign_up.email)
///             .first()
///             .exec(ex)
///             .await?;
///         if taken.is_some() {
///             errors.add("email", "Email has already been taken");
///         }
///         Ok(errors)
///     }
///
///     async fn register(
///         &self,
///         _cx: &Cx,
///         sign_up: SignUp,
///         ex: &mut dyn toasty::Executor,
///     ) -> topcoat::Result<Staff> {
///         Ok(toasty::create!(Staff {
///             email: sign_up.email,
///             password_hash: hash_password(&sign_up.password)?,
///             name: sign_up.name,
///         })
///         .exec(ex)
///         .await?)
///     }
/// }
/// ```
///
/// A new account that [may access the panel](PanelUser::can_access_panel) is signed in and lands
/// on the panel; one that may not, such as an account awaiting approval, lands on the login page
/// with a notification saying it was created.
pub trait Registrar: Send + Sync + 'static {
    /// What the sign-up page asks for: [`SignUp`], or the app's own
    /// [`#[derive(ActionInput)]`](derive@crate::ActionInput) struct. `#[form(password)]` masks a
    /// field, and `#[form(email)]` checks an address.
    type Input: ActionInput;

    /// The account a sign-up creates: the type the panel's
    /// [`Authenticator`](super::Authenticator) loads, which mounting checks.
    type User: PanelUser;

    /// Refuse a submission [`register`](Self::register) should not receive, such as an email
    /// another account holds: each error names an input field's key and renders under its
    /// control. Defaults to none.
    ///
    /// It runs inside the sign-up's transaction `ex`, after the input parses and its controls'
    /// own rules pass.
    fn validate(
        &self,
        _cx: &Cx,
        _input: &Self::Input,
        _ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = topcoat::Result<FieldErrors>> + Send {
        async { Ok(FieldErrors::new()) }
    }

    /// Create the account from a validated `input` through the transaction `ex`, which commits
    /// when it returns `Ok`.
    fn register(
        &self,
        cx: &Cx,
        input: Self::Input,
        ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = topcoat::Result<Self::User>> + Send;
}

/// What a sign-up submission came to.
pub(crate) enum Outcome {
    /// The committed account.
    Registered(Arc<dyn PanelUser>),
    /// The refused submission, its values, and why. The page renders a password control empty.
    Refused {
        values: HashMap<String, String>,
        errors: FieldErrors,
    },
}

/// The boxed future the erased registrar returns.
type OutcomeFuture<'a> = Pin<Box<dyn Future<Output = topcoat::Result<Outcome>> + Send + 'a>>;

/// [`Registrar`] with its input and user types erased, as the panel keeps it.
pub(crate) trait DynRegistrar: Send + Sync {
    /// The sign-up form.
    fn schema(&self) -> Schema;
    /// Parses, checks, validates and registers one submission.
    fn sign_up<'a>(&'a self, cx: &'a Cx, values: HashMap<String, String>) -> OutcomeFuture<'a>;
    /// The [`TypeId`] and name of [`Registrar::User`], which mounting compares with the
    /// authenticator's.
    fn user_type(&self) -> (TypeId, &'static str);
}

impl<R: Registrar> DynRegistrar for R {
    fn schema(&self) -> Schema {
        R::Input::schema()
    }

    fn sign_up<'a>(&'a self, cx: &'a Cx, values: HashMap<String, String>) -> OutcomeFuture<'a> {
        Box::pin(async move {
            let schema = R::Input::schema();
            let mut errors = FieldErrors::new();
            let parsed = match R::Input::parse(cx, &values) {
                Ok(parsed) => Some(parsed),
                Err(refused) => {
                    for error in refused {
                        errors.push(error);
                    }
                    None
                }
            };
            schema
                .check_controls(cx, &values, &HashMap::new(), &mut errors)
                .await;
            let refused = |errors| Outcome::Refused {
                values: values.clone(),
                errors,
            };
            let Some(input) = parsed.filter(|_| errors.is_empty()) else {
                return Ok(refused(errors));
            };
            let mut db = crate::db::db(cx);
            let mut tx = db.transaction().await.map_err(infrastructure_failure)?;
            let errors = self.validate(cx, &input, &mut tx).await?;
            if !errors.is_empty() {
                return Ok(refused(errors));
            }
            let user = self.register(cx, input, &mut tx).await?;
            tx.commit().await.map_err(infrastructure_failure)?;
            Ok(Outcome::Registered(Arc::new(user)))
        })
    }

    fn user_type(&self) -> (TypeId, &'static str) {
        (TypeId::of::<R::User>(), std::any::type_name::<R::User>())
    }
}

/// The sign-up form [`PasswordAuth`](super::PasswordAuth) registers from, and a custom
/// [`Registrar`] can take as its input: a name, an email address, and a new password typed
/// twice.
#[derive(Clone, PartialEq, Eq)]
pub struct SignUp {
    /// The display name, posted as `name`.
    pub name: String,
    /// The email address the account signs in with, posted as `email`.
    pub email: String,
    /// The new password, posted as `password`.
    pub password: String,
    /// The password typed again, posted as `password_confirmation`.
    pub password_confirmation: String,
}

impl SignUp {
    /// The refusals [`new_password_errors`] makes of this sign-up's passwords.
    pub fn password_errors(&self) -> FieldErrors {
        new_password_errors(&self.password, &self.password_confirmation)
    }
}

/// Redacts the passwords, so a sign-up logged with `?` writes none to the log.
impl std::fmt::Debug for SignUp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignUp")
            .field("name", &self.name)
            .field("email", &self.email)
            .field("password", &"<redacted>")
            .field("password_confirmation", &"<redacted>")
            .finish()
    }
}

impl ActionInput for SignUp {
    fn schema() -> Schema {
        let field = crate::resource::required_input;
        Schema::new((
            field(Field::text_input::<String>("name").label("Name")),
            field(Field::text_input::<String>("email").email().label("Email")),
            field(
                Field::text_input::<String>(PASSWORD_FIELD)
                    .password()
                    .label("Password"),
            ),
            field(
                Field::text_input::<String>(CONFIRMATION_FIELD)
                    .password()
                    .label("Confirm password"),
            ),
        ))
    }

    fn parse(_cx: &Cx, values: &HashMap<String, String>) -> Result<Self, Vec<FieldError>> {
        let mut errors = Vec::new();
        type Parse =
            fn(&str, &HashMap<String, String>, Option<String>) -> Result<String, FieldError>;
        let mut read = |key: &str, parse: Parse| {
            parse(key, values, None)
                .map_err(|error| errors.push(error))
                .ok()
        };
        let name = read("name", parse_scalar::<String>);
        let email = read("email", parse_scalar::<String>);
        let password = read(PASSWORD_FIELD, parse_password);
        let confirmation = read(CONFIRMATION_FIELD, parse_password);
        match (name, email, password, confirmation) {
            (Some(name), Some(email), Some(password), Some(password_confirmation))
                if errors.is_empty() =>
            {
                Ok(Self {
                    name,
                    email,
                    password,
                    password_confirmation,
                })
            }
            _ => Err(errors),
        }
    }
}

/// The key [`SignUp`]'s new password posts under.
const PASSWORD_FIELD: &str = "password";
/// The key [`SignUp`]'s repeated password posts under.
const CONFIRMATION_FIELD: &str = "password_confirmation";

/// The fewest characters a new password holds, as NIST SP 800-63B asks of a password a user
/// chooses.
pub const MIN_PASSWORD_CHARS: usize = 8;

/// Refuses a new `password` shorter than [`MIN_PASSWORD_CHARS`] under the `password` key, and a
/// `confirmation` that differs from it under `password_confirmation`: the checks a sign-up form
/// with [`SignUp`]'s password keys runs in [`Registrar::validate`].
#[must_use]
pub fn new_password_errors(password: &str, confirmation: &str) -> FieldErrors {
    let mut errors = FieldErrors::new();
    if password.chars().count() < MIN_PASSWORD_CHARS {
        errors.add(
            PASSWORD_FIELD,
            format!("Password must be at least {MIN_PASSWORD_CHARS} characters"),
        );
    }
    if password != confirmation {
        errors.add(CONFIRMATION_FIELD, "Passwords do not match");
    }
    errors
}

/// The keys a sign-up POST carries besides its input, which no input field may post.
pub(crate) const RESERVED_KEYS: [&str; 2] = [crate::csrf::FIELD_NAME, NEXT_FIELD];

/// Where the panel's sign-up page lives: `{prefix}/register`.
pub(crate) fn register_url(cx: &Cx) -> String {
    format!("{}/register", panel_prefix(cx))
}

/// `url` with `next` in its query: how the login and sign-up pages carry it to each other.
pub(crate) fn with_next(url: String, next: &str) -> String {
    if next.is_empty() {
        return url;
    }
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    serializer.append_pair(NEXT_FIELD, next);
    format!("{url}?{}", serializer.finish())
}

/// Whether the request's panel installs a [`Registrar`].
pub(crate) fn offered(cx: &Cx) -> bool {
    current(cx).is_some_and(|panel| panel.auth.registrar().is_some())
}

/// `GET {prefix}/register` — the standalone sign-up page.
pub(crate) fn register_page(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let next = next_from_query(cx).unwrap_or_default();
        page_response(cx, &HashMap::new(), &FieldErrors::new(), next, None).await
    })
}

/// `POST {prefix}/register` — create the account, then sign it in or send it to the login page.
pub(crate) fn register_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let panel = current(cx).ok_or_else(topcoat::router::error::not_found)?;
        let registrar = panel
            .auth
            .registrar()
            .ok_or_else(topcoat::router::error::not_found)?;
        let form = crate::panel::parse_form_body(cx, body).await?;
        crate::csrf::verify(cx, &form.values)?;
        let next = form
            .values
            .get(NEXT_FIELD)
            .and_then(|value| safe_next(value))
            .map(str::to_string)
            .unwrap_or_default();
        let schema = registrar.schema();
        let values = schema.read_input(&form.values, &form.lists, &RESERVED_KEYS)?;
        let client = client_ip(cx);
        if !panel.auth.throttle_sign_ups().attempt("", client) {
            tracing::warn!(panel = %panel.prefix, client = ?client, "sign-up attempt throttled");
            return page_response(
                cx,
                &values,
                &FieldErrors::new(),
                next,
                Some(Alert::Throttled),
            )
            .await;
        }
        let user = match registrar.sign_up(cx, values).await {
            Ok(Outcome::Registered(user)) => user,
            Ok(Outcome::Refused { values, errors }) => {
                return page_response(cx, &values, &errors, next, None).await;
            }
            Err(error) => {
                let error = infrastructure_failure(error);
                if crate::error::TabloError::is_infrastructure(&error) {
                    return unavailable(cx, next).await;
                }
                return Err(error);
            }
        };
        if !user.can_access_panel() {
            set_notification(
                cx,
                Notification::success("Account created")
                    .description("You can sign in once it is approved."),
            );
            return topcoat::router::error::see_other(login_url(cx)).into_response(cx);
        }
        // Rotate as a login does, so a session presented before the sign-up cannot be replayed.
        if let Some(hash) = session::token_hash(cx).await? {
            delete_session(cx, &hash).await?;
        }
        let session = session::start(cx).await?;
        if let Err(error) = record(cx, &session, &*user, panel).await {
            // The account is committed: send it to sign in rather than to a sign-up whose retry
            // would find its email taken.
            tracing::error!(error = %infrastructure_failure(error), "sign-up session not recorded");
            set_notification(
                cx,
                Notification::success("Account created").description("Sign in to continue."),
            );
            return topcoat::router::error::see_other(login_url(cx)).into_response(cx);
        }
        let target = if next.is_empty() {
            panel_root(cx)
        } else {
            next
        };
        topcoat::router::error::see_other(target).into_response(cx)
    })
}

/// The sign-up page with the outage alert.
async fn unavailable(cx: &Cx, next: String) -> topcoat::Result<Response> {
    let values = HashMap::new();
    page_response(
        cx,
        &values,
        &FieldErrors::new(),
        next,
        Some(Alert::Unavailable),
    )
    .await
}

/// What the sign-up page says above its form, besides a field's error.
#[derive(Debug, Clone, Copy)]
enum Alert {
    /// The client is past its sign-up limit; answers 429.
    Throttled,
    /// The database behind sign-up could not answer; answers 503.
    Unavailable,
}

impl Alert {
    fn message(self) -> &'static str {
        match self {
            Self::Throttled => "Too many sign-up attempts. Try again shortly.",
            Self::Unavailable => "Sign-up is unavailable right now. Try again shortly.",
        }
    }

    fn status(self) -> http::StatusCode {
        match self {
            Self::Throttled => http::StatusCode::TOO_MANY_REQUESTS,
            Self::Unavailable => http::StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

/// The sign-up page: 422 for a refused submission, the alert's status with one, else 200.
async fn page_response(
    cx: &Cx,
    values: &HashMap<String, String>,
    errors: &FieldErrors,
    next: String,
    alert: Option<Alert>,
) -> topcoat::Result<Response> {
    let status = match alert {
        Some(alert) => Some(alert.status()),
        None => (!errors.is_empty()).then_some(http::StatusCode::UNPROCESSABLE_ENTITY),
    };
    let page = render_page(cx, values, errors, next, alert, status).await?;
    page.single().await?.into_response(cx)
}

/// Renders the standalone sign-up document: the brand, the registrar's form with its errors,
/// and a link to the login page.
async fn render_page<'a>(
    cx: &'a Cx,
    values: &HashMap<String, String>,
    errors: &FieldErrors,
    next: String,
    alert: Option<Alert>,
    status: Option<http::StatusCode>,
) -> topcoat::Result<BoxView<'a>> {
    let schema = current(cx)
        .and_then(|panel| panel.auth.registrar())
        .map_or_else(Schema::empty, DynRegistrar::schema);
    let csrf = crate::csrf::ensure_token(cx);
    let action = register_url(cx);
    let login = with_next(login_url(cx), &next);
    let brand = Panel::render_brand(cx).await?;
    let fields = schema.render(cx, Source::form(values, errors)).await?;
    let body = topcoat::view::view! {
        cx =>
        <div class="flex min-h-svh items-center justify-center bg-muted p-6">
            <div
                class="flex w-full max-w-sm flex-col gap-6 rounded-xl border border-border bg-card p-6 text-card-foreground shadow-sm"
            >
                if let Some(status) = status {
                    (status)
                }
                <div class="flex flex-col items-center gap-2">
                    (brand)
                    <h1 class="text-lg font-semibold text-foreground">
                        "Create an account"
                    </h1>
                </div>
                <form method="post" action=(action) class="flex flex-col gap-4">
                    (crate::csrf::field(cx, &csrf))
                    <input type="hidden" name=(NEXT_FIELD) value=(next)>
                    if let Some(alert) = alert {
                        tablo_ui::alert(
                            variant: tablo_ui::AlertVariant::Destructive,
                            attrs: topcoat::view::attributes! { role="alert" },
                            tablo_ui::alert_title((alert.message()))
                        )
                    }
                    (fields)
                    tablo_ui::button(
                        variant: tablo_ui::ButtonVariant::Primary,
                        attrs: topcoat::view::attributes! { type="submit" class="w-full" },
                        "Sign up"
                    )
                </form>
                <p class="text-center text-sm text-muted-foreground">
                    "Already have an account? "
                    <a
                        href=(login)
                        class="font-medium text-foreground underline underline-offset-4"
                    >
                        "Sign in"
                    </a>
                </p>
            </div>
        </div>
    }
    .boxed();
    Panel::render_document(cx, "Sign up".to_string(), body).await
}

#[cfg(test)]
mod tests;
