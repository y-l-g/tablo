//! `Panel` — an admin panel an app mounts into its router.

mod actions;
mod build;
mod detail;
mod forms;
pub(crate) mod gate;
mod headers;
mod list;
mod pages;
mod register;
mod relations;
mod shell;
pub(crate) mod state;
#[cfg(test)]
pub(crate) mod test_support;
pub mod url;
mod write;

use std::path::PathBuf;

use topcoat::{asset::Asset, font::Font, router::LayoutRenderFn};

pub use self::{build::RouterBuilderPanelExt, gate::can_list, shell::Brand};
use self::{
    build::is_directory_pattern,
    register::{PageRegistration, Registration, ResourceRegistration},
    shell::ShellAssets,
};
pub(crate) use self::{build::route_path, forms::parse_form_body, gate::panel_prefix};
use crate::{
    DeclarationError, DeclarationErrorKind, Page,
    resource::{Resource, ResourceDef},
};

/// Returns the panel's list table for `R`, with the request's row actions wired on, for a page
/// that owns its table; render it over the page's own query with
/// [`WiredTable::render`](crate::table::WiredTable::render).
///
/// # Errors
///
/// A declaration error when the request's panel does not mount `R`.
pub fn wired_table<R: Resource>(
    cx: &topcoat::context::Cx,
) -> topcoat::Result<crate::table::WiredTable<R::Model>> {
    let resource = crate::resource::require_mounted::<R>(cx)?;
    Ok(self::list::wire_table_actions(cx, &resource))
}

/// `R`'s sidebar entry in the request's panel, with its label, URL and icon; `None` when the
/// panel does not register `R`.
pub fn navigation<R: Resource>(cx: &topcoat::context::Cx) -> Option<crate::NavigationItem> {
    // No panel at all means no sidebar entry; `mounted()` falls back to `R::declare()` for loaders.
    topcoat::context::try_app_context::<crate::resource::MountScope>(cx)?;
    crate::resource::mounted::<R>(cx).map(|resource| resource.navigation.clone())
}

/// An admin panel: resources and pages under one prefix, framed by one shell
/// and gated by one [`Auth`](crate::auth::Auth).
///
/// The app owns the router and mounts the panel into it with
/// [`RouterBuilderPanelExt::panel`]:
///
/// ```text
/// let router = Router::builder()
///     .discover()
///     .app_context(db)
///     .panel(Panel::new("admin").resource::<UserResource>())?
///     .build();
/// ```
///
/// Registering is declarative: mounting builds each resource's [`ResourceDef`] and checks every
/// slug, route and declaration before the panel serves anything.
pub struct Panel {
    prefix: String,
    shell_assets: Option<ShellAssets>,
    brand: Option<Brand>,
    dark_mode: Option<bool>,
    /// Frames the panel's pages; `None` renders the shipped shell.
    layout: Option<LayoutRenderFn>,
    /// The resources and pages, in registration order.
    registrations: Vec<Box<dyn Registration>>,
    /// `Content-Security-Policy: frame-ancestors …` for every response under the prefix; `None`
    /// opts out, the default is `'self'`.
    frame_ancestors: Option<String>,
    /// Mistakes in the panel's own configuration, for mount to report.
    configuration_errors: Vec<DeclarationError>,
    /// Where file field bytes go; `None` stores the sanitized basename.
    uploads: Option<crate::upload::InstalledUploader>,
    /// App-owned filesystem directories served with hardening headers.
    served_dirs: Vec<(String, PathBuf)>,
    login_hint: Option<String>,
    auth: crate::auth::Auth,
}

impl Panel {
    /// Creates a `Panel` mounted at `prefix`, defaulting an empty prefix to `"/admin"`.
    pub fn new(prefix: impl Into<String>) -> Self {
        let raw = prefix.into();
        let trimmed = raw.trim().trim_matches('/').to_string();
        let prefix = if trimmed.is_empty() {
            "/admin".to_string()
        } else {
            format!("/{trimmed}")
        };
        let configuration_errors = prefix
            .trim_matches('/')
            .split('/')
            .filter_map(|segment| build::validate_route_segment("panel prefix", segment).err())
            .map(DeclarationError::panel)
            .collect();
        Self {
            prefix,
            shell_assets: None,
            brand: None,
            dark_mode: None,
            layout: None,
            registrations: Vec::new(),
            frame_ancestors: Some(headers::DEFAULT_FRAME_ANCESTORS.to_string()),
            configuration_errors,
            uploads: None,
            served_dirs: Vec::new(),
            login_hint: None,
            auth: crate::auth::Auth::default(),
        }
    }

