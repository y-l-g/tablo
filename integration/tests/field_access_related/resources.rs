//! The resources the relation field-access tests mount: a restricting member resource, the
//! cards that show its records, and the declarations the mount refuses.

use tablo::{
    ComputedColumn, CountColumn, Detail, FieldAccess, NoForm, ReadOnly, Reads, RelationColumn,
    Resource, ResourceDef, Table, Tenancy, TextColumn, lens, relation,
};
use topcoat::context::Cx;

use super::{Card, Dossier, Holder, Member, Picker, Ticket, Warehouse, hr};

/// A label source that is not a resource: no panel mounts it, and it answers no field access.
pub(super) struct BareSource;

impl tablo::extend::OptionSource for BareSource {
    type Model = Member;

    fn scoped_query(_cx: &Cx) -> topcoat::Result<toasty::stmt::Query<toasty::stmt::List<Member>>> {
        Ok(toasty::stmt::Query::all())
    }

    fn label(_cx: &Cx, member: &Member) -> String {
        member.name.clone()
    }
}

/// Restricts a member's salary from every request but HR's.
pub(super) struct MemberResource;

impl Resource for MemberResource {
    type Model = Member;
    type Form = NoForm<Member>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("members")
            .policy(ReadOnly)
            .tenancy(Tenancy::column(Member::fields().tenant_id()))
            .field_access(lens!(Member.salary), FieldAccess::new().view(hr))
            .record_title(lens!(Member.name))
            .table(Table::new(TextColumn::new(lens!(Member.name))))
    }
}

/// The member's salary, which `MemberResource` restricts.
pub(super) fn pay() -> RelationColumn<Card> {
    RelationColumn::new::<MemberResource>(relation!(Card.member), lens!(Member.salary), |m| {
        m.salary.clone()
    })
    .label("Pay")
}

/// The member's name, which every user views.
pub(super) fn holder() -> RelationColumn<Card> {
    RelationColumn::new::<MemberResource>(relation!(Card.member), lens!(Member.name), |m| {
        m.name.clone()
    })
    .label("Holder")
}

pub(super) struct CardResource;

impl Resource for CardResource {
    type Model = Card;
    type Form = NoForm<Card>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("cards")
            .policy(ReadOnly)
            .tenancy(Tenancy::column(Card::fields().tenant_id()))
            .table(Table::new((
                TextColumn::new(lens!(Card.code)),
                holder(),
                pay(),
                RelationColumn::of::<MemberResource>(relation!(Card.member)),
            )))
            .detail(Detail::new((TextColumn::new(lens!(Card.code)), pay())))
    }
}

/// Every member's columns, with no field access of its own: a second resource over the model.
pub(super) struct OpenMember;

impl Resource for OpenMember {
    type Model = Member;
    type Form = NoForm<Member>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("open-members")
            .policy(ReadOnly)
            .tenancy(Tenancy::column(Member::fields().tenant_id()))
            .table(Table::new((
                TextColumn::new(lens!(Member.name)),
                TextColumn::new(lens!(Member.salary)),
            )))
    }
}

/// A card showing its member's salary through `OpenMember`, which restricts nothing.
pub(super) struct OpenCard;

impl Resource for OpenCard {
    type Model = Card;
    type Form = NoForm<Card>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("open-cards")
            .policy(ReadOnly)
            .tenancy(Tenancy::column(Card::fields().tenant_id()))
            .table(Table::new((
                TextColumn::new(lens!(Card.code)),
                RelationColumn::new::<OpenMember>(
                    relation!(Card.member),
                    lens!(Member.salary),
                    |m: &Member| m.salary.clone(),
                )
                .label("Pay"),
            )))
    }
}

/// A member titled by the salary a resource restricts.
pub(super) struct TitledMember;

impl Resource for TitledMember {
    type Model = Member;
    type Form = NoForm<Member>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("titled-members")
            .policy(ReadOnly)
            .tenancy(Tenancy::column(Member::fields().tenant_id()))
            .record_title(lens!(Member.salary))
            .table(Table::new(TextColumn::new(lens!(Member.name))))
    }
}

/// A member whose key a resource restricts: the key is the URL id every page names it by.
pub(super) struct KeyMember;

impl Resource for KeyMember {
    type Model = Member;
    type Form = NoForm<Member>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("key-members")
            .policy(ReadOnly)
            .tenancy(Tenancy::column(Member::fields().tenant_id()))
            .field_access(lens!(Member.id), FieldAccess::new().view(hr))
            .table(Table::new(TextColumn::new(lens!(Member.name))))
    }
}

