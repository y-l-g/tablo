use topcoat::context::CxTestBuilder;

use super::*;
use crate::{Resource, ResourceDef, TenantId, can, can_list, lens};

struct Post {
    locked: bool,
}

const ABILITIES: usize = 6;

/// Every ability over `post`, in declaration order.
fn abilities(post: &Post) -> [Ability<'_, Post>; ABILITIES] {
    [
        Ability::ViewAny,
        Ability::View(post),
        Ability::Create,
        Ability::Update(post),
        Ability::DeleteAny,
        Ability::Delete(post),
    ]
}

fn answers(policy: &impl Policy<Post>, cx: &Cx, post: &Post) -> [bool; ABILITIES] {
    abilities(post).map(|ability| policy.allows(cx, ability))
}

#[test]
fn the_building_blocks_answer_per_ability() {
    let cx = CxTestBuilder::new().build();
    let post = Post { locked: false };
    assert_eq!(answers(&Allow, &cx, &post), [true; ABILITIES]);
    assert_eq!(answers(&Deny, &cx, &post), [false; ABILITIES]);
    assert_eq!(
        answers(&ReadOnly, &cx, &post),
        [true, true, false, false, false, false]
    );
}

#[test]
fn a_closure_matches_on_the_ability_and_reads_the_record() {
    let cx = CxTestBuilder::new().build();
    let unlocked = |_cx: &Cx, ability: Ability<'_, Post>| match ability {
        Ability::Update(post) | Ability::Delete(post) => !post.locked,
        _ => true,
    };
    assert_eq!(
        answers(&unlocked, &cx, &Post { locked: true }),
        [true, true, true, false, true, false]
    );
    assert_eq!(
        answers(&unlocked, &cx, &Post { locked: false }),
        [true; ABILITIES]
    );
}

#[test]
fn combinators_compose_policies() {
    let cx = CxTestBuilder::new().build();
    let post = Post { locked: false };
    assert_eq!(
        answers(&ReadOnly.and(Allow), &cx, &post),
        answers(&ReadOnly, &cx, &post)
    );
    assert_eq!(answers(&ReadOnly.and(Deny), &cx, &post), [false; ABILITIES]);
    assert_eq!(answers(&ReadOnly.or(Allow), &cx, &post), [true; ABILITIES]);
    let create_only = |_cx: &Cx, ability: Ability<'_, Post>| matches!(ability, Ability::Create);
    assert_eq!(
        answers(&ReadOnly.or(create_only), &cx, &post),
        [true, true, true, false, false, false]
    );
}

#[test]
fn when_gates_every_ability_on_the_request() {
    #[derive(Clone, Copy)]
    struct Staff;
    let staff = CxTestBuilder::new().request_context(Staff).build();
    let visitor = CxTestBuilder::new().build();
    let policy = Allow.and(when(|cx: &Cx| {
        topcoat::context::try_request_context::<Staff>(cx).is_some()
    }));
    let post = Post { locked: false };
    assert_eq!(answers(&policy, &staff, &post), [true; ABILITIES]);
    assert_eq!(answers(&policy, &visitor, &post), [false; ABILITIES]);
}

#[test]
fn ability_names_its_record() {
    let post = Post { locked: true };
    let named: Vec<bool> = abilities(&post)
        .into_iter()
        .map(|ability| ability.record().is_some_and(|record| record.locked))
        .collect();
    assert_eq!(named, [false, true, false, true, false, true]);
    let reads: Vec<bool> = abilities(&post).into_iter().map(Ability::is_read).collect();
    assert_eq!(reads, [true, true, false, false, false, false]);
}

#[derive(Debug, Clone, toasty::Model)]
struct Note {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: TenantId,
    title: String,
}

fn note_table() -> crate::table::Table<Note> {
    crate::table::Table::new(crate::table::TextColumn::new(lens!(Note.title)))
}

struct OpenNotes;
impl Resource for OpenNotes {
    type Model = Note;
    type Form = crate::NoForm<Note>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().policy(ReadOnly).table(note_table())
    }
}

struct TenantNotes;
impl Resource for TenantNotes {
    type Model = Note;
    type Form = crate::NoForm<Note>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(ReadOnly)
            .tenancy(crate::Tenancy::column(Note::fields().tenant_id()))
            .table(note_table())
    }
}

struct ClosedNotes;
impl Resource for ClosedNotes {
    type Model = Note;
    type Form = crate::NoForm<Note>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().table(note_table())
    }
}

/// `can_list` answers what the list handler's gate and `ViewAny` answer: the
/// policy, then a tenant for a tenant-scoped resource.
#[tokio::test]
async fn can_list_checks_the_policy_and_the_tenant() {
    let db = toasty::Db::builder()
        .models(toasty::models!(Note))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let anonymous = crate::Panel::new("admin")
        .resource::<OpenNotes>()
        .resource::<TenantNotes>()
        .resource::<ClosedNotes>()
        .context(&db)
        .expect("panel builds");
    let tenanted = anonymous.with(crate::Tenant(uuid::Uuid::new_v4()));
    assert!(can_list::<OpenNotes>(&anonymous));
    assert!(
        !can_list::<ClosedNotes>(&tenanted),
        "the default policy denies"
    );
    assert!(
        !can_list::<TenantNotes>(&anonymous),
        "a scoped resource needs a tenant"
    );
    assert!(can_list::<TenantNotes>(&tenanted));
    assert!(
        can::<TenantNotes>(&anonymous, Ability::ViewAny),
        "`can` asks the policy alone"
    );
}
