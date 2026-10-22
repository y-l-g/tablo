//! The typed builders the [`Field`] constructors return, each offering only its control's modifiers
//! so a modifier on the wrong control does not compile.

use std::{marker::PhantomData, sync::Arc};

pub use self::into::{IntoFormValue, IntoOptions};
use super::{
    super::{
        IntoSchema, OptionSource, Retype, Schema,
        condition::{Condition, Watched},
        repeater::RepeaterControl,
    },
    ChoiceControl, Control, ControlKind, Field, Kind, Pattern, RichTextControl, RichTextField,
    TextControl,
};

mod choices;
mod comparisons;
mod files;

/// `label`, `name` and `visible_when`, shared by every builder.
macro_rules! base_modifiers {
    ($builder:ident) => {
        impl $builder {
            pub(super) fn new(field: Field) -> Self {
                Self(field, PhantomData)
            }
        }

        impl<F> $builder<F> {
            pub fn label(mut self, label: impl Into<String>) -> Self {
                self.0.label = Some(label.into());
                self
            }

            pub fn name(&self) -> &str {
                self.0.name()
            }

            /// Shows the field only while `watched` posts one of `values`: an option's value, or
            /// `true` for a checked toggle. A hidden field posts nothing, so an edit keeps its
            /// stored value and a create takes its blank answer; a field with no blank answer
            /// cannot be conditional. A field shows under one condition: a later call replaces an
            /// earlier one, so a field depending on two watched fields sits in a
            /// [`Section`](crate::Section) guarded by the other.
            ///
            /// ```rust
            /// # #[derive(Debug, Clone, toasty::Model)]
            /// # struct Customer {
            /// #     #[key] #[auto] id: uuid::Uuid,
            /// #     kind: String,
            /// #     vat_number: Option<String>,
            /// # }
            /// # use tablo_core::{Field, Schema};
            /// let kind = Field::choice(Customer::fields().kind()).options(["person", "company"]);
            /// let vat = Field::text(Customer::fields().vat_number()).visible_when(&kind, ["company"]);
            /// Schema::new((kind, vat));
            /// ```
            ///
            /// - The browser shows and hides the field as the watched one changes, with no request.
            ///   A hidden field's control is disabled, so the browser neither validates nor posts
            ///   it. Without JavaScript, the fields start as the stored values show them and stay
            ///   that way.
            /// - The panel reads a submission the same way and drops a hidden field's posted key,
            ///   and a file field's carried upload: an edit keeps the stored value, and a create
            ///   stores the blank answer. A conditional field therefore needs a blank answer, and
            ///   mounting refuses one without.
            /// - Mounting also refuses a value the watched field never posts (a choice's option or
            ///   a toggle's `true` and `false` are checked), and a condition watching a field that
            ///   another condition hides while the guarded field still shows: place the guarded
            ///   field inside the block that hides the watched one. A schema also refuses a
            ///   condition watching a field it does not place, which only a page's or an action's
            ///   schema can do: a resource's form places every record-form field.
            /// - A toggle is followed through its checkbox. An app's own checkbox `Control` posts
            ///   the same `value` checked or not, so a condition cannot follow it.
            /// - A multiple choice's key holds a list, which no condition value spells: mounting
            ///   refuses a condition watching one.
            pub fn visible_when(
                mut self,
                watched: &impl Watched<F>,
                values: impl IntoIterator<Item = impl Into<String>>,
            ) -> Self {
                self.0.condition = Some(Condition::new(watched, values));
                self
            }
        }

        impl<F> From<$builder<F>> for Field {
            fn from(builder: $builder<F>) -> Field {
                builder.0
            }
        }

        impl<F> IntoSchema<F> for $builder<F> {
            fn into_schema(self) -> Schema<F> {
                self.0.into_schema().retype()
            }
        }

        impl<F> Retype for $builder<F> {
            type As<G> = $builder<G>;

            fn retype<G>(self) -> $builder<G> {
                $builder(self.0, PhantomData)
            }
        }
    };
}

