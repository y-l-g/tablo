use topcoat::context::CxTestBuilder;

use super::*;
use crate::{Resource, ResourceDef, TenantId, can, can_list, lens, test_support::tableless_db};

struct Post {
    locked: bool,
}

const ABILITIES: usize = 12;

/// Every ability over `post`, in declaration order.
fn abilities(post: &Post) -> [Ability<'_, Post>; ABILITIES] {
    [
        Ability::ViewAny,
        Ability::View(post),
        Ability::Create,
        Ability::Update(post),
        Ability::DeleteAny,
        Ability::Delete(post),
        Ability::RestoreAny,
        Ability::Restore(post),
        Ability::ForceDeleteAny,
        Ability::ForceDelete(post),
        Ability::RunAny { action: "lock" },
        Ability::Run {
            action: "lock",
            record: post,
        },
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
        [
            true, true, false, false, false, false, false, false, false, false, false, false
        ]
    );
}

#[test]
fn a_closure_matches_on_the_ability_and_reads_the_record() {
    let cx = CxTestBuilder::new().build();
    let unlocked = |_cx: &Cx, ability: Ability<'_, Post>| match ability {
        Ability::Update(post) | Ability::Delete(post) | Ability::Run { record: post, .. } => {
            !post.locked
        }
        _ => true,
    };
    assert_eq!(
        answers(&unlocked, &cx, &Post { locked: true }),
        [
            true, true, true, false, true, false, true, true, true, true, true, false
        ]
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
        [
            true, true, true, false, false, false, false, false, false, false, false, false
        ]
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

#[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
struct Note {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: TenantId,
    title: String,
}

fn note_table() -> crate::table::Table<Note> {
    crate::table::Table::new(crate::TextColumn::new(lens!(Note.title)))
}

struct OpenNotes;
impl Resource for OpenNotes {
    type Model = Note;
    type Form = crate::NoForm<Note>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(ReadOnly)
            .tenancy(crate::Tenancy::none())
            .table(note_table())
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
        ResourceDef::new()
            .tenancy(crate::Tenancy::none())
            .table(note_table())
    }
}

/// `can_list` answers what the list handler's gate and `ViewAny` answer: the
/// policy, then a tenant for a tenant-scoped resource.
#[tokio::test]
async fn can_list_checks_the_policy_and_the_tenant() {
    let db = tableless_db(toasty::models!(Note)).await;
    let anonymous = crate::Panel::new("admin")
        .resource::<OpenNotes>()
        .resource::<TenantNotes>()
        .resource::<ClosedNotes>()
        .handle(&db)
        .expect("panel builds")
        .context();
    let tenanted = anonymous.with(crate::Tenant(uuid::Uuid::new_v4().into()));
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

struct KeptNotes;
impl Resource for KeptNotes {
    type Model = Note;
    type Form = crate::NoForm<Note>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(|_cx: &Cx, ability: Ability<'_, Note>| {
                !matches!(ability, Ability::DeleteAny | Ability::Delete(_))
            })
            .tenancy(crate::Tenancy::none())
            .table(note_table())
    }
}

/// A policy refusing a delete refuses the force delete its catch-all arm would allow.
#[tokio::test]
async fn a_force_delete_needs_the_delete() {
    let db = tableless_db(toasty::models!(Note)).await;
    let cx = crate::Panel::new("admin")
        .resource::<KeptNotes>()
        .handle(&db)
        .expect("panel builds")
        .context();
    let note = Note {
        id: uuid::Uuid::new_v4(),
        tenant_id: TenantId::from(uuid::Uuid::new_v4()),
        title: "kept".into(),
    };
    assert!(!can::<KeptNotes>(&cx, Ability::ForceDeleteAny));
    assert!(!can::<KeptNotes>(&cx, Ability::ForceDelete(&note)));
    assert!(can::<KeptNotes>(&cx, Ability::Restore(&note)));
}

