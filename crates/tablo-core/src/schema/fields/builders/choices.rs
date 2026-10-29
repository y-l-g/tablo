//! The choice builder's modifiers: its options, how they render, and where they load from.

use std::sync::Arc;

use super::{
    super::{super::condition::Watched, ChoiceControl, ControlKind, choice},
    ChoiceField, IntoOptions, OptionSource,
};
use crate::values::FormScalar;

impl<F> ChoiceField<F> {
    fn choice_mut(&mut self) -> &mut ChoiceControl {
        match &mut self.0.control {
            ControlKind::Choice(choice) => choice,
            _ => unreachable!("a ChoiceField holds a choice control"),
        }
    }

    /// Static options: values that are their own label
    /// (`["draft", "published"]`), `(value, label)` pairs whose label is a string or a
    /// [`Text`](crate::Text) (`[("draft", text!("status-draft"))]`), or a
    /// [`#[derive(Options)]`](crate::Options) enum's
    /// [`options()`](crate::schema::Options::options).
    pub fn options(mut self, options: impl IntoOptions) -> Self {
        self.choice_mut().options = options.into_options();
        self
    }

    /// Takes several options: a checkbox per option, posting the field's key once per checked
    /// box. A record form's many-to-many field (`Vec<K>` with `#[form(relationship = R)]`) is one;
    /// [`parse_list`](crate::values::parse_list) reads one in an action's input.
    ///
    /// It offers at most [`MAX_RELATIONSHIP_OPTIONS`](crate::schema::MAX_RELATIONSHIP_OPTIONS)
    /// records: past them it lists none but the chosen ones, and refuses a submission choosing
    /// another. A record form's edit re-checks only the keys the record does not link yet.
    /// `searchable` filters the listed options as the user types. A condition cannot watch it,
    /// it cannot depend on another field, and its `default` is a list.
    pub fn multiple(mut self) -> Self {
        self.choice_mut().multiple = true;
        self
    }

    /// Renders a radio per option rather than a `<select>`, so every option shows without a
    /// click: for a few options, such as a status or a visibility. A [`multiple`](Self::multiple)
    /// choice keeps its checkboxes.
    ///
    /// The options load as a select's do, and past the option cap only the chosen one lists.
    /// [`searchable`](Self::searchable) filters the listed options as the user types. A radio
    /// choice cannot [depend on](Self::depends_on) another field, since the browser lists its
    /// options once, but another choice can depend on it.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, visibility: String }
    /// # use tablo_core::{Field, lens};
    /// Field::choice(lens!(Post.visibility))
    ///     .options(vec![
    ///         ("public".to_string(), "Public".to_string()),
    ///         ("private".to_string(), "Private".to_string()),
    ///     ])
    ///     .radio();
    /// ```
    pub fn radio(mut self) -> Self {
        self.choice_mut().layout = choice::Layout::Radio;
        self
    }