/// The base modifiers, `help`, `disabled` and `custom`, on every builder of one value's control.
macro_rules! common_modifiers {
    ($builder:ident) => {
        base_modifiers!($builder);

        impl<F> $builder<F> {
            /// Renders `text` under the control, which the control names as its description.
            pub fn help(mut self, text: impl Into<String>) -> Self {
                self.0.help = Some(text.into());
                self
            }

            /// Renders the control disabled on the create and edit forms. The server drops a
            /// value posted for it anyway, so an edit keeps the stored value and a create takes
            /// the field's `default`, else its blank answer; a field with neither cannot be
            /// created, and its panel refuses to mount.
            pub fn disabled(mut self) -> Self {
                self.0.disabled = super::Disabled::Always;
                self
            }

            /// Renders the control disabled on the edit form only: the create form sets the
            /// value, and no edit changes it.
            pub fn disabled_on_edit(mut self) -> Self {
                self.0.disabled = super::Disabled::OnEdit;
                self
            }

            /// Renders the field with an app's [`Control`] instead, as [`Field::custom`] does:
            /// how a record form's control takes a custom input. The field keeps its key and
            /// label; the replaced control's own modifiers, such as `email`, `unique` or
            /// `options`, are dropped with it.
            pub fn custom(mut self, control: impl Control + 'static) -> CustomField<F> {
                self.0.control = ControlKind::Custom(Arc::new(control));
                self.0.checkbox = false;
                CustomField(self.0, PhantomData)
            }
        }
    };
}

/// Renders the control required, as a panel does for a record-form field with no blank answer.
#[cfg(test)]
macro_rules! required_for_tests {
    ($($builder:ident),*) => {$(
        impl<F> $builder<F> {
            pub(crate) fn required(mut self) -> Self {
                self.0.set_required(true);
                self
            }
        }
    )*};
}

#[cfg(test)]
required_for_tests!(TextField);

/// `default`, on every builder whose control a value fills.
macro_rules! default_modifier {
    ($($builder:ident),*) => {$(
        impl<F> $builder<F> {
            /// Fills the create form's control with `value`: any
            /// [`FormScalar`](crate::FormScalar) or a `&str` in the form spelling, or, for a
            /// [`multiple`](ChoiceField::multiple) choice, an array or `Vec` of them. On a
            /// [`disabled`](Self::disabled) field it is the value a create stores. A schema refuses a default the control never posts: one a typed field
            /// does not parse, one not among a choice's options, a list on a control posting one
            /// value, and one value on a multiple choice.
            ///
            /// ```rust
            /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
            /// # struct Post { #[key] #[auto] id: uuid::Uuid, status: String, views: i64 }
            /// # use tablo_core::Field;
            /// Field::choice(Post::fields().status())
            ///     .options(["draft", "published"])
            ///     .default("draft");
            /// Field::text(Post::fields().views()).default(0_i64).disabled();
            /// Field::choice_input("channels")
            ///     .options(["email", "sms"])
            ///     .multiple()
            ///     .default(["email"]);
            /// ```
            ///
            /// - The control names its help text in `aria-describedby`, beside its error.
            /// - A disabled control renders `disabled`, so the browser posts nothing for it, and
            ///   the panel drops a key posted anyway, with a file field's carried upload. An edit
            ///   keeps the stored value. A create stores the field's default, else its blank
            ///   answer: `.disabled()` on a field with neither refuses to mount, since no create
            ///   could fill it.
            /// - `.disabled_on_edit()` leaves the create form's control enabled: the create sets
            ///   the value, and no edit changes it.
            /// - A condition watching a disabled field reads what the browser shows: its default on
            ///   a create, its stored value on an edit, never what the submission posts.
            /// - A default seeds the create form and an action's input form; an edit form shows the
            ///   stored value. The blank answer is different: it is what an empty submission
            ///   stores. Mounting refuses a default the control never posts: one a typed field does
            ///   not parse, or not one of a choice's options.
            /// - A repeater's row controls take neither `disabled` nor `default`: mounting refuses
            ///   both.
            pub fn default(mut self, value: impl IntoFormValue) -> Self {
                self.0.default = Some(value.into_form_value());
                self
            }
        }
    )*};
}

default_modifier!(TextField, ChoiceField, CustomField, RichTextField);

/// A text field: [`Field::text`], or a record form's text control (`F` is the form).
pub struct TextField<F = ()>(pub(super) Field, PhantomData<fn() -> F>);

/// A choice field: [`Field::choice`], or a record form's choice control (`F` is the form).
pub struct ChoiceField<F = ()>(pub(super) Field, PhantomData<fn() -> F>);

/// A file field: [`Field::file`], or a record form's file control (`F` is the form).
pub struct FileField<F = ()>(pub(super) Field, PhantomData<fn() -> F>);

