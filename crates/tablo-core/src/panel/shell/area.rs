//! The shell inside an [`Area`](crate::Area): the sidebar entries it lists, and the link back to
//! the panel's home above them.

use tablo_ui::{
    SidebarMenuButtonSize, SidebarMenuButtonVariant, sidebar_group, sidebar_menu,
    sidebar_menu_button_variants, sidebar_menu_item,
};
use topcoat::{
    context::Cx,
    icon::icon,
    view::{BoxView, ViewExt, view},
};

use crate::{
    NavigationItem, Text,
    panel::state::{NavEntry, PanelState},
    t,
};

/// The sidebar entries `panel` lists in `area`, or in its own sidebar for `None`: those the
/// request's user may open. A tenant group's entries render under the tenant's name.
pub(in crate::panel) fn listed<'p>(
    cx: &Cx,
    panel: &'p PanelState,
    area: Option<usize>,
) -> Vec<&'p NavEntry> {
    panel
        .nav_items
        .iter()
        .filter(|entry| entry.area.map(|slot| slot.area) == area)
        .filter(|entry| (entry.visible)(cx))
        .collect()
}

/// `entry`'s item as the sidebar renders it: an entry of an area's tenant group is grouped under
/// the name of the tenant the request acts for.
pub(in crate::panel) fn item(cx: &Cx, entry: &NavEntry) -> NavigationItem {
    let mut item = entry.item.clone();
    if entry.area.is_some_and(|slot| slot.tenant)
        && let Some(membership) = crate::auth::membership(cx)
    {
        item.group = Some(Text::from(membership.name.clone()));
    }
    item
}

/// The entries `area`'s sidebar lists for the request, in the order it lists them.
pub(in crate::panel) fn area_entries(
    cx: &Cx,
    panel: &PanelState,
    area: usize,
) -> Vec<NavigationItem> {
    let items: Vec<NavigationItem> = listed(cx, panel, Some(area))
        .into_iter()
        .map(|entry| item(cx, entry))
        .collect();
    super::sidebar::sidebar_sections(super::sidebar::in_sidebar_order(&items))
        .into_iter()
        .flat_map(|(_, entries)| entries)
        .map(|(_, item)| item)
        .collect()
}

/// The link from an area's sidebar back to the panel's home at `home`.
pub(super) fn back_link<'a>(cx: &'a Cx, home: &str) -> BoxView<'a> {
    let link = crate::navigation::runtime_link(cx, home);
    let class =
        sidebar_menu_button_variants(SidebarMenuButtonVariant::Default, SidebarMenuButtonSize::Md);
    let label = t!(cx, "tablo-shell-back");
    view! {
        sidebar_group(
            sidebar_menu(
                sidebar_menu_item(
                    <a (link) class=(class) data-area-back="" title=(label.clone())>
                        icon(data: tablo_ui::icons::ARROW_LEFT)
                        <span>(label)</span>
                    </a>
                )
            )
        )
    }
    .boxed()
}
