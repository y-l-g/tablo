//! Registering an [`Area`]: its slug, its groups' resources and pages under its URL, the pages
//! auth serves that it holds, and its entry in the panel's sidebar.

use std::{any::TypeId, sync::Arc};

use topcoat::{
    context::Cx,
    router::{Body, RouteFuture, error::redirect},
};

use super::{CustomizePage, Registration, Registry, list_actions_route};
use crate::{
    AreaFault, DeclarationError, DeclarationErrorKind, NavigationItem, Page, PageDef, Text,
    navigation::NavTarget,
    page::{MountedPage, check_page},
    panel::{
        area::{Area, AreaEntry, AreaGroupLabel, AuthPage},
        pages::{page_action, page_action_options},
        state::{AreaNav, AreaSlot, NavEntry, current},
    },
};

/// The area and group the registrations now running land in.
pub(super) struct Scope {
    /// The area's URL, which they mount under.
    pub(super) base: String,
    pub(super) slot: AreaSlot,
    /// The label of the group their sidebar entries render in.
    pub(super) group: Text,
}

/// The group label a tenant group's entries carry until the shell names the tenant.
pub(crate) const TENANT_GROUP: &str = "tablo-shell-tenants";

/// The profile page as the panel's pages know it once it is placed in an area, so its header
/// actions mount, check and render as a page's do. Auth renders its body.
struct ProfileSettings;

impl Page for ProfileSettings {}

/// The tenant settings page as the panel's pages know it, as [`ProfileSettings`] is.
struct TenantSettings;

impl Page for TenantSettings {}

impl Registration for Area {
    fn register(self: Box<Self>, registry: &mut Registry) {
        let Area {
            slug,
            label,
            icon,
            navigation_order,
            navigation_group,
            navigation_hidden,
            groups,
        } = *self;
        let Some(url) = registry.claim("Area::new", &slug, DeclarationError::panel) else {
            return;
        };
        let label = label.unwrap_or_else(|| {
            Text::derived(
                crate::naming::derived_id("area", &slug),
                crate::naming::sentence_case(&slug),
            )
        });
        let area = registry.areas.len();
        registry.areas.push(AreaNav {
            url: url.clone(),
            label: label.clone(),
        });
        registry.route(http::Method::GET, &url, area_root);
        for (group_label, group) in groups {
            let (group_label, tenant) = match group_label {
                AreaGroupLabel::Text(text) => (text, false),
                AreaGroupLabel::Tenant => (Text::message(TENANT_GROUP), true),
            };
            registry.scope = Some(Scope {
                base: url.clone(),
                slot: AreaSlot { area, tenant },
                group: group_label,
            });
            for entry in group.entries {
                match entry {
                    AreaEntry::Registration(registration) => registration.register(registry),
                    AreaEntry::Auth(page, customize) => {
                        registry.register_auth_page(page, customize)
                    }
                }
            }
        }
        registry.scope = None;
        if navigation_hidden {
            return;
        }
        // Like a resource listed first, the area takes the root a sign-in lands on.
        if registry.root.is_none() {
            registry.root = Some(super::Root::Redirect(url.clone()));
        }
        let entries: Vec<_> = registry
            .nav_items
            .iter()
            .filter(|entry| entry.area.is_some_and(|slot| slot.area == area))
            .map(|entry| Arc::clone(&entry.visible))
            .collect();
        let entry = NavEntry {
            item: NavigationItem {
                label,
                target: NavTarget::Url(url),
                order: navigation_order,
                icon,
                group: navigation_group,
                badge: None,
            },
            visible: Arc::new(move |cx| entries.iter().any(|visible| visible(cx))),
            badge: None,
            area: None,
        };
        registry.push_nav(entry, false);
    }
}

impl AuthPage {
    /// The page's name in a declaration error.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Profile => "profile",
            Self::TwoFactor => "two-factor",
            Self::TenantProfile => "tenant settings",
        }
    }

    /// The slug, label and icon the page takes where its def names none.
    fn defaults(self) -> (&'static str, &'static str, topcoat::icon::IconData) {
        match self {
            Self::Profile => ("profile", "tablo-shell-profile", tablo_ui::icons::USER),
            Self::TwoFactor => (
                "two-factor",
                "tablo-shell-two-factor",
                tablo_ui::icons::CIRCLE_CHECK,
            ),
            Self::TenantProfile => (
                "tenant",
                "tablo-shell-tenant-settings",
                tablo_ui::icons::PENCIL,
            ),
        }
    }

    /// Whether the request's user may open the page.
    fn offered(self, cx: &Cx) -> bool {
        match self {
            Self::Profile => crate::auth::profile_offered(cx),
            Self::TwoFactor => crate::auth::two_factor_offered(cx),
            Self::TenantProfile => crate::auth::tenant_profile_offered(cx),
        }
    }
}

