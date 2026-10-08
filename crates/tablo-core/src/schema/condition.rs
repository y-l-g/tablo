//! Conditions: a field or a layout block that shows only while another field posts one of the
//! values its condition names.
//!
//! The browser follows the watched field through one signal per condition, so the form needs no
//! rerun. A hidden field or block sits in a disabled fieldset, which the browser neither validates
//! nor submits. The server reads the same submission: [`Schema::condition_hidden`] names the
//! fields it hides, and the panel drops their keys before parsing, so an edit keeps their stored
//! values and a create takes their blank answers.

use std::collections::HashMap;

use topcoat::{
    context::Cx,
    runtime::{Event, Signal, signal},
    view::*,
};

use super::{
    Schema,
    fields::{ChoiceField, CustomField, Field, TextField},
    tree::Node,
};
use crate::{DeclarationErrorKind, topcoat_compat::async_page};

/// A field a condition can watch: a text, choice or custom field of the form `F`, passed to
/// `visible_when` by reference.
///
/// A condition follows a toggle (`Field::toggle`, `Field::toggle_input`) through its checkbox, and
/// any other control through its `value`: an app's own checkbox control posts the same `value`
/// checked or not, so a condition cannot follow it.
pub trait Watched<F> {
    #[doc(hidden)]
    fn key(&self) -> &str;

    #[doc(hidden)]
    fn checkbox(&self) -> bool {
        false
    }
}

impl<F> Watched<F> for TextField<F> {
    fn key(&self) -> &str {
        self.name()
    }
}

impl<F> Watched<F> for ChoiceField<F> {
    fn key(&self) -> &str {
        self.name()
    }
}

impl<F> Watched<F> for CustomField<F> {
    fn key(&self) -> &str {
        self.name()
    }

    fn checkbox(&self) -> bool {
        self.is_checkbox()
    }
}

/// Shows a field or a block while the field `watched` posts one of `values`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Condition {
    watched: String,
    /// Whether the watched field is a checkbox, which renders unchecked with no value.
    checkbox: bool,
    values: Vec<String>,
}

impl Condition {
    pub(crate) fn new<F>(
        watched: &impl Watched<F>,
        values: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            watched: watched.key().to_string(),
            checkbox: watched.checkbox(),
            values: values.into_iter().map(Into::into).collect(),
        }
    }

    /// Whether the submission `values` shows what the condition guards. A checkbox with no value
    /// reads as `false`, as it renders and posts.
    fn holds(&self, values: &HashMap<String, String>) -> bool {
        let posted = match values.get(&self.watched) {
            Some(value) => value.trim(),
            None if self.checkbox => "false",
            None => "",
        };
        self.values.iter().any(|value| value == posted)
    }

    /// Renders `inner` in a fieldset the browser hides and disables while the condition fails, so
    /// a hidden required field never blocks the submit.
    pub(crate) fn guard<'a>(
        &self,
        cx: &'a Cx,
        scope: &str,
        values: &HashMap<String, String>,
        inner: BoxView<'a>,
    ) -> BoxView<'a> {
        let condition = self.clone();
        let initial = self.holds(values);
        let scope = scope.to_string();
        async_page(async move {
            let shown = shown(cx, &scope, &condition, initial);
            let enabled = shown.clone();
            Ok(view! {
                cx =>
                <fieldset
                    class="contents"
                    :disabled=$(!enabled.get())
                    :hidden=$(!shown.get())
                >
                    (inner)
                </fieldset>
            })
        })
    }
}

/// The signal holding whether `condition` holds in the form `scope`, shared by its guard and its
/// watched field.
///
/// Keyed by the page's path: navigation carries the values of signals two pages share, and
/// another record's form starts from its own stored values. Keyed by the form's scope too: an
/// action's input in a dialog may watch a key the page's own form watches.
fn shown(cx: &Cx, scope: &str, condition: &Condition, initial: bool) -> Signal<bool> {
    let page = topcoat::context::try_request_context::<http::request::Parts>(cx)
        .map(|parts| parts.uri.path().to_string())
        .unwrap_or_default();
    let values = condition.values.join("\u{1f}");
    signal(
        &cx.keyed((
            "tablo-condition",
            page,
            scope,
            condition.watched.as_str(),
            values,
        )),
        move || initial,
    )
}

/// Renders the watched field `field` inside one change handler per condition reading it.
///
/// A handler reads only the event of the control posting the watched key: a searchable choice's
/// filter box or an app control's own inputs change inside the same wrapper. A runtime expression
/// has no `||`, so a condition naming several values stacks one handler per value: the innermost
/// sets the signal from the first value, and each outer one, run after it as the event bubbles,
/// sets it when its own value matches.
pub(crate) fn watch<'a>(
    cx: &'a Cx,
    field: &Field,
    conditions: &[Condition],
    scope: &str,
    values: &HashMap<String, String>,
    control: BoxView<'a>,
) -> BoxView<'a> {
    let scope = scope.to_string();
    let conditions: Vec<(Condition, bool)> = conditions
        .iter()
        .map(|condition| (condition.clone(), condition.holds(values)))
        .collect();
    let checkbox = field.is_checkbox();
    let key = field.name().to_string();
    async_page(async move {
        let mut view = control;
        for (condition, initial) in conditions {
            let shown = shown(cx, &scope, &condition, initial);
            if checkbox {
                // A checkbox posts `true` when checked and its hidden `false` otherwise.
                let on = condition.values.iter().any(|value| value == "true");
                let off = condition.values.iter().any(|value| value == "false");
                let name = key.clone();
                view = view! {
                    cx =>
                    <div
                        class="contents"
                        @change=$(|e: Event| if e.target.name == name {
                            shown.set(if e.target.checked { on } else { off })
                        })
                    >
                        (view)
                    </div>
                }
                .boxed();
                continue;
            }
            let mut values = condition.values.into_iter();
            let Some(first) = values.next() else {
                continue;
            };
            let (set, name) = (shown.clone(), key.clone());
            view = view! {
                cx =>
                <div
                    class="contents"
                    @change=$(|e: Event| if e.target.name == name {
                        set.set(e.target.value.trim() == first)
                    })
                >
                    (view)
                </div>
            }
            .boxed();
            for value in values {
                let (set, name) = (shown.clone(), key.clone());
                view = view! {
                    cx =>
                    <div
                        class="contents"
                        @change=$(|e: Event| if e.target.name == name {
                            if e.target.value.trim() == value {
                                set.set(true)
                            }
                        })
                    >
                        (view)
                    </div>
                }
                .boxed();
            }
        }
        Ok(view)
    })
}

