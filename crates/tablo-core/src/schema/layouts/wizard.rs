//! Holds the `Wizard` block, which walks a form's fields one step at a time.

use topcoat::{
    Result,
    context::Cx,
    view::{StaticClass, class, *},
};

use super::{Span, spanned, tab_id};
use crate::schema::{
    Schema,
    condition::Condition,
    fields::Field,
    tree::{IntoSchema, Node, Retype, Source, render_nodes},
};

/// Renders the row of step triggers, which numbers them through a CSS counter: a step a condition
/// hides takes no number.
const STEP_LIST: StaticClass =
    class!("flex flex-wrap items-center gap-x-6 gap-y-2 [counter-reset:tablo-step]");
/// Renders one trigger, which the current step raises and an error marks.
const STEP_TRIGGER: StaticClass = class!(
    "group inline-flex items-center gap-2 rounded-md text-sm font-medium text-muted-foreground \
     outline-none [counter-increment:tablo-step] focus-visible:ring-[3px] \
     focus-visible:ring-ring/50 aria-[current=step]:text-foreground \
     data-wizard-error:text-destructive"
);
/// Renders a trigger's number, filled on the current step and outlined on a passed one.
const STEP_NUMBER: StaticClass = class!(
    "flex size-7 shrink-0 items-center justify-center rounded-full border border-input text-xs \
     before:content-[counter(tablo-step)] group-data-wizard-done:border-primary \
     group-data-wizard-done:text-primary group-aria-[current=step]:border-primary \
     group-aria-[current=step]:bg-primary group-aria-[current=step]:text-primary-foreground \
     group-data-wizard-error:border-destructive group-data-wizard-error:text-destructive"
);

/// A block walking a form's fields one [`Step`] at a time, with a button to the next step and one
/// back.
///
/// ```rust
/// # #[derive(Debug, toasty::Model, tablo_core::ModelColumns)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String, email: String, bio: String }
/// use tablo_core::{Field, Schema, Step, Wizard};
///
/// let user = User::fields();
/// Schema::new(Wizard::new([
///     Step::new("Account").schema((Field::text(user.name()), Field::text(user.email()))),
///     Step::new("About").schema(Field::text(user.bio()).multiline(4)),
/// ]));
/// ```
///
/// Every step's fields render and post, as a [`Tabs`](crate::Tabs) block's do, and the form's
/// own buttons save from any step. The next step opens once the browser's own checks pass on the
/// current one's controls; a later step's trigger runs them on each step before it, and an earlier
/// one's opens it without. Enter in a field moves to the next step rather than saving, until the
/// last. The page opens on the first step holding a field error, else on the first step; the
/// browser opens the step of a control it refuses on submit. A step whose every field the request
/// may not see renders no trigger, and a block with no such step renders nothing.
#[derive(Debug)]
pub struct Wizard<C = Schema> {
    /// The block's steps, each one node.
    pub(crate) children: C,
    pub(crate) condition: Option<Condition>,
    pub(super) span: Option<Span>,
}

impl Wizard<()> {
    /// Holds `steps`, in the order the form walks them.
    pub fn new<F>(steps: impl IntoIterator<Item = Step<Schema<F>>>) -> Wizard<Schema<F>> {
        let mut children = Schema::<F>::default();
        for step in steps {
            let mut fields = Vec::new();
            let step = step.map(|mut children| {
                fields = std::mem::take(&mut children.fields);
                children.retype()
            });
            children.append(Schema::from_parts(vec![Node::Step(Box::new(step))], fields));
        }
        Wizard {
            children,
            condition: None,
            span: None,
        }
    }
}

impl<C> Wizard<C> {
    pub(crate) fn map<D>(self, f: impl FnOnce(C) -> D) -> Wizard<D> {
        Wizard {
            children: f(self.children),
            condition: self.condition,
            span: self.span,
        }
    }
}

impl<F> IntoSchema<F> for Wizard<Schema<F>> {
    fn into_schema(self) -> Schema<F> {
        let mut fields = Vec::new();
        let block = self.map(|mut children| {
            fields = std::mem::take(&mut children.fields);
            children.retype()
        });
        Schema::from_parts(vec![Node::Wizard(Box::new(block))], fields)
    }
}

/// One step of a [`Wizard`] block: a titled trigger and the fields it shows.
#[derive(Debug)]
pub struct Step<C = Schema> {
    label: crate::Text,
    pub(crate) children: C,
    pub(crate) condition: Option<Condition>,
}

impl Step<()> {
    pub fn new(label: impl Into<crate::Text>) -> Self {
        Self {
            label: label.into(),
            children: (),
            condition: None,
        }
    }