/// A repeater: [`Field::repeater`], or a record form's `#[form(repeat)]` control (`F` is the
/// form).
///
/// - Each row posts its item's fields under its own prefix (`steps.0.minutes`), and the repeater's
///   own key lists the rows in the order they show. A refused row renders its error under its own
///   control, and the rows come back in the order the user left them.
/// - A repeater takes at most `MAX_REPEATER_ROWS` (100) rows; a submission listing more is refused.
/// - No rows is the repeater's blank answer, so a repeater is never required. An edit that does not
///   post the repeater, such as one hidden by a condition, keeps the stored rows; one that posts no
///   rows stores an empty list.
/// - Adding, removing and moving rows needs JavaScript. Without it the stored rows still edit.
/// - An action's input can place a repeater too, with a hand-written `FormInput` whose `parse`
///   reads the rows with `parse_items`.
/// - On the detail page, `RepeaterColumn` shows each item's fields under their labels. The model's
///   default detail page includes one; its table does not.
/// - Not supported inside an item: an embedded value, another repeater, a relationship, a file
///   field and a condition.
pub struct RepeaterField<F = ()>(pub(super) Field, PhantomData<fn() -> F>);

/// A field an app's [`Control`] renders: [`Field::custom`] and [`Field::toggle`], or a record
/// form's toggle (`F` is the form).
pub struct CustomField<F = ()>(pub(super) Field, PhantomData<fn() -> F>);

impl<F> CustomField<F> {
    /// Renders a `bool` field as a switch rather than a checkbox: the same toggle, posting `true`
    /// or `false`, drawn as an on/off track. A condition follows it as it follows a toggle.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, featured: bool }
    /// # use tablo_core::{Field, lens};
    /// Field::toggle(lens!(Post.featured)).switch();
    /// ```
    pub fn switch(mut self) -> Self {
        self.0.control = ControlKind::Custom(Arc::new(super::Switch));
        self.0.checkbox = true;
        self
    }

    /// Whether the control is the built-in checkbox, which a condition follows through `checked`.
    pub(crate) fn is_checkbox(&self) -> bool {
        self.0.is_checkbox()
    }

    /// Refuse a submission that leaves the checkbox unchecked, and render it `required`: the box
    /// accepting the terms an action's input asks for. Only the built-in checkbox of
    /// [`Field::toggle`] and [`Field::toggle_input`] takes it, drawn as a box or a
    /// [`switch`](Self::switch); another control fails the mount, as
    /// does a [`disabled`](Self::disabled) one with no `true` default. A hidden field is not
    /// checked, nor a value the record already stores.
    ///
    /// ```rust
    /// # use tablo_core::Field;
    /// Field::toggle_input("terms")
    ///     .label("I accept the terms")
    ///     .accepted();
    /// ```
    pub fn accepted(mut self) -> Self {
        self.0.accepted = true;
        self
    }
}

common_modifiers!(TextField);
common_modifiers!(RichTextField);
common_modifiers!(ChoiceField);
common_modifiers!(FileField);
common_modifiers!(CustomField);
base_modifiers!(RepeaterField);

/// `choice`, on every builder but the choice's own.
macro_rules! to_choice {
    ($($builder:ident),*) => {$(
        impl<F> $builder<F> {
            /// Renders the field as a choice instead, as [`Field::choice`] does, ready for
            /// [`options`](ChoiceField::options) or [`relationship`](ChoiceField::relationship).
            /// The replaced control's own modifiers are dropped with it.
            pub fn choice(mut self) -> ChoiceField<F> {
                self.0.control = ControlKind::Choice(ChoiceControl::default());
                self.0.checkbox = false;
                ChoiceField(self.0, PhantomData)
            }
        }
    )*};
}

to_choice!(TextField, FileField, CustomField);

impl<F> TextField<F> {
    /// Renders the field as a [rich text](Field::rich_text) editor instead, storing HTML. The
    /// field's type must be `String` or `Option<String>`, else mounting the panel fails, and the
    /// text control's own modifiers are dropped.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, body: String }
    /// # #[derive(tablo_core::RecordForm)]
    /// # #[form(model = Post)]
    /// # struct PostForm { body: String }
    /// use tablo_core::Tool;
    ///
    /// let c = PostForm::controls();
    /// c.body
    ///     .rich_text()
    ///     .toolbar(&[&[Tool::Bold, Tool::Italic], &[Tool::BulletList]]);
    /// ```
    pub fn rich_text(mut self) -> RichTextField<F> {
        let input = self.text().input_type();
        self.0.control = ControlKind::RichText(RichTextControl {
            non_text: (input != "text").then_some(input),
            ..RichTextControl::default()
        });
        RichTextField(self.0, PhantomData)
    }