/// A dossier whose computed column reads its member's salary through an embedded relation.
pub(super) struct DossierResource;

impl Resource for DossierResource {
    type Model = Dossier;
    type Form = NoForm<Dossier>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("dossiers")
            .policy(ReadOnly)
            .tenancy(Tenancy::column(Dossier::fields().tenant_id()))
            .table(Table::new((
                TextColumn::new(lens!(Dossier.code)),
                ComputedColumn::new("Leak", |d: &Dossier| {
                    d.assignment.member.get().salary.clone()
                })
                .include_unscoped(Dossier::fields().assignment().member()),
            )))
    }
}

/// A card reading its member's salary through the title column beside it, which loads the member
/// but masks nothing: the computed column reads the loaded member.
pub(super) struct SiblingCard;

impl Resource for SiblingCard {
    type Model = Card;
    type Form = NoForm<Card>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("sibling-cards")
            .policy(ReadOnly)
            .tenancy(Tenancy::column(Card::fields().tenant_id()))
            .table(Table::new((
                TextColumn::new(lens!(Card.code)),
                RelationColumn::of::<MemberResource>(relation!(Card.member)),
                ComputedColumn::new("Leak", |c: &Card| c.member.get().salary.clone())
                    .reading(Reads::of(Card::fields().member())),
            )))
    }
}

/// The same card table without the computed column: the mount accepts it.
pub(super) struct PlainCard;

impl Resource for PlainCard {
    type Model = Card;
    type Form = NoForm<Card>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("plain-cards")
            .policy(ReadOnly)
            .tenancy(Tenancy::column(Card::fields().tenant_id()))
            .table(Table::new((
                TextColumn::new(lens!(Card.code)),
                RelationColumn::of::<MemberResource>(relation!(Card.member)),
            )))
    }
}

/// Restricts a picker's salary from every request but HR's.
pub(super) struct PickerResource;

impl Resource for PickerResource {
    type Model = Picker;
    type Form = NoForm<Picker>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("pickers")
            .policy(ReadOnly)
            .tenancy(Tenancy::column(Picker::fields().tenant_id()))
            .field_access(lens!(Picker.salary), FieldAccess::new().view(hr))
            .table(Table::new(TextColumn::new(lens!(Picker.name))))
    }
}

/// A label source over pickers that no panel mounts.
pub(super) struct BarePicker;

impl tablo::extend::OptionSource for BarePicker {
    type Model = Picker;

    fn scoped_query(_cx: &Cx) -> topcoat::Result<toasty::stmt::Query<toasty::stmt::List<Picker>>> {
        Ok(toasty::stmt::Query::all())
    }

    fn label(_cx: &Cx, picker: &Picker) -> String {
        picker.name.clone()
    }
}

/// A count of a warehouse's pickers, scoped to a source no panel mounts.
pub(super) struct CountOfBare;

impl Resource for CountOfBare {
    type Model = Warehouse;
    type Form = NoForm<Warehouse>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("bare-warehouses")
            .policy(ReadOnly)
            .tenancy(Tenancy::column(Warehouse::fields().tenant_id()))
            .table(Table::new(CountColumn::of::<BarePicker>(relation!(
                Warehouse.pickers
            ))))
    }
}

/// A count of a warehouse's pickers, scoped to a mounted resource.
pub(super) struct CountOfResource;

impl Resource for CountOfResource {
    type Model = Warehouse;
    type Form = NoForm<Warehouse>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("resource-warehouses")
            .policy(ReadOnly)
            .tenancy(Tenancy::column(Warehouse::fields().tenant_id()))
            .table(Table::new(CountColumn::of::<PickerResource>(relation!(
                Warehouse.pickers
            ))))
    }
}

/// A ticket whose computed column reads the salary of the member one variant of its holder holds.
pub(super) struct TicketResource;

impl Resource for TicketResource {
    type Model = Ticket;
    type Form = NoForm<Ticket>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("tickets")
            .policy(ReadOnly)
            .tenancy(Tenancy::column(Ticket::fields().tenant_id()))
            .table(Table::new((
                TextColumn::new(lens!(Ticket.code)),
                ComputedColumn::new("Leak", |t: &Ticket| match &t.holder {
                    Holder::Assigned { member, .. } => member.get().salary.clone(),
                    Holder::Unassigned => String::new(),
                })
                .include_unscoped(Ticket::fields().holder().assigned().member()),
            )))
    }
}
