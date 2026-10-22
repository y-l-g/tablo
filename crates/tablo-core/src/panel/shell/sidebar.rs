//! The sidebar's entries: their order, and the labelled sections they group under.

use crate::navigation::NavigationItem;

/// `nav_items` by `order`, declaration order breaking ties: a resource's `navigation()` override
/// interleaves by setting it.
pub(in crate::panel) fn in_sidebar_order(nav_items: &[NavigationItem]) -> Vec<NavigationItem> {
    let mut nav_items = nav_items.to_vec();
    nav_items.sort_by_key(|item| item.order);
    nav_items
}

/// The sidebar's sections of `nav_items`: the ungrouped entries first, under the default label,
/// then one labelled group per `group`, in the order of its first entry. Each entry keeps its
/// index in `nav_items`.
pub(in crate::panel) fn sidebar_sections(
    nav_items: Vec<NavigationItem>,
) -> Vec<(String, Vec<(usize, NavigationItem)>)> {
    let mut sections: Vec<(String, Vec<(usize, NavigationItem)>)> =
        vec![("Navigation".to_string(), Vec::new())];
    for (index, item) in nav_items.into_iter().enumerate() {
        let at = match &item.group {
            None => Some(0),
            Some(group) => sections
                .iter()
                .skip(1)
                .position(|(label, _)| label == group)
                .map(|at| at + 1),
        };
        match at {
            Some(at) => sections[at].1.push((index, item)),
            None => sections.push((item.group.clone().unwrap_or_default(), vec![(index, item)])),
        }
    }
    if sections.len() > 1 && sections[0].1.is_empty() {
        sections.remove(0);
    }
    sections
}

#[cfg(test)]
mod tests;
