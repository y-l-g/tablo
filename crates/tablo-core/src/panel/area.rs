//! [`Area`]: a part of a panel under its own path segment, with a sidebar of its own, such as the
//! settings of an account and of the tenant it acts for.

use topcoat::icon::IconData;

use super::register::{CustomizePage, PageRegistration, Registration, ResourceRegistration};
use crate::{Page, PageDef, Resource, ResourceDef};

/// A part of a panel under `{prefix}/{slug}` with a sidebar of its own: a settings area, an
/// account area. Registered with [`Panel::area`](crate::Panel::area).
///
/// Outside the area, the panel's sidebar shows the area as one entry, which opens the first of
/// its entries the user may open. Inside it, the sidebar shows a link back to the panel's home
/// and the area's groups, each listing the entries the user may open; a group with none is left
/// out. The topbar's trail starts at the area.
///
/// ```rust
/// # use tablo_core::{Area, NoForm, Panel, Resource};
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
/// # struct Member { #[key] #[auto] id: uuid::Uuid, name: String }
/// # struct MemberResource;
/// # impl Resource for MemberResource { type Model = Member; type Form = NoForm<Member>; }
/// Panel::new("app").area(
///     Area::new("settings")
///         .group("Account", |group| group.profile().two_factor())
///         .tenant_group(|group| group.tenant_profile().resource::<MemberResource>()),
/// );
/// ```
pub struct Area {
    pub(super) slug: String,
    pub(super) label: Option<crate::Text>,
    pub(super) icon: Option<IconData>,
    pub(super) navigation_order: i32,
    pub(super) navigation_group: Option<crate::Text>,
    pub(super) navigation_hidden: bool,
    pub(super) groups: Vec<(AreaGroupLabel, AreaGroup)>,
}

impl Area {
    /// An area at `{prefix}/{slug}`, with no groups yet. `slug` is one URL path segment, taken
    /// from the namespace the panel's resources and pages share.
    pub fn new(slug: impl Into<String>) -> Self {
        Self {
            slug: slug.into(),
            label: None,
            icon: None,
            navigation_order: 0,
            navigation_group: None,
            navigation_hidden: false,
            groups: Vec::new(),
        }
    }

    /// The label of the area's entry in the panel's sidebar and of the first step of its trail.
    ///
    /// Defaults to the message `area-{slug}` of the panel's catalogs, else the slug in sentence
    /// case: `settings` reads `area-settings`, else `Settings`.
    #[must_use]
    pub fn label(mut self, label: impl Into<crate::Text>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The icon of the area's entry in the panel's sidebar.
    #[must_use]
    pub fn icon(mut self, icon: IconData) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Where the area's entry sorts among the panel's sidebar entries, lowest first; defaults to
    /// `0`, as [`PageDef::navigation_order`] does.
    #[must_use]
    pub fn navigation_order(mut self, order: i32) -> Self {
        self.navigation_order = order;
        self
    }

    /// The labelled sidebar group the area's entry renders in; defaults to none, above every
    /// group, as [`PageDef::navigation_group`] does.
    #[must_use]
    pub fn navigation_group(mut self, group: impl Into<crate::Text>) -> Self {
        self.navigation_group = Some(group.into());
        self
    }

    /// Keeps the area's entry out of the panel's sidebar. The area stays mounted: the user menu
    /// and the tenant menu lead into it through the [profile](AreaGroup::profile) and
    /// [tenant settings](AreaGroup::tenant_profile) pages it holds, or a link of the app's own.
    #[must_use]
    pub fn without_navigation(mut self) -> Self {
        self.navigation_hidden = true;
        self
    }

    /// Adds a group headed `label`, after the groups already declared, holding what `build`
    /// registers in it.
    #[must_use]
    pub fn group(
        mut self,
        label: impl Into<crate::Text>,
        build: impl FnOnce(AreaGroup) -> AreaGroup,
    ) -> Self {
        self.groups.push((
            AreaGroupLabel::Text(label.into()),
            build(AreaGroup::default()),
        ));
        self
    }

    /// Adds a group headed by the name of the tenant the request acts for, after the groups
    /// already declared, holding what `build` registers in it: the tenant's settings, members and
    /// billing. A request that acts for no tenant sees none of its entries.
    #[must_use]
    pub fn tenant_group(mut self, build: impl FnOnce(AreaGroup) -> AreaGroup) -> Self {
        self.groups
            .push((AreaGroupLabel::Tenant, build(AreaGroup::default())));
        self
    }
}

impl std::fmt::Debug for Area {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Area")
            .field("slug", &self.slug)
            .field("label", &self.label)
            .field("navigation_order", &self.navigation_order)
            .field("navigation_group", &self.navigation_group)
            .field("navigation_hidden", &self.navigation_hidden)
            .field("groups", &self.groups.len())
            .finish_non_exhaustive()
    }
}