#[test]
fn written_record_names_the_record_of_every_write() {
    let post = Post { locked: false };
    let written = abilities(&post).map(|ability| ability.written_record().is_some());
    assert_eq!(
        written,
        [
            false, false, false, true, false, true, false, true, false, true, false, true
        ]
    );
    assert!(
        Ability::<Post>::RunHeader { action: "lock" }
            .written_record()
            .is_none()
    );
}

#[test]
fn abilities_deny_what_they_do_not_declare() {
    let cx = CxTestBuilder::new().build();
    let post = Post { locked: false };
    assert_eq!(
        answers(&Abilities::<Post>::new(), &cx, &post),
        [false; ABILITIES]
    );
    // `view` alone allows no list: the list asks `ViewAny` and shows every row of the query.
    let read_and_create = Abilities::<Post>::new()
        .view(|_cx, _post| true)
        .create(|_cx| true);
    assert_eq!(
        answers(&read_and_create, &cx, &post),
        [
            false, true, true, false, false, false, false, false, false, false, false, false
        ]
    );
}

#[test]
fn a_record_check_allows_the_ability_at_all_and_an_any_check_narrows_it() {
    let cx = CxTestBuilder::new().build();
    let unlocked = |_cx: &Cx, post: &Post| !post.locked;
    let policy = Abilities::<Post>::new()
        .view(unlocked)
        .view_any(|_cx| false)
        .update(unlocked)
        .delete(unlocked)
        .restore_any(|_cx| true)
        .force_delete(unlocked);
    assert_eq!(
        answers(&policy, &cx, &Post { locked: false }),
        [
            false, true, false, true, true, true, true, false, true, true, false, false
        ]
    );
    assert_eq!(
        answers(&policy, &cx, &Post { locked: true }),
        [
            false, false, false, false, true, false, true, false, true, false, false, false
        ]
    );
}

struct Lock;
impl crate::Action<OpenNotes> for Lock {
    type Input = ();
    const NAME: &'static str = "lock";

    async fn run(
        _cx: &Cx,
        _notes: &[Note],
        _input: (),
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<crate::Outcome, crate::WriteError> {
        Ok(crate::Outcome::done())
    }
}

struct Archive;
impl crate::Action<OpenNotes> for Archive {
    type Input = ();
    const NAME: &'static str = "archive";

    async fn run(
        _cx: &Cx,
        _notes: &[Note],
        _input: (),
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<crate::Outcome, crate::WriteError> {
        Ok(crate::Outcome::done())
    }
}

struct Recount;
impl crate::HeaderAction for Recount {
    type Input = ();
    const NAME: &'static str = "recount";

    async fn run(
        _cx: &Cx,
        _input: (),
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<crate::Outcome, crate::WriteError> {
        Ok(crate::Outcome::done())
    }
}

#[test]
fn abilities_answer_each_action_by_its_name() {
    let cx = CxTestBuilder::new().build();
    let note = |title: &str| Note {
        id: uuid::Uuid::new_v4(),
        tenant_id: TenantId::from(uuid::Uuid::new_v4()),
        title: title.into(),
    };
    let (open, kept) = (note("open"), note("kept"));
    let policy = Abilities::<Note>::new()
        .run::<Lock, _>(|_cx, note| note.title != "kept")
        .run_any::<Archive, _>(|_cx| true)
        .run_header::<Recount>(|_cx| true);
    let allows = |ability| policy.allows(&cx, ability);
    assert!(allows(Ability::RunAny { action: "lock" }));
    assert!(allows(Ability::Run {
        action: "lock",
        record: &open
    }));
    assert!(!allows(Ability::Run {
        action: "lock",
        record: &kept
    }));
    assert!(
        allows(Ability::RunAny { action: "archive" }),
        "run_any alone allows the action at all"
    );
    assert!(
        !allows(Ability::Run {
            action: "archive",
            record: &open
        }),
        "and no record"
    );
    assert!(!allows(Ability::RunAny { action: "publish" }));
    assert!(allows(Ability::RunHeader { action: "recount" }));
    assert!(!allows(Ability::RunHeader { action: "lock" }));
}