    /// Returns the mount prefix, e.g. `"/admin"`.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Installs the [`Uploader`](crate::Uploader) this panel's file fields store through; without
    /// one a file field stores the sanitized client filename.
    pub fn uploads(mut self, uploader: impl crate::Uploader) -> Self {
        self.uploads = Some(crate::upload::InstalledUploader::new(uploader));
        self
    }

    /// Serves the directory `dir` at route pattern `path`, which must end in a catch-all and sits
    /// outside the auth gate with hardening headers.
    pub fn serve_dir(mut self, path: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        let path = path.into();
        if !is_directory_pattern(&path) {
            self.configuration_errors.push(DeclarationError::panel(
                DeclarationErrorKind::ServeDirWithoutCatchAll { path: path.clone() },
            ));
        }
        self.served_dirs.push((path, dir.into()));
        self
    }

    /// Registers the stylesheet and font the default shell links, resolved through the router's
    /// asset bundle.
    pub fn shell_assets(mut self, stylesheet: Asset, font: Font) -> Self {
        self.shell_assets = Some(ShellAssets { stylesheet, font });
        self
    }

    /// Registers the [`Resource`] `R` at `{prefix}/{slug}` with its routes and sidebar entry, as
    /// [`Resource::declare`] declares it.
    pub fn resource<R: Resource>(self) -> Self {
        self.resource_with::<R>(|def| def)
    }

    /// Registers the [`Resource`] `R` as [`resource`](Self::resource) does, with `customize`
    /// adjusting its def for this panel only:
    ///
    /// ```text
    /// Panel::new("portal").resource_with::<PostResource>(|def| def.policy(ReadOnly))
    /// ```
    pub fn resource_with<R: Resource>(
        mut self,
        customize: impl FnOnce(ResourceDef<R>) -> ResourceDef<R> + Send + 'static,
    ) -> Self {
        self.registrations
            .push(Box::new(ResourceRegistration::<R>(Box::new(customize))));
        self
    }

    /// Declares a [`Page`] at `{prefix}/{slug}` with its sidebar entry; pages share the resources'
    /// slug namespace.
    pub fn page<P: Page>(mut self) -> Self {
        self.registrations.push(Box::new(PageRegistration::<P> {
            home: false,
            _page: std::marker::PhantomData,
        }));
        self
    }

    /// Declares the panel's home page at the panel prefix, replacing the redirect to the first
    /// resource's list.
    pub fn home<P: Page>(mut self) -> Self {
        self.registrations.push(Box::new(PageRegistration::<P> {
            home: true,
            _page: std::marker::PhantomData,
        }));
        self
    }

    /// Frames the panel's pages with `render` instead of the shipped shell; `render` usually wraps
    /// [`Panel::layout_shell`].
    pub fn layout(mut self, render: LayoutRenderFn) -> Self {
        self.layout = Some(render);
        self
    }

    /// Sets branding for the shell (sidebar header, login card, and the topbar below `md`).
    pub fn brand(mut self, brand: Brand) -> Self {
        self.brand = Some(brand);
        self
    }

    /// Sets the `frame-ancestors` directive the panel sends on every response under its prefix,
    /// defaulting to `'self'`.
    pub fn frame_ancestors(mut self, ancestors: impl Into<String>) -> Self {
        self.frame_ancestors = Some(ancestors.into());
        self
    }

    /// Sends no `frame-ancestors` directive for deployments whose proxy owns the whole CSP.
    pub fn without_frame_ancestors(mut self) -> Self {
        self.frame_ancestors = None;
        self
    }

    /// Sets the theme a visitor who has not chosen one sees, dark when `true`.
    pub fn dark_mode(mut self, enabled: bool) -> Self {
        self.dark_mode = Some(enabled);
        self
    }

    /// Configures this panel's authentication, defaulting to password auth; sessions belong to the
    /// panel that signed them in.
    pub fn auth(mut self, auth: crate::auth::Auth) -> Self {
        self.auth = auth;
        self
    }

    /// Renders a line under the login form for demo credentials or deployment hints.
    pub fn login_hint(mut self, hint: impl Into<String>) -> Self {
        self.login_hint = Some(hint.into());
        self
    }
}

/// What the panel serves at its prefix.
enum Root {
    /// A redirect to the first declared resource's list.
    Redirect(String),
    /// The [`Panel::home`] page.
    Home,
}

#[cfg(test)]
mod tests;
