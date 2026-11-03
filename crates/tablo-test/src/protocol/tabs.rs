//! Reads a page's `Tabs` blocks: `[role=tab]` for each trigger, `aria-selected` for the active
//! one, `aria-controls` for its `[role=tabpanel]`, `hidden` on a panel the page hides,
//! `data-tab-error` on the trigger of a tab holding a field error, and `[data-tab-badge]` for the
//! badge closing a trigger.

use super::dom::{attr_value, strip_text, tags_where};

/// One tab of a `Tabs` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    /// The trigger's text, without its badge.
    pub label: String,
    /// The text of the trigger's badge, if it shows one.
    pub badge: Option<String>,
    /// Whether the page opens on this tab: its trigger is selected and its panel shows.
    pub active: bool,
    /// Whether the trigger marks a field error in the tab.
    pub errored: bool,
    /// The keys the controls in the tab's panel post, in document order, each once.
    pub keys: Vec<String>,
}

/// The tabs of every `Tabs` block `html` renders, in document order: a nested block's after the
/// tab holding it, whose `keys` hold the nested block's too.
pub fn tabs(html: &str) -> Vec<Tab> {
    tags_where(html, |tag, attrs| {
        tag == "button" && attr_value(attrs, "role").as_deref() == Some("tab")
    })
    .into_iter()
    .map(|(_, attrs, inner)| {
        let panel = attr_value(&attrs, "aria-controls").and_then(|id| {
            tags_where(html, |_, attrs| {
                attr_value(attrs, "id").as_deref() == Some(id.as_str())
            })
            .into_iter()
            .next()
        });
        let shown = panel
            .as_ref()
            .is_some_and(|(_, attrs, _)| !attrs.iter().any(|(name, _)| name == "hidden"));
        let mut keys: Vec<String> = Vec::new();
        if let Some((_, _, panel)) = &panel {
            for (_, attrs, _) in tags_where(panel, |_, attrs| attr_value(attrs, "name").is_some()) {
                let key = attr_value(&attrs, "name").unwrap_or_default();
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
        // The badge closes the trigger: the label is what comes before it.
        let (label, badge) = match inner.find("data-tab-badge") {
            Some(at) => {
                let start = inner[..at].rfind('<').unwrap_or(at);
                (&inner[..start], Some(strip_text(&inner[start..])))
            }
            None => (inner.as_str(), None),
        };
        Tab {
            label: strip_text(label),
            badge,
            active: shown && attr_value(&attrs, "aria-selected").as_deref() == Some("true"),
            errored: attrs.iter().any(|(name, _)| name == "data-tab-error"),
            keys,
        }
    })
    .collect()
}
