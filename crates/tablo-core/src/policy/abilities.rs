//! [`Abilities`]: a policy declared one ability at a time, denying every ability it does not
//! declare.

use topcoat::context::Cx;

use super::{Ability, Policy};

/// A check on the request alone.
type RequestCheck = Box<dyn Fn(&Cx) -> bool + Send + Sync>;

/// A check on the request and one record.
type RecordCheck<M> = Box<dyn Fn(&Cx, &M) -> bool + Send + Sync>;

/// The two checks of an ability asked at all and then of each record, such as `DeleteAny` and
/// `Delete`.
struct Pair<M> {
    any: Option<RequestCheck>,
    record: Option<RecordCheck<M>>,
}

impl<M> Default for Pair<M> {
    fn default() -> Self {
        Self {
            any: None,
            record: None,
        }
    }
}

impl<M> Pair<M> {
    /// The `any` check, else allowed when a record check is declared: declaring who may delete
    /// a record lets the request delete at all.
    fn any_allows(&self, cx: &Cx) -> bool {
        match &self.any {
            Some(check) => check(cx),
            None => self.record.is_some(),
        }
    }

    fn record_allows(&self, cx: &Cx, record: &M) -> bool {
        self.record.as_ref().is_some_and(|check| check(cx, record))
    }
}

