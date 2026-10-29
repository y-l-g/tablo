//! Where a field's control renders on its page.

/// Where a field's control renders: under its parent's value, and in which form of its page.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Placement<'p> {
    /// The value the field's parent posts: a dependent choice offers the rows it selects.
    pub(crate) parent: Option<&'p str>,
    /// The prefix of the form's DOM ids, so several forms on one page never share one; empty
    /// for a form alone on its page.
    pub(crate) scope: &'p str,
    /// The URL a searchable choice fetches its options from; `None` for the resource's own
    /// `{list}/-/options`, which the browser derives from the page's URL.
    pub(crate) options: Option<&'p str>,
    /// Whether the form edits a stored record, where `disabled_on_edit` disables a control.
    pub(crate) editing: bool,
    /// Whether the request's field access lets the user view the field but not change it.
    pub(crate) locked: bool,
    /// What every control's name starts with: a related form's `{slug}.` inside its owner's
    /// form, empty otherwise.
    pub(crate) prefix: &'p str,
}

/// Where one control posts: its DOM id, the name it posts under, and whether it renders
/// disabled.
pub(crate) struct Posting {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) disabled: bool,
}

impl Placement<'_> {
    /// The name the control of the field `key` posts under.
    pub(crate) fn name(&self, key: &str) -> String {
        format!("{}{key}", self.prefix)
    }

    /// The DOM id of the control posting `name`.
    pub(crate) fn id(&self, name: &str) -> String {
        if self.scope.is_empty() {
            name.to_string()
        } else {
            format!("{}-{name}", self.scope)
        }
    }
}
