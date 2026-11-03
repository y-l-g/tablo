//! Reads a page's `Wizard` blocks: `[data-wizard-trigger]` for each step's trigger,
//! `aria-current="step"` for the current one, `aria-controls` for its panel, `hidden` on a panel
//! the page hides, and `data-wizard-error` on the trigger of a step holding a field error.

use super::dom::{attr_value, strip_text, tags_where};

/// One step of a `Wizard` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WizardStep {
    /// The trigger's text.
    pub label: String,
    /// Whether the page opens on this step: its trigger is current and its panel shows.
    pub current: bool,
    /// Whether the trigger marks a field error in the step.
    pub errored: bool,
    /// The keys the controls in the step's panel post, in document order, each once.
    pub keys: Vec<String>,
}

/// The steps of every `Wizard` block `html` renders, in document order.
pub fn wizard_steps(html: &str) -> Vec<WizardStep> {
    tags_where(html, |tag, attrs| {
        tag == "button" && attrs.iter().any(|(name, _)| name == "data-wizard-trigger")
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
        WizardStep {
            label: strip_text(&inner),
            current: shown && attr_value(&attrs, "aria-current").as_deref() == Some("step"),
            errored: attrs.iter().any(|(name, _)| name == "data-wizard-error"),
            keys,
        }
    })
    .collect()
}