/// What heads a group of an [`Area`].
pub(super) enum AreaGroupLabel {
    /// A label of the app's.
    Text(crate::Text),
    /// The name of the tenant the request acts for.
    Tenant,
}

/// The entries of one group of an [`Area`], in the order they are registered. Each resource and
/// page mounts at `{prefix}/{area}/{slug}`, its slug taken from the area's own namespace, and
/// its def's [`navigation_group`](PageDef::navigation_group) gives way to the group.
#[derive(Default)]
pub struct AreaGroup {
    pub(super) entries: Vec<AreaEntry>,
}

/// One entry of an [`AreaGroup`].
pub(super) enum AreaEntry {
    /// A resource or page of the app's.
    Registration(Box<dyn Registration>),
    /// A page auth serves, adjusted by its def.
    Auth(AuthPage, CustomizePage),
    /// The list of the panel's audit log.
    AuditLog,
}

/// A page auth serves that an area can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthPage {
    /// [`Auth::profile`](crate::Auth::profile)'s page.
    Profile,
    /// [`Auth::two_factor`](crate::Auth::two_factor)'s settings page.
    TwoFactor,
    /// [`Auth::tenant_profile`](crate::Auth::tenant_profile)'s page.
    TenantProfile,
}

impl AreaGroup {
    /// Registers the [`Resource`] `R` in the group, as [`Panel::resource`](crate::Panel::resource)
    /// registers it in the panel.
    #[must_use]
    pub fn resource<R: Resource>(self) -> Self {
        self.resource_with::<R>(|def| def)
    }

    /// Registers the [`Resource`] `R` in the group with `customize` adjusting its def, as
    /// [`Panel::resource_with`](crate::Panel::resource_with) does.
    #[must_use]
    pub fn resource_with<R: Resource>(
        mut self,
        customize: impl FnOnce(ResourceDef<R>) -> ResourceDef<R> + Send + 'static,
    ) -> Self {
        self.entries.push(AreaEntry::Registration(Box::new(
            ResourceRegistration::<R>(Box::new(customize)),
        )));
        self
    }

    /// Declares the [`Page`] `P` in the group, as [`Panel::page`](crate::Panel::page) declares it
    /// in the panel.
    #[must_use]
    pub fn page<P: Page>(self) -> Self {
        self.page_with::<P>(|def| def)
    }

    /// Declares the [`Page`] `P` in the group with `customize` adjusting its def, as
    /// [`Panel::page_with`](crate::Panel::page_with) does.
    #[must_use]
    pub fn page_with<P: Page>(
        mut self,
        customize: impl FnOnce(PageDef) -> PageDef + Send + 'static,
    ) -> Self {
        self.entries
            .push(AreaEntry::Registration(Box::new(PageRegistration::<P> {
                home: false,
                customize: Box::new(customize),
                _page: std::marker::PhantomData,
            })));
        self
    }

    /// Moves the profile page [`Auth::profile`](crate::Auth::profile) installs into the group, at
    /// `{prefix}/{area}/profile`. The user menu's Profile entry follows it there.
    #[must_use]
    pub fn profile(self) -> Self {
        self.profile_with(|def| def)
    }