    fn text(&mut self) -> &mut TextControl {
        match &mut self.0.control {
            ControlKind::Text(text) => text,
            _ => unreachable!("a TextField holds a text control"),
        }
    }

    /// Validate the value as an email address, and render `type="email"`: the domain needs two
    /// labels (`a@b` is refused), a display name (`Ada <ada@example.com>`) is refused, and the
    /// address is capped at 254 bytes.
    pub fn email(mut self) -> Self {
        self.text().kind = Kind::Email;
        self
    }

    /// Validate the value as an absolute `http` or `https` URL naming a host, and render
    /// `type="url"`. The browser's own check takes any scheme, so the server refuses the others,
    /// such as `javascript:`, which a link to the value would run.
    pub fn url(mut self) -> Self {
        self.text().kind = Kind::Url;
        self
    }

    /// Render `type="tel"`, which brings up a phone keypad. No format is checked, since numbers
    /// vary by country: add a [`pattern`](Self::pattern) to require one.
    pub fn tel(mut self) -> Self {
        self.text().kind = Kind::Tel;
        self
    }

    /// Render `type="password"`: the browser masks the value, a button beside it shows what was
    /// typed, and a refused form renders the control empty rather than echoing the password back
    /// into the page. It renders the one-line
    /// input even when [`multiline`](Self::multiline).
    pub fn password(mut self) -> Self {
        self.text().password = true;
        self
    }

    /// A [`password`](Self::password) the user signs in with today, as a profile's current
    /// password: the control asks a password manager to fill the stored one.
    pub(crate) fn current_password(mut self) -> Self {
        let text = self.text();
        text.password = true;
        text.current_password = true;
        self
    }

    /// Probes a unique index before the write, and reports a taken value inline. A text field over
    /// a column with a unique index probes without it; mounting refuses it on a column no unique
    /// index covers, and on a non-nullable column whose record-form field has a blank answer,
    /// since every empty submission would store one value.
    ///
    /// The probe runs before the write, so two concurrent submissions can both pass it; the index
    /// stays the final check. When a write fails on a database error, the form probes again once
    /// the transaction is rolled back, so the losing submission re-renders with the same inline
    /// error, and a failure the probe does not explain answers 500. That needs the database to
    /// refuse the losing write once the winning one commits, as PostgreSQL and MySQL do; SQLite
    /// can refuse it as busy before then, and Turso's concurrent transactions conflict at commit,
    /// so either can still answer 500. The probe reads through the resource's scoped query, so an
    /// index broader than the scope, such as one wider than the tenancy, also answers 500.
    pub fn unique(mut self) -> Self {
        self.text().unique = true;
        self
    }

