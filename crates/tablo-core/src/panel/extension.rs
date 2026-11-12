//! What a crate that extends panels registers beside the app's own declarations: the hooks auth
//! runs, the models its writes need, and its catalogs.

use super::Panel;
use crate::{AuthHook, auth::hook};

impl Panel {
    /// Runs the [`AuthHook`] `H` inside the transaction of each [`AuthEvent`](crate::AuthEvent)
    /// of the panel. Several hooks run in the order they are registered; registering `H` again
    /// changes nothing, so it never runs twice.
    #[must_use]
    pub fn auth_hook<H: AuthHook>(mut self) -> Self {
        let id = std::any::TypeId::of::<H>();
        if !self.auth_hooks.iter().any(|(hook, _)| *hook == id) {
            self.auth_hooks.push((id, hook::erased::<H>));
        }
        self
    }

    /// Refuses to mount the panel over a `Db` that does not register the model `M`
    /// ([`UnregisteredModel`](crate::DeclarationErrorKind::UnregisteredModel)), for a hook that
    /// writes `M` on every request: without it, each write would fail as it commits.
    #[must_use]
    pub fn require_model<M: toasty::schema::Model>(mut self) -> Self {
        let model = M::schema().name().upper_camel_case();
        if !self.required_models.contains(&model) {
            self.required_models.push(model);
        }
        self
    }

    /// Adds a catalog for `locale` under Tablo's and the app's
    /// [`translations`](Self::translations): a crate that extends panels ships the messages its
    /// [`Text`](crate::Text)s name through it, in every language it has, and rewords none of
    /// Tablo's. Adding the same catalog again changes nothing.
    ///
    /// A catalog for a locale the panel does not offer is left out, where
    /// [`translations`](Self::translations) refuses it: the app picks the locales, the crate
    /// does not. A message both this catalog and the app's hold reads as the app's.
    ///
    /// ```rust,ignore
    /// panel
    ///     .default_translations(locale!("en"), include_str!("../locales/en.ftl"))
    ///     .default_translations(locale!("fr"), include_str!("../locales/fr.ftl"))
    /// ```
    #[must_use]
    pub fn default_translations(
        mut self,
        locale: topcoat::l10n::Locale,
        catalog: impl Into<String>,
    ) -> Self {
        let catalog = (locale, catalog.into());
        if !self.default_translations.contains(&catalog) {
            self.default_translations.push(catalog);
        }
        self
    }
}
