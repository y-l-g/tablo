//! [`ProviderIdentity`], who a provider says signed in, and the [`IdentityLinker`] an app turns
//! it into its user with.

use std::{future::Future, sync::Arc};

use topcoat::context::Cx;

use super::super::{PanelUser, infrastructure_failure};
use crate::BoxFuture;

/// Who an OpenID Connect provider says signed in, once the panel has checked its ID token: the
/// token came from the provider's token endpoint, names the panel's client, has not expired and
/// carries the nonce this sign-in sent.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProviderIdentity {
    /// The provider's key, such as `google`.
    pub provider: String,
    /// The provider's issuer, such as `https://accounts.google.com`.
    pub issuer: String,
    /// The provider's id for the account (`sub`): with the issuer, the one stable name of the
    /// account. An email address can change hands; this cannot.
    pub subject: String,
    /// The account's email address, as the provider has it.
    pub email: Option<String>,
    /// Whether the provider says the account proved it reads `email`.
    pub email_verified: bool,
    /// The account's display name.
    pub name: Option<String>,
    /// Every claim of the ID token, such as Google's `hd`, the Workspace domain of the account:
    /// what a linker reads that the fields above do not carry.
    pub claims: serde_json::Map<String, serde_json::Value>,
}

impl ProviderIdentity {
    /// The account `subject` at the provider keyed `provider` whose issuer is `issuer`, with no
    /// address, name or other claim: for a test of an [`IdentityLinker`], which adds them with
    /// [`with_email`](Self::with_email), [`with_name`](Self::with_name) and
    /// [`with_claim`](Self::with_claim).
    #[must_use]
    pub fn new(
        provider: impl Into<String>,
        issuer: impl Into<String>,
        subject: impl Into<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            issuer: issuer.into(),
            subject: subject.into(),
            email: None,
            email_verified: false,
            name: None,
            claims: serde_json::Map::new(),
        }
    }

    /// This identity holding `email`, which the provider `verified` or not.
    #[must_use]
    pub fn with_email(mut self, email: impl Into<String>, verified: bool) -> Self {
        self.email = Some(email.into());
        self.email_verified = verified;
        self
    }

    /// This identity named `name`.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// This identity carrying the claim `name` with `value`.
    #[must_use]
    pub fn with_claim(
        mut self,
        name: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.claims.insert(name.into(), value.into());
        self
    }
}

/// How a panel turns a [`ProviderIdentity`] into one of the app's users, installed with
/// [`Auth::oidc`](crate::Auth::oidc) through an [`Oidc`](super::Oidc).
///
/// The app stores which of its users each identity is, as a row keyed by
/// [`issuer`](ProviderIdentity::issuer) and [`subject`](ProviderIdentity::subject). A sign-in
/// whose identity no row names may link it to an existing account, or create one, as the app
/// decides; linking by email alone hands that account to whoever controls the address at the
/// provider, so link only an address the provider [verified](ProviderIdentity::email_verified).
///
/// ```rust
/// # use tablo_core::{IdentityLinker, PanelUser, ProviderIdentity};
/// # use topcoat::context::Cx;
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Staff {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     #[unique] email: String,
/// #     name: String,
/// # }
/// # impl PanelUser for Staff {
/// #     fn user_id(&self) -> String { self.id.to_string() }
/// #     fn display_name(&self) -> &str { &self.name }
/// #     fn can_access_panel(&self, _panel: &str) -> bool { true }
/// # }
/// #[derive(Debug, Clone, toasty::Model)]
/// struct StaffIdentity {
///     #[key]
///     #[auto]
///     id: uuid::Uuid,
///     #[index]
///     staff_id: uuid::Uuid,
///     issuer: String,
///     subject: String,
/// }
///
/// struct StaffLinker;
///
/// impl IdentityLinker for StaffLinker {
///     type User = Staff;
///
///     async fn sign_in(
///         &self,
///         _cx: &Cx,
///         identity: &ProviderIdentity,
///         ex: &mut dyn toasty::Executor,
///     ) -> topcoat::Result<Option<Staff>> {
///         let linked = StaffIdentity::filter(
///             StaffIdentity::fields()
///                 .issuer()
///                 .eq(identity.issuer.clone())
///                 .and(
///                     StaffIdentity::fields()
///                         .subject()
///                         .eq(identity.subject.clone()),
///                 ),
///         )
///         .first()
///         .exec(&mut *ex)
///         .await?;
///         if let Some(linked) = linked {
///             return Ok(Staff::filter_by_id(linked.staff_id)
///                 .first()
///                 .exec(ex)
///                 .await?);
///         }
///         // Link an existing account only by an address the provider verified.
///         let (Some(email), true) = (&identity.email, identity.email_verified) else {
///             return Ok(None);
///         };
///         let Some(staff) = Staff::filter_by_email(email).first().exec(&mut *ex).await? else {
///             return Ok(None);
///         };
///         toasty::create!(StaffIdentity {
///             staff_id: staff.id,
///             issuer: identity.issuer.clone(),
///             subject: identity.subject.clone(),
///         })
///         .exec(ex)
///         .await?;
///         Ok(Some(staff))
///     }
/// }
/// ```
pub trait IdentityLinker: Send + Sync + 'static {
    /// The account an identity signs in as: the type the panel's
    /// [`Authenticator`](crate::auth::Authenticator) loads, which `Auth<U>` checks.
    type User: PanelUser;

    /// The user `identity` signs in as, linking or creating one as the app decides, through the
    /// sign-in's transaction `ex`, which commits when it returns `Ok`. `None` refuses the sign-in,
    /// which reads as a failed one. A user who may not
    /// [access the panel](PanelUser::can_access_panel) is refused too.
    fn sign_in(
        &self,
        cx: &Cx,
        identity: &ProviderIdentity,
        ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = topcoat::Result<Option<Self::User>>> + Send;
}

/// [`IdentityLinker`] with its user type erased, as the panel keeps it.
pub(super) trait DynLinker: Send + Sync {
    fn sign_in<'a>(
        &'a self,
        cx: &'a Cx,
        identity: &'a ProviderIdentity,
    ) -> BoxFuture<'a, topcoat::Result<Option<Arc<dyn PanelUser>>>>;
}

impl<L: IdentityLinker> DynLinker for L {
    fn sign_in<'a>(
        &'a self,
        cx: &'a Cx,
        identity: &'a ProviderIdentity,
    ) -> BoxFuture<'a, topcoat::Result<Option<Arc<dyn PanelUser>>>> {
        Box::pin(async move {
            let mut db = crate::db::db(cx)?;
            let mut tx = db.transaction().await.map_err(infrastructure_failure)?;
            let user = IdentityLinker::sign_in(self, cx, identity, &mut tx).await?;
            tx.commit().await.map_err(infrastructure_failure)?;
            Ok(user.map(|user| Arc::new(user) as Arc<dyn PanelUser>))
        })
    }
}