    /// The control's placeholder text.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.text().placeholder = Some(placeholder.into());
        self
    }

    /// Text shown before the one-line input, joined to it, such as a currency: `.prefix("$")`. It
    /// is not part of the value, and a [`multiline`](Self::multiline) textarea renders none.
    pub fn prefix(mut self, text: impl Into<String>) -> Self {
        self.text().prefix = Some(text.into());
        self
    }

    /// Text shown after the one-line input, joined to it, such as a unit: `.suffix("kg")`. Like a
    /// [`prefix`](Self::prefix), it is not part of the value.
    pub fn suffix(mut self, text: impl Into<String>) -> Self {
        self.text().suffix = Some(text.into());
        self
    }

    /// Values the one-line input suggests as the user types, through a `<datalist>`. The user may
    /// still enter any other value: use a [choice](Self::choice) to restrict them. A
    /// [`password`](Self::password) suggests none.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Product { #[key] #[auto] id: uuid::Uuid, weight: i64, brand: String }
    /// # use tablo_core::{Field, lens};
    /// Field::text(lens!(Product.weight)).suffix("kg");
    /// Field::text(lens!(Product.brand)).suggestions(["Acme", "Globex", "Initech"]);
    /// ```
    pub fn suggestions(mut self, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.text().suggestions = values.into_iter().map(Into::into).collect();
        self
    }

    /// Render a `<textarea>` of `rows` lines rather than a one-line input,
    /// for a column holding prose.
    pub fn multiline(mut self, rows: u32) -> Self {
        self.text().rows = Some(rows);
        self
    }

    /// Refuse a value of fewer than `chars` characters, and render `minlength`.
    ///
    /// Like every rule here, the browser checks it before it submits, and the server checks it
    /// again on the trimmed value once the field's type parses it. A blank value is the field's
    /// blank answer, which no rule refuses, and a value the record already stores is not checked
    /// again. A length counts characters, where a browser counts UTF-16 units.
    pub fn min_length(mut self, chars: usize) -> Self {
        self.text().rules.min_length = Some(chars);
        self
    }

    /// Refuse a value of more than `chars` characters, and render `maxlength`, which stops the
    /// browser's input there.
    pub fn max_length(mut self, chars: usize) -> Self {
        self.text().rules.max_length = Some(chars);
        self
    }

    /// Refuse a value below `bound`, and render `min`.
    ///
    /// The bound is a value of the field's type, which orders: a number, or a `jiff` date, time,
    /// date and time, or timestamp. `.min(0)` on an `i64` field, `.min(date(2024, 1, 1))` on a
    /// `jiff::civil::Date` one; a bound the type does not parse or order fails the mount. A time's
    /// control edits it to the minute, so give a time bound in whole minutes.
    pub fn min(mut self, bound: impl std::fmt::Display) -> Self {
        self.text().rules.min = Some(bound.to_string());
        self
    }

    /// Refuse a value above `bound`, and render `max`; [`min`](Self::min) says which bounds a
    /// field takes.
    pub fn max(mut self, bound: impl std::fmt::Display) -> Self {
        self.text().rules.max = Some(bound.to_string());
        self
    }

    /// Refuse a value the regular expression `pattern` does not match whole, and render it as
    /// the `pattern` attribute.
    ///
    /// The server matches it with the `regex` crate and the browser as a JavaScript expression in
    /// its `v` mode, both against the whole value, so write it in the syntax they share: classes,
    /// groups, alternation and repetition. Spell digits `[0-9]`, since the server's `\d` matches
    /// other scripts' digits too; escape a `-` inside a class, which the browser otherwise refuses
    /// along with the whole pattern; and leave out lookaround, backreferences and inline flags.
    /// One the `regex` crate does not compile fails the mount. A `.multiline` textarea renders no
    /// `pattern`, so only the server checks it there.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Product { #[key] #[auto] id: uuid::Uuid, sku: String }
    /// # use tablo_core::{Field, lens};
    /// Field::text(lens!(Product.sku)).pattern("[A-Z]{3}-[0-9]{4}");
    /// ```
    pub fn pattern(mut self, pattern: impl Into<String>) -> Self {
        self.text().rules.pattern = Some(Pattern::new(pattern.into()));
        self
    }
}

impl<F> RepeaterField<F> {
    fn repeater(&mut self) -> &mut RepeaterControl {
        match &mut self.0.control {
            ControlKind::Repeater(repeater) => repeater,
            _ => unreachable!("a RepeaterField holds a repeater control"),
        }
    }

    /// The add button's label, over "Add item".
    pub fn add_label(mut self, label: impl Into<String>) -> Self {
        self.repeater().add_label = Some(label.into());
        self
    }

    /// Refuse a submission of fewer than `count` rows, and disable each row's Remove button while
    /// the list holds no more. A minimum above `max_items` fails the mount.
    pub fn min_items(mut self, count: usize) -> Self {
        self.repeater().bounds.min = Some(count);
        self
    }

    /// Refuse a submission of more than `count` rows, and disable the add button while the list
    /// holds that many. A count above [`MAX_REPEATER_ROWS`](crate::MAX_REPEATER_ROWS), which
    /// every repeater keeps to, fails the mount.
    pub fn max_items(mut self, count: usize) -> Self {
        self.repeater().bounds.max = Some(count);
        self
    }
}

mod into;

/// The crate's tests read a builder's field as the schema will hold it.
#[cfg(test)]
mod test_deref {
    use super::{ChoiceField, CustomField, Field, FileField, RepeaterField, TextField};

    macro_rules! deref_field {
        ($($builder:ident),*) => {$(
            impl<F> std::ops::Deref for $builder<F> {
                type Target = Field;
                fn deref(&self) -> &Field {
                    &self.0
                }
            }
        )*};
    }

    deref_field!(
        TextField,
        ChoiceField,
        FileField,
        CustomField,
        RepeaterField
    );
}
