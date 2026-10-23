//! [`FilterInput`]: what a filter's control renders for one request, and the select, checklist
//! and label it builds.

use topcoat::{context::Cx, icon::icon, view::*};

use super::FilterValue;

/// A filter control's label, over its control in the filter menu.
const FILTER_LABEL_CLASS: StaticClass =
    class!("flex flex-col gap-1.5 text-sm font-medium text-foreground");

/// A checklist's button, as wide as the filter menu like its selects.
const CHECKLIST_BUTTON_CLASS: StaticClass = class!(
    "flex h-9 w-full items-center justify-between gap-2 rounded-lg border \
     border-border bg-transparent px-3 text-sm font-normal text-foreground outline-none \
     focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 \
     focus-visible:ring-offset-background"
);

/// A checklist's popover, below its button and as wide as it.
const CHECKLIST_POPOVER_CLASS: StaticClass = class!(
    "inset-auto m-0 mt-1 max-h-72 w-[anchor-size(width)] min-w-48 flex-col overflow-y-auto \
     rounded-lg border border-border bg-popover p-1 text-popover-foreground shadow-sm open:flex \
     [position-area:bottom_span-right] [position-try-fallbacks:flip-block]"
);

/// One option of a checklist.
const CHECKLIST_OPTION_CLASS: StaticClass = class!(
    "flex items-center gap-2 rounded-md px-2 py-1.5 text-sm font-normal text-foreground \
     hover:bg-foreground/5"
);

/// What a [`Filter::control`](super::Filter::control) renders for one request.
#[derive(Debug, Clone)]
pub struct FilterInput {
    name: String,
    param: String,
    label: String,
    value: FilterValue,
}

impl FilterInput {
    pub(crate) fn new(name: &str, param: String, label: &str, value: FilterValue) -> Self {
        Self {
            name: name.to_string(),
            param,
            label: label.to_string(),
            value,
        }
    }

    /// The filter's name, which the control carries as `data-filter-name`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The form field name the control submits a value as; a control submitting several values
    /// repeats it.
    pub fn param(&self) -> &str {
        &self.param
    }

    /// The form field name the control submits `part` as: [`param`](Self::param)`.{part}`.
    pub fn part_param(&self, part: &str) -> String {
        format!("{}.{part}", self.param)
    }

    /// The filter's label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The value the request carries for this filter.
    pub fn value(&self) -> &FilterValue {
        &self.value
    }

    /// A labelled `<select>` over `options`, led by the empty "All" option, which is selected
    /// while the filter holds no value. A select submits one value, so a filter holding several
    /// (`?f.status=draft&f.status=review`) marks the first; the rest keep applying until the form
    /// is submitted again.
    pub fn select<'a>(self, cx: &'a Cx, options: Vec<(String, String)>) -> BoxView<'a> {
        self.select_led_by(cx, "All", options)
    }

    /// [`select`](Self::select) with its empty option labelled `lead`, for a filter whose empty
    /// value selects something other than every row.
    pub(crate) fn select_led_by<'a>(
        self,
        cx: &'a Cx,
        lead: &str,
        options: Vec<(String, String)>,
    ) -> BoxView<'a> {
        let option_views: Vec<BoxView<'a>> = std::iter::once((String::new(), lead.to_string()))
            .chain(options)
            .map(|(value, text)| {
                let selected = if value.is_empty() {
                    self.value.is_empty()
                } else {
                    self.value.values().first() == Some(&value)
                };
                crate::schema::option_view(cx, value, text, selected)
            })
            .collect();
        let Self {
            name, param, label, ..
        } = self;
        let aria = label.clone();
        let control = view! {
            cx =>
            tablo_ui::select(
                attrs: attributes! { name=(param) data-filter-name=(name) aria-label=(aria) },
                for option in option_views {
                    (option)
                }
            )
        }
        .boxed();
        labelled(cx, label, control)
    }

    /// A labelled button opening a checkbox per option in `options`, each submitting its value
    /// under [`param`](Self::param) and checked while the filter holds it. The button reads the
    /// checked options' labels, or "All" while none is checked.
    ///
    /// The list is a popover, so it stays open while the table reruns as each box changes.
    pub fn checklist<'a>(self, cx: &'a Cx, options: Vec<(String, String)>) -> BoxView<'a> {
        let checked: Vec<&str> = options
            .iter()
            .filter(|(value, _)| self.value.holds(None, value))
            .map(|(_, label)| label.as_str())
            .collect();
        let summary = if checked.is_empty() {
            "All".to_string()
        } else {
            checked.join(", ")
        };
        let boxes: Vec<BoxView<'a>> = options
            .iter()
            .map(|(value, text)| {
                let checked = self.value.holds(None, value);
                let (name, value, text) = (self.param.clone(), value.clone(), text.clone());
                view! {
                    cx =>
                    <label class=(CHECKLIST_OPTION_CLASS)>
                        tablo_ui::checkbox(
                            attrs: attributes! { name=(name) value=(value) checked=(checked) }
                        )
                        <span>(text)</span>
                    </label>
                }
                .boxed()
            })
            .collect();
        let empty = boxes.is_empty();
        let Self {
            name, param, label, ..
        } = self;
        let list = format!("{param}-options");
        let label_id = format!("{param}-label");
        // The popover sits under the button that anchors it, whatever opened it.
        let anchor: String = list
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        view! {
            cx =>
            <div class=(FILTER_LABEL_CLASS) data-filter-name=(name)>
                <span id=(label_id.clone())>(label)</span>
                <button
                    type="button"
                    popovertarget=(list.clone())
                    aria-labelledby=(format!("{label_id} {list}-summary"))
                    class=(CHECKLIST_BUTTON_CLASS)
                    style=(format!("anchor-name: --{anchor}"))
                >
                    <span id=(format!("{list}-summary")) class="truncate">
                        (summary)
                    </span>
                    icon(
                        data: tablo_ui::icons::CHEVRON_DOWN,
                        attrs: attributes! { class="size-4 shrink-0 opacity-50" }
                    )
                </button>
                <div
                    id=(list)
                    popover=""
                    role="group"
                    aria-labelledby=(label_id)
                    class=(CHECKLIST_POPOVER_CLASS)
                    style=(format!("position-anchor: --{anchor}"))
                >
                    for checkbox in boxes {
                        (checkbox)
                    }
                    if empty {
                        <p class="px-2 py-1.5 text-sm text-muted-foreground">
                            "No options"
                        </p>
                    }
                </div>
            </div>
        }
        .boxed()
    }

    /// `control` under the filter's label.
    pub fn labelled<'a>(self, cx: &'a Cx, control: BoxView<'a>) -> BoxView<'a> {
        labelled(cx, self.label, control)
    }
}

/// `control` inside the label element the filter menu lays out.
fn labelled<'a>(cx: &'a Cx, label: String, control: BoxView<'a>) -> BoxView<'a> {
    view! {
        cx =>
        <label class=(FILTER_LABEL_CLASS)>
            (label)
            (control)
        </label>
    }
    .boxed()
}