impl Node {
    /// The condition on this node: a field's own, or a layout block's.
    fn condition<'s>(&'s self, fields: &'s [Field]) -> Option<&'s Condition> {
        match self {
            Node::Field(index) => fields[*index].condition(),
            Node::Section(s) => s.condition.as_ref(),
            Node::Group(g) => g.condition.as_ref(),
            Node::Grid(g) => g.condition.as_ref(),
            Node::Embedded(_) | Node::Unbound(_) => None,
        }
    }
}

/// Where a schema's conditions sit.
#[derive(Default)]
struct Scan<'s> {
    /// Each field slot with the conditions guarding it, outermost first and its own last.
    fields: Vec<(usize, Vec<&'s Condition>)>,
    /// Each condition with the conditions guarding the node it sits on.
    conditions: Vec<(&'s Condition, Vec<&'s Condition>)>,
}

impl<'s> Scan<'s> {
    fn of(nodes: &'s [Node], fields: &'s [Field]) -> Self {
        let mut scan = Self::default();
        scan.walk(nodes, fields, &mut Vec::new());
        scan
    }

    fn walk(&mut self, nodes: &'s [Node], fields: &'s [Field], chain: &mut Vec<&'s Condition>) {
        for node in nodes {
            let own = node.condition(fields);
            if let Some(condition) = own {
                self.conditions.push((condition, chain.clone()));
                chain.push(condition);
            }
            match node {
                Node::Field(index) => self.fields.push((*index, chain.clone())),
                Node::Embedded(embedded) => {
                    embedded.visit_fields(&mut |index| self.fields.push((index, chain.clone())));
                }
                node => self.walk(node.children().unwrap_or_default(), fields, chain),
            }
            if own.is_some() {
                chain.pop();
            }
        }
    }

    /// The conditions guarding the field named `key`, or `None` when the schema places none.
    fn guarding(&self, fields: &[Field], key: &str) -> Option<&[&'s Condition]> {
        self.fields
            .iter()
            .find(|(index, _)| fields[*index].name() == key)
            .map(|(_, chain)| chain.as_slice())
    }
}

impl<F> Schema<F> {
    /// The keys of the fields a condition hides in the submission `values`.
    pub(crate) fn condition_hidden(&self, values: &HashMap<String, String>) -> Vec<String> {
        Scan::of(&self.nodes, &self.fields)
            .fields
            .into_iter()
            .filter(|(_, chain)| chain.iter().any(|condition| !condition.holds(values)))
            .map(|(index, _)| self.fields[index].name().to_string())
            .collect()
    }

    /// The conditions each watched field's key drives, each once.
    pub(crate) fn watched(&self) -> HashMap<String, Vec<Condition>> {
        let mut out: HashMap<String, Vec<Condition>> = HashMap::new();
        for (condition, _) in Scan::of(&self.nodes, &self.fields).conditions {
            let driven = out.entry(condition.watched.clone()).or_default();
            if !driven.contains(condition) {
                driven.push(condition.clone());
            }
        }
        out
    }

    /// What is wrong with the schema's conditions: a watched field the schema does not place, one
    /// a condition hides while what it guards still shows, a value the watched field never posts,
    /// and a hidden field with no blank answer, which a submission that hides it could not parse.
    pub(crate) fn condition_errors(&self) -> Vec<DeclarationErrorKind> {
        let scan = Scan::of(&self.nodes, &self.fields);
        let mut errors = Vec::new();
        for (condition, guarding) in &scan.conditions {
            let watched = condition.watched.clone();
            match scan.guarding(&self.fields, &condition.watched) {
                None => errors.push(DeclarationErrorKind::UnplacedWatchedField {
                    field: watched.clone(),
                }),
                // The browser follows the watched field's last change, the server its posted
                // value: they agree only while every condition hiding the watched field also
                // hides what it guards.
                Some(chain)
                    if !chain
                        .iter()
                        .all(|hiding| guarding.iter().any(|c| std::ptr::eq(*c, *hiding))) =>
                {
                    errors.push(DeclarationErrorKind::HiddenWatchedField {
                        field: watched.clone(),
                    });
                }
                Some(_) => {}
            }
            if let Some(field) = self.fields.iter().find(|f| f.name() == condition.watched) {
                for value in &condition.values {
                    if field.can_post(value) == Some(false) {
                        errors.push(DeclarationErrorKind::UnpostedConditionValue {
                            field: watched.clone(),
                            value: value.clone(),
                        });
                    }
                }
            }
        }
        for (index, chain) in &scan.fields {
            let field = &self.fields[*index];
            if !chain.is_empty() && field.is_required() {
                errors.push(DeclarationErrorKind::RequiredConditionalField {
                    field: field.name().to_string(),
                });
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests;