    /// Renders a button per option, pressed while its option is chosen: a compact
    /// [`radio`](Self::radio) list, or with [`multiple`](Self::multiple), a compact checkbox
    /// list. It takes the same modifiers and limits as [`radio`](Self::radio).
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Task { #[key] #[auto] id: uuid::Uuid, priority: String }
    /// # use tablo_core::{Field, lens};
    /// Field::choice(lens!(Task.priority))
    ///     .options(["low", "normal", "high"])
    ///     .buttons();
    /// ```
    pub fn buttons(mut self) -> Self {
        self.choice_mut().layout = choice::Layout::Buttons;
        self
    }

    /// Refuse a submission choosing fewer than `count` options. A [`multiple`](Self::multiple)
    /// choice's blank answer is the empty list, which no other rule refuses, so `min_items(1)` is
    /// how one requires a choice. Only the server checks it. A choice that is not `multiple` fails
    /// the mount, as does a minimum above `max_items` or above the count of static options.
    pub fn min_items(mut self, count: usize) -> Self {
        self.choice_mut().bounds.min = Some(count);
        self
    }

    /// Refuse a submission choosing more than `count` options; [`min_items`](Self::min_items)
    /// says which choices take it.
    pub fn max_items(mut self, count: usize) -> Self {
        self.choice_mut().bounds.max = Some(count);
        self
    }

    /// Filters options as the user types, fetching from the relationship past the option cap.
    pub fn searchable(mut self) -> Self {
        self.choice_mut().searchable = true;
        self
    }

    /// Loads options from a related source's tenant-scoped query, each valued by its primary key
    /// and labelled by the source's [`label`](OptionSource::label), degrading to type-to-search
    /// past the option cap.
    ///
    /// A resource's options carry its [`record_title`](crate::ResourceDef::record_title), the
    /// title its detail page shows. [`relationship_labelled`](Self::relationship_labelled)
    /// labels one field's options otherwise.
    ///
    /// - Options come from the related resource's tenant-scoped query and follow its policy: the
    ///   list is empty and the field shows "not available" unless the related resource's policy
    ///   allows `ViewAny`, and each record must pass its `View`.
    /// - A submitted key must be one of those records, so a hand-crafted POST cannot point at
    ///   another tenant's row. The write checks the key again inside its transaction, so a record
    ///   deleted, moved to another tenant or hidden since the form validated refuses the write with
    ///   the same field error.
    /// - At most 200 options load. Past that, a `.searchable()` choice turns into type-to-search
    ///   against `GET {list_url}/-/options`, which searches the related table's `searchable()`
    ///   columns; a non-searchable choice shows an error instead. `.searchable()` also filters a
    ///   short list as you type. Without JavaScript the plain select remains.
    pub fn relationship<R>(mut self) -> Self
    where
        R: OptionSource + 'static,
    {
        self.choice_mut().relationship = Some(choice::Relationship::new::<R>(R::label));
        self
    }

    /// Offers to create the related record from the form: a link beside the select opens the
    /// [`relationship`](Self::relationship)'s resource's create page in a dialog, and the record it
    /// saves becomes the choice, while the form keeps what was typed in it.
    ///
    /// - The dialog frames the related resource's own create page, so the record passes that
    ///   resource's policy, tenancy, field access, rules and hooks, as a create from its list does.
    ///   The link renders only while the panel mounts the resource with a create page and its
    ///   policy allows `Create`.
    /// - The saved record becomes the choice only when the related resource lets the user view it
    ///   in the request's tenant; otherwise the dialog says it was created and the choice stays as
    ///   it was. The submission checks the key as for any relationship.
    /// - The option shows the record's [`record_title`](crate::ResourceDef::record_title) until the
    ///   form renders again, even under [`relationship_labelled`](Self::relationship_labelled).
    /// - Without JavaScript the link opens the create page in a new tab, and the record is offered
    ///   once the form renders again.
    /// - Mounting refuses a creatable choice that is not a single select over a resource's records:
    ///   a multiple choice, [radios](Self::radio) or [buttons](Self::buttons), a
    ///   [dependent](Self::depends_on) choice, a source that is not a resource, and a choice in a
    ///   repeater's rows.
    pub fn creatable(mut self) -> Self {
        self.choice_mut().creatable = true;
        self
    }

    /// Offers only the related rows whose column `column` equals the value `parent` posts: the
    /// cities of the chosen country. The browser fetches the options again when `parent` changes,
    /// and a submission must name a row of the posted parent value. A blank parent offers none.
    ///
    /// `column` belongs to the [`relationship`](Self::relationship)'s source model; mounting
    /// refuses another model, a choice without a relationship, and a `parent` the schema does not
    /// place.
    ///
    /// - The form renders the options of the parent's current value, and none while it is blank.
    ///   When the parent changes, the browser fetches the new options from `GET
    ///   {list_url}/-/options` and keeps the current choice only if they still offer it. A choice
    ///   they drop changes too, so a choice depending on it, or a condition watching it, follows.
    /// - A submission must name a record of the parent value it posts, or, on an edit that does not
    ///   post the parent, of its stored value. The write checks the key again inside its
    ///   transaction, as for any relationship.
    /// - The 200-option cap counts the parent's records. Past it, a `.searchable()` dependent
    ///   choice searches among them, and any other offers no option. A failed fetch offers none
    ///   either.
    /// - Without JavaScript the options stay those of the value the page rendered with, and a
    ///   submission naming another value's record is refused.
    /// - `column` must belong to the relationship's model, and the parent must be placed in the
    ///   same schema: mounting refuses either mistake. Only a resource's form serves the options,
    ///   so mounting also refuses a dependent choice in an action's input.
    /// - A multiple choice neither depends on another field nor narrows one: mounting refuses both.
    pub fn depends_on<M, T>(
        mut self,
        parent: &impl Watched<F>,
        column: impl Into<toasty::stmt::Path<M, T>>,
    ) -> Self
    where
        M: toasty::schema::Model + Send + Sync + 'static,
        T: FormScalar + toasty::stmt::IntoExpr<T> + Send + Sync + 'static,
    {
        let column: toasty::stmt::Path<M, T> = column.into();
        let field = crate::toasty_compat::model::field::<M>(
            &crate::toasty_compat::model::ModelPath::of(&column),
        )
        .ok()
        .map(|field| field.name);
        self.choice_mut().parent = Some(choice::Parent {
            watched: parent.key().to_string(),
            model: std::any::TypeId::of::<M>(),
            column: field,
            scope: Arc::new(move |value: &str| {
                T::parse_form(value)
                    .ok()
                    .map(|value| column.clone().eq(value))
            }),
        });
        self
    }

    /// Loads options as [`relationship`](Self::relationship) does, labelling each by `label`
    /// instead of the source's label: to tell apart records that share a title. The option's
    /// record loads without its relations.
    pub fn relationship_labelled<R>(
        mut self,
        label: impl Fn(&R::Model) -> String + Send + Sync + 'static,
    ) -> Self
    where
        R: OptionSource + 'static,
    {
        self.choice_mut().relationship = Some(
            choice::Relationship::new::<R>(move |_cx: &topcoat::context::Cx, record: &R::Model| {
                label(record)
            })
            .with_custom_label(),
        );
        self
    }
}
