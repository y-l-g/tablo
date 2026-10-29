//! Reads a creatable choice's link to the related create page, and the page a create framed by
//! its dialog lands on.

use super::dom::{attr_value, open_tags};

/// A creatable choice's link: the select it creates a record for, and the create page it opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateOptionLink {
    /// The DOM id of the select whose choice the created record becomes.
    pub select: String,
    /// The related resource's create page.
    pub href: String,
}

/// What the page a create framed by a creatable choice's dialog lands on offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picked {
    /// The record the choice takes: its key and its title.
    Option {
        /// The record's key, the option's value.
        value: String,
        /// The record's title, the option's label.
        label: String,
    },
    /// The record was created, but the user may not choose it.
    Unchoosable,
}

/// Each creatable choice's link on the page.
pub fn create_option_links(html: &str) -> Vec<CreateOptionLink> {
    let tags = open_tags(html);
    tags.iter()
        .filter(|(tag, _, _)| tag == "a")
        .filter_map(|(_, attrs, _)| {
            let dialog = attr_value(attrs, "data-create-option")?;
            let select = tags.iter().find_map(|(tag, attrs, _)| {
                (tag == "dialog" && attr_value(attrs, "id").as_deref() == Some(dialog.as_str()))
                    .then(|| attr_value(attrs, "data-create-option-select"))
                    .flatten()
            })?;
            Some(CreateOptionLink {
                select,
                href: attr_value(attrs, "href")?,
            })
        })
        .collect()
}

/// What the landing page of a framed create offers, or `None` on any other page.
pub fn picked(html: &str) -> Option<Picked> {
    open_tags(html).into_iter().find_map(|(_, attrs, _)| {
        if attr_value(&attrs, "data-picked-hidden").is_some() {
            return Some(Picked::Unchoosable);
        }
        attr_value(&attrs, "data-picked")?;
        Some(Picked::Option {
            value: attr_value(&attrs, "data-picked-value")?,
            label: attr_value(&attrs, "data-picked-label")?,
        })
    })
}