    /// Holds a form's fields and blocks.
    pub fn schema<F>(self, children: impl IntoSchema<F>) -> Step<Schema<F>> {
        self.map(|_| children.into_schema())
    }
}

impl<C> Step<C> {
    pub(crate) fn map<D>(self, f: impl FnOnce(C) -> D) -> Step<D> {
        Step {
            label: self.label,
            children: f(self.children),
            condition: self.condition,
        }
    }
}

impl Wizard<Schema> {
    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        let steps: Vec<(&Node, &Step)> = self
            .children
            .nodes
            .iter()
            .filter_map(|node| match node {
                Node::Step(step) if node.shows_any(fields, source) => Some((node, &**step)),
                _ => None,
            })
            .collect();
        let errored: Vec<bool> = steps
            .iter()
            .map(|(node, _)| node.has_error(fields, source))
            .collect();
        // The first step holding an error, else the first the page shows.
        let shown: Vec<bool> = steps
            .iter()
            .map(|(_, step)| source.holds(step.condition.as_ref()))
            .collect();
        let active = errored
            .iter()
            .position(|e| *e)
            .unwrap_or_else(|| shown.iter().position(|s| *s).unwrap_or(0));
        let first = shown.iter().position(|s| *s);
        let last = shown.iter().rposition(|s| *s);
        let depth = source.tab_depth();
        let mut triggers = Vec::with_capacity(steps.len());
        let mut panels = Vec::with_capacity(steps.len());
        for (index, (node, step)) in steps.iter().enumerate() {
            let id = node
                .first_field()
                .map(|field| tab_id(&source.id(fields[field].name()), depth))
                .unwrap_or_default();
            source.set_tab_depth(depth + 1);
            let body = render_nodes(cx, &step.children.nodes, fields, source).await;
            source.set_tab_depth(depth);
            let (trigger, panel) = step_view(
                cx,
                &id,
                &step.label,
                index.cmp(&active),
                errored[index],
                body?,
            );
            triggers.push(source.guard(cx, step.condition.as_ref(), trigger));
            panels.push(source.guard(cx, step.condition.as_ref(), panel));
        }
        let view = view! {
            <div data-wizard="" class="flex flex-col gap-6">
                <div class=(STEP_LIST)>
                    for trigger in triggers {
                        (trigger)
                    }
                </div>
                for panel in panels {
                    (panel)
                }
                <div class="flex items-center justify-between gap-2">
                    tablo_ui::button(
                        variant: tablo_ui::ButtonVariant::Outline,
                        attrs: attributes! {
                            type="button"
                            data-wizard-back=""
                            hidden=(first.is_none_or(|first| active <= first))
                        },
                        (crate::t!(cx, "tablo-wizard-back"))
                    )
                    tablo_ui::button(
                        variant: tablo_ui::ButtonVariant::Secondary,
                        attrs: attributes! {
                            type="button"
                            data-wizard-next=""
                            class="ms-auto"
                            hidden=(last.is_none_or(|last| active >= last))
                        },
                        (crate::t!(cx, "tablo-wizard-next"))
                    )
                </div>
            </div>
        }
        .boxed();
        Ok(spanned(cx, self.span, view))
    }
}

/// Renders the step `id`: a trigger reading `label`, and a panel holding `body`, shown only when
/// `place` is the current step's. `errored` marks the trigger of a step holding a field error.
fn step_view<'a>(
    cx: &'a Cx,
    id: &str,
    label: &crate::Text,
    place: std::cmp::Ordering,
    errored: bool,
    body: BoxView<'a>,
) -> (BoxView<'a>, BoxView<'a>) {
    let trigger_id = format!("{id}-step");
    let panel_id = format!("{id}-steppanel");
    let labelled_by = trigger_id.clone();
    let controls = panel_id.clone();
    let label = label.localize(cx);
    let current = place.is_eq();
    let trigger = view! {
        <button
            type="button"
            id=(trigger_id)
            aria-controls=(controls)
            aria-current=(current.then_some("step"))
            data-wizard-trigger=""
            data-wizard-done=(place.is_lt())
            data-wizard-error=(errored)
            class=(STEP_TRIGGER)
        >
            <span aria-hidden="true" class=(STEP_NUMBER)></span>
            (label)
        </button>
    }
    .boxed();
    let panel = view! {
        <div
            role="group"
            id=(panel_id)
            aria-labelledby=(labelled_by)
            hidden=(!current)
            data-wizard-step=""
            class="flex flex-col gap-6"
        >
            (body)
        </div>
    }
    .boxed();
    (trigger, panel)
}