impl Registry {
    /// The mistakes in the auth pages placed in the panel's areas: each one `auth` does not
    /// install.
    pub(in crate::panel) fn unoffered_auth_pages(
        &self,
        auth: &crate::auth::AuthConfig,
    ) -> Vec<DeclarationError> {
        self.auth_pages
            .iter()
            .filter(|(page, _)| match page {
                AuthPage::Profile => auth.profile_editor().is_none(),
                AuthPage::TwoFactor => auth.two_factor_config().is_none(),
                AuthPage::TenantProfile => auth.tenant_editor().is_none(),
            })
            .map(|(page, _)| {
                DeclarationError::panel(DeclarationErrorKind::Area(AreaFault::AuthPageNotOffered(
                    page.name(),
                )))
            })
            .collect()
    }

    /// Places the auth page `page`, its def adjusted by `customize`, in the group the
    /// registrations now running land in.
    fn register_auth_page(&mut self, page: AuthPage, customize: CustomizePage) {
        let refuse = |fault| DeclarationError::panel(DeclarationErrorKind::Area(fault));
        if self.auth_pages.iter().any(|(placed, _)| *placed == page) {
            self.errors
                .push(refuse(AreaFault::AuthPagePlacedTwice(page.name())));
            return;
        }
        let (slug, label, icon) = page.defaults();
        let mounted = MountedPage::declared(
            customize(PageDef::new().icon(icon)),
            slug.to_string(),
            Text::message(label),
        )
        .with_access(move |cx| page.offered(cx));
        let declared = [
            (!mounted.widgets.widgets.is_empty(), "widgets"),
            (!mounted.filters.is_empty(), "filters"),
            (mounted.access_declared, "an access check"),
            // Auth renders the two-factor page as no form it shares with the others.
            (
                page == AuthPage::TwoFactor && !mounted.header_actions.is_empty(),
                "header actions",
            ),
        ];
        let mut refused = false;
        for (_, feature) in declared.into_iter().filter(|(declares, _)| *declares) {
            refused = true;
            self.errors.push(refuse(AreaFault::AuthPageDeclares {
                page: page.name(),
                feature,
            }));
        }
        if refused {
            return;
        }
        let Some(url) = self.claim("PageDef::slug", &mounted.slug, |kind| {
            DeclarationError::panel(kind)
        }) else {
            return;
        };
        self.auth_pages.push((page, url.clone()));
        let entry = NavEntry {
            item: mounted.navigation.clone().resolved(&url),
            visible: mounted.access(),
            badge: None,
            area: None,
        };
        let listed = !mounted.navigation_hidden;
        if !mounted.header_actions.is_empty() {
            match page {
                AuthPage::Profile => self.mount_actions::<ProfileSettings>(&url, mounted),
                AuthPage::TenantProfile => self.mount_actions::<TenantSettings>(&url, mounted),
                AuthPage::TwoFactor => {}
            }
        }
        if listed {
            self.push_nav(entry, false);
        }
    }

    /// Serves the header actions of `mounted`, the auth page at `url`, as the page `P`'s: their
    /// POST and option search, behind the page's access, checked as a page's are.
    fn mount_actions<P: Page>(&mut self, url: &str, mounted: MountedPage) {
        let mounted = Arc::new(mounted);
        let actions = list_actions_route(url);
        self.page(http::Method::POST, &actions, page_action::<P>);
        self.route(
            http::Method::GET,
            &format!("{actions}/options"),
            page_action_options::<P>,
        );
        self.page_checks.push(check_page::<P>(Arc::clone(&mounted)));
        self.urls.insert(TypeId::of::<P>(), url.to_string());
        self.mounted_pages.insert(TypeId::of::<P>(), mounted);
    }
}

/// `GET {prefix}/{area}`: a redirect to the first entry of the area the user may open, in the
/// order its sidebar lists them; 404 when they may open none.
fn area_root(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        // Re-checks the resolved user so a mis-mounted gate cannot leak the area's entries.
        crate::auth::guard(cx)?;
        let path = topcoat::router::request::uri(cx).path();
        let target = current(cx)
            .and_then(|panel| {
                let area = panel.area_at(path)?;
                crate::panel::shell::area_entries(cx, panel, area)
                    .into_iter()
                    .find_map(|item| item.url().map(str::to_string))
            })
            .ok_or_else(topcoat::router::error::not_found)?;
        Err(redirect(target).into())
    })
}