    /// Moves the profile page into the group as [`profile`](Self::profile) does, with `customize`
    /// adjusting its def: its [`slug`](PageDef::slug), [`label`](PageDef::label),
    /// [`icon`](PageDef::icon), [`navigation_order`](PageDef::navigation_order) and
    /// [`header_action`](PageDef::header_action)s, which render in the form's header and serve
    /// at `{url}/-/actions/{NAME}` as a page's do: deleting the account belongs there.
    ///
    /// Auth renders the page's form and decides who opens it, so mounting refuses a def that
    /// declares widgets, filters or an access check.
    #[must_use]
    pub fn profile_with(self, customize: impl FnOnce(PageDef) -> PageDef + Send + 'static) -> Self {
        self.auth_page(AuthPage::Profile, customize)
    }

    /// Moves the two-factor settings page [`Auth::two_factor`](crate::Auth::two_factor) installs
    /// into the group, at `{prefix}/{area}/two-factor`. The user menu's entry follows it there.
    #[must_use]
    pub fn two_factor(self) -> Self {
        self.two_factor_with(|def| def)
    }

    /// Moves the two-factor settings page into the group as [`two_factor`](Self::two_factor)
    /// does, with `customize` adjusting its def as [`profile_with`](Self::profile_with) reads it,
    /// but for header actions, which mounting refuses: the page renders no form header to hold
    /// them.
    #[must_use]
    pub fn two_factor_with(
        self,
        customize: impl FnOnce(PageDef) -> PageDef + Send + 'static,
    ) -> Self {
        self.auth_page(AuthPage::TwoFactor, customize)
    }

    /// Moves the tenant settings page [`Auth::tenant_profile`](crate::Auth::tenant_profile)
    /// installs into the group, at `{prefix}/{area}/tenant`. The tenant menu's entry follows it
    /// there. The entry shows to a user who may edit the tenant the request acts for.
    #[must_use]
    pub fn tenant_profile(self) -> Self {
        self.tenant_profile_with(|def| def)
    }

    /// Moves the tenant settings page into the group as [`tenant_profile`](Self::tenant_profile)
    /// does, with `customize` adjusting its def as [`profile_with`](Self::profile_with) reads it:
    /// deleting the tenant is a header action of it.
    #[must_use]
    pub fn tenant_profile_with(
        self,
        customize: impl FnOnce(PageDef) -> PageDef + Send + 'static,
    ) -> Self {
        self.auth_page(AuthPage::TenantProfile, customize)
    }

    /// Moves the list of the audit log [`Panel::audit_log`](crate::Panel::audit_log) keeps into
    /// the group, at `{prefix}/{area}/audit-log`, with the def `audit_log` gave it. In a
    /// [`tenant_group`](Area::tenant_group), the list sits beside the tenant's members and billing.
    ///
    /// Mounting refuses an area that places the log when the panel keeps none, and a second group
    /// that places it.
    #[must_use]
    pub fn audit_log(mut self) -> Self {
        self.entries.push(AreaEntry::AuditLog);
        self
    }

    fn auth_page(
        mut self,
        page: AuthPage,
        customize: impl FnOnce(PageDef) -> PageDef + Send + 'static,
    ) -> Self {
        self.entries
            .push(AreaEntry::Auth(page, Box::new(customize)));
        self
    }
}

impl std::fmt::Debug for AreaGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AreaGroup")
            .field("entries", &self.entries.len())
            .finish()
    }
}

/// Where auth serves its settings pages: under `prefix`, but for those `placed` in an area.
pub(super) fn settings_paths(
    prefix: &str,
    placed: &[(AuthPage, String)],
) -> crate::auth::SettingsPaths {
    let mut paths = crate::auth::SettingsPaths::under(prefix);
    for (page, url) in placed {
        let path = match page {
            AuthPage::Profile => &mut paths.profile,
            AuthPage::TwoFactor => &mut paths.two_factor,
            AuthPage::TenantProfile => &mut paths.tenant_profile,
        };
        path.clone_from(url);
    }
    paths
}