/// A policy that answers each [`Ability`] from the check declared for it, and denies every
/// ability it declares none for.
///
/// A closure policy that ends in `_ => true` allows whatever it does not name, such as the
/// restore a soft delete adds or an action declared later. `Abilities` allows only what it
/// names:
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, locked: bool }
/// # use tablo_core::Abilities;
/// # use topcoat::context::Cx;
/// # fn is_editor(_cx: &Cx) -> bool { true }
/// let policy = Abilities::<Post>::new()
///     .view_any(is_editor)
///     .view(|_cx, _post| true)
///     .create(is_editor)
///     .update(|cx, post| is_editor(cx) && !post.locked);
/// // No delete, restore or action: each is refused until the policy declares it.
/// # let _ = policy;
/// ```
///
/// A write's record check also allows the write at all: [`delete`](Self::delete) allows
/// [`DeleteAny`](Ability::DeleteAny), [`restore`](Self::restore) allows
/// [`RestoreAny`](Ability::RestoreAny), [`force_delete`](Self::force_delete) allows
/// [`ForceDeleteAny`](Ability::ForceDeleteAny), and [`run`](Self::run) allows
/// [`RunAny`](Ability::RunAny) for its action, since the panel asks the record's check before
/// each of these writes. The `_any` methods narrow it; declared alone, they allow no record.
/// [`view`](Self::view) does not allow [`ViewAny`](Ability::ViewAny): the list asks `ViewAny`
/// alone and shows every row the resource's query returns, so [`view_any`](Self::view_any) is
/// declared on its own.
///
/// Declaring an ability twice replaces its earlier check; combine the checks in one closure.
///
/// The model's type is named on `new`, `Abilities::<Post>::new()`: a closure is checked before
/// the chain learns the model from where the policy goes.
pub struct Abilities<M> {
    view_any: Option<RequestCheck>,
    view: Option<RecordCheck<M>>,
    create: Option<RequestCheck>,
    update: Option<RecordCheck<M>>,
    delete: Pair<M>,
    restore: Pair<M>,
    force_delete: Pair<M>,
    /// Each record action's checks, by its `NAME`.
    actions: Vec<(&'static str, Pair<M>)>,
    /// Each header action's check, by its `NAME`.
    header_actions: Vec<(&'static str, RequestCheck)>,
}

impl<M> Default for Abilities<M> {
    fn default() -> Self {
        Self {
            view_any: None,
            view: None,
            create: None,
            update: None,
            delete: Pair::default(),
            restore: Pair::default(),
            force_delete: Pair::default(),
            actions: Vec::new(),
            header_actions: Vec::new(),
        }
    }
}

impl<M> Abilities<M> {
    /// A policy that denies every ability.
    pub fn new() -> Self {
        Self::default()
    }

    /// Who may see the resource at all: [`ViewAny`](Ability::ViewAny), which opens the list
    /// with every row of the resource's query, the sidebar entry, the export, related tables,
    /// relationship options and header actions. Refused until declared, whether or not
    /// [`view`](Self::view) is.
    #[must_use]
    pub fn view_any(mut self, check: impl Fn(&Cx) -> bool + Send + Sync + 'static) -> Self {
        self.view_any = Some(Box::new(check));
        self
    }

    /// Who may see a record: [`View`](Ability::View), asked by its detail page, its edit form,
    /// each write to it, the export and relationship options. The list shows a row without it,
    /// and asks it only to offer the row's actions.
    #[must_use]
    pub fn view(mut self, check: impl Fn(&Cx, &M) -> bool + Send + Sync + 'static) -> Self {
        self.view = Some(Box::new(check));
        self
    }

    /// Who may create a record: [`Create`](Ability::Create).
    #[must_use]
    pub fn create(mut self, check: impl Fn(&Cx) -> bool + Send + Sync + 'static) -> Self {
        self.create = Some(Box::new(check));
        self
    }

    /// Who may edit a record: [`Update`](Ability::Update).
    #[must_use]
    pub fn update(mut self, check: impl Fn(&Cx, &M) -> bool + Send + Sync + 'static) -> Self {
        self.update = Some(Box::new(check));
        self
    }

    /// Who may delete at all: [`DeleteAny`](Ability::DeleteAny). Defaults to whether
    /// [`delete`](Self::delete) is declared.
    #[must_use]
    pub fn delete_any(mut self, check: impl Fn(&Cx) -> bool + Send + Sync + 'static) -> Self {
        self.delete.any = Some(Box::new(check));
        self
    }

    /// Who may delete a record: [`Delete`](Ability::Delete).
    #[must_use]
    pub fn delete(mut self, check: impl Fn(&Cx, &M) -> bool + Send + Sync + 'static) -> Self {
        self.delete.record = Some(Box::new(check));
        self
    }

    /// Who may restore trashed records at all: [`RestoreAny`](Ability::RestoreAny). Defaults to
    /// whether [`restore`](Self::restore) is declared.
    #[must_use]
    pub fn restore_any(mut self, check: impl Fn(&Cx) -> bool + Send + Sync + 'static) -> Self {
        self.restore.any = Some(Box::new(check));
        self
    }

    /// Who may restore a trashed record: [`Restore`](Ability::Restore).
    #[must_use]
    pub fn restore(mut self, check: impl Fn(&Cx, &M) -> bool + Send + Sync + 'static) -> Self {
        self.restore.record = Some(Box::new(check));
        self
    }

    /// Who may delete trashed records for good at all:
    /// [`ForceDeleteAny`](Ability::ForceDeleteAny), which the panel also asks `DeleteAny` of.
    /// Defaults to whether [`force_delete`](Self::force_delete) is declared.
    #[must_use]
    pub fn force_delete_any(mut self, check: impl Fn(&Cx) -> bool + Send + Sync + 'static) -> Self {
        self.force_delete.any = Some(Box::new(check));
        self
    }

    /// Who may delete a trashed record for good: [`ForceDelete`](Ability::ForceDelete), which
    /// the panel also asks [`Delete`](Ability::Delete) of.
    #[must_use]
    pub fn force_delete(mut self, check: impl Fn(&Cx, &M) -> bool + Send + Sync + 'static) -> Self {
        self.force_delete.record = Some(Box::new(check));
        self
    }

    /// The checks of the record action `name`, added empty when none is declared yet.
    fn action_mut(&mut self, name: &'static str) -> &mut Pair<M> {
        let index = match self.actions.iter().position(|(action, _)| *action == name) {
            Some(index) => index,
            None => {
                self.actions.push((name, Pair::default()));
                self.actions.len() - 1
            }
        };
        &mut self.actions[index].1
    }

    /// Declares the record action `name`'s check on the request.
    pub(crate) fn set_action_any(&mut self, name: &'static str, check: RequestCheck) {
        self.action_mut(name).any = Some(check);
    }

    /// Declares the record action `name`'s check on each record.
    pub(crate) fn set_action_record(&mut self, name: &'static str, check: RecordCheck<M>) {
        self.action_mut(name).record = Some(check);
    }

    /// Declares the header action `name`'s check, replacing an earlier one.
    pub(crate) fn set_header_action(&mut self, name: &'static str, check: RequestCheck) {
        self.header_actions.retain(|(action, _)| *action != name);
        self.header_actions.push((name, check));
    }

    fn action(&self, name: &str) -> Option<&Pair<M>> {
        self.actions
            .iter()
            .find(|(action, _)| *action == name)
            .map(|(_, pair)| pair)
    }
}

impl<M> std::fmt::Debug for Abilities<M> {
    /// Lists the declared abilities by method name; their checks are closures.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let declared = [
            ("view_any", self.view_any.is_some()),
            ("view", self.view.is_some()),
            ("create", self.create.is_some()),
            ("update", self.update.is_some()),
            ("delete_any", self.delete.any.is_some()),
            ("delete", self.delete.record.is_some()),
            ("restore_any", self.restore.any.is_some()),
            ("restore", self.restore.record.is_some()),
            ("force_delete_any", self.force_delete.any.is_some()),
            ("force_delete", self.force_delete.record.is_some()),
        ];
        f.debug_struct("Abilities")
            .field(
                "declared",
                &declared
                    .iter()
                    .filter_map(|(name, set)| set.then_some(*name))
                    .collect::<Vec<_>>(),
            )
            .field(
                "actions",
                &self
                    .actions
                    .iter()
                    .map(|(name, _)| name)
                    .collect::<Vec<_>>(),
            )
            .field(
                "header_actions",
                &self
                    .header_actions
                    .iter()
                    .map(|(name, _)| name)
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl<M: 'static> Policy<M> for Abilities<M> {
    fn allows(&self, cx: &Cx, ability: Ability<'_, M>) -> bool {
        // No catch-all arm: a new ability fails to compile here until it is given a check.
        match ability {
            Ability::ViewAny => self.view_any.as_ref().is_some_and(|check| check(cx)),
            Ability::View(record) => self.view.as_ref().is_some_and(|check| check(cx, record)),
            Ability::Create => self.create.as_ref().is_some_and(|check| check(cx)),
            Ability::Update(record) => self.update.as_ref().is_some_and(|check| check(cx, record)),
            Ability::DeleteAny => self.delete.any_allows(cx),
            Ability::Delete(record) => self.delete.record_allows(cx, record),
            Ability::RestoreAny => self.restore.any_allows(cx),
            Ability::Restore(record) => self.restore.record_allows(cx, record),
            Ability::ForceDeleteAny => self.force_delete.any_allows(cx),
            Ability::ForceDelete(record) => self.force_delete.record_allows(cx, record),
            Ability::RunAny { action } => self.action(action).is_some_and(|p| p.any_allows(cx)),
            Ability::Run { action, record } => self
                .action(action)
                .is_some_and(|pair| pair.record_allows(cx, record)),
            Ability::RunHeader { action } => self
                .header_actions
                .iter()
                .find(|(name, _)| *name == action)
                .is_some_and(|(_, check)| check(cx)),
        }
    }
}
