//! An app's own user table whose accounts hold seats in tenants, and a tenant-owned resource: what
//! the tenant registration and settings suites sign in and scope.

use tablo::{
    Auth, Authenticator, Membership, Panel, PanelUser, ReadOnly, Resource, ResourceDef,
    RouterBuilderPanelExt, Table, Tenancy, TenantId, TextColumn, auth::AuthSession, lens,
};
use toasty::Db;
use topcoat::{
    context::Cx,
    router::{Router, RouterBuilderDiscoverExt},
};
use uuid::Uuid;

use super::test_db;

/// An account of the app's own user table.
#[derive(Debug, Clone, toasty::Model)]
pub struct Member {
    #[key]
    #[auto]
    pub id: Uuid,
    pub name: String,
}

/// A member's seat in one tenant, the tenant's name with it.
#[derive(Debug, Clone, toasty::Model)]
pub struct Seat {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub member_id: Uuid,
    pub tenant: Uuid,
    pub tenant_name: String,
    /// Whether the member owns the tenant, and so may edit it.
    pub owner: bool,
}

/// The signed-in member, the tenants they hold a seat in, and the ones they own.
#[derive(Debug)]
pub struct SignedMember {
    pub member: Member,
    pub tenants: Vec<Membership>,
    pub owned: Vec<Uuid>,
}

impl PanelUser for SignedMember {
    fn user_id(&self) -> String {
        self.member.id.to_string()
    }

    fn display_name(&self) -> &str {
        &self.member.name
    }

    fn tenants(&self) -> &[Membership] {
        &self.tenants
    }
}

/// Loads members and their seats; signs nobody in by password, since the suites sign in directly.
pub struct MemberAuth;

impl Authenticator for MemberAuth {
    type User = SignedMember;

    async fn verify(
        &self,
        _cx: &Cx,
        _login: &str,
        _password: &str,
    ) -> topcoat::Result<Option<SignedMember>> {
        Ok(None)
    }

    async fn find_by_id(&self, cx: &Cx, id: &str) -> topcoat::Result<Option<SignedMember>> {
        let Ok(id) = Uuid::parse_str(id) else {
            return Ok(None);
        };
        let mut db = tablo::db::db(cx)?;
        let Some(member) = Member::filter(Member::fields().id().eq(id))
            .first()
            .exec(&mut db)
            .await?
        else {
            return Ok(None);
        };
        let seats = seats(&mut db, member.id).await?;
        Ok(Some(SignedMember {
            member,
            owned: seats
                .iter()
                .filter(|seat| seat.owner)
                .map(|seat| seat.tenant)
                .collect(),
            tenants: seats
                .into_iter()
                .map(|seat| Membership::new(seat.tenant, seat.tenant_name))
                .collect(),
        }))
    }
}

/// The seats `member` holds.
pub async fn seats(ex: &mut dyn toasty::Executor, member: Uuid) -> toasty::Result<Vec<Seat>> {
    Seat::filter(Seat::fields().member_id().eq(member))
        .exec(ex)
        .await
}

/// A tenant-owned note.
#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
pub struct Note {
    #[key]
    #[auto]
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub body: String,
}

pub struct NoteResource;

impl Resource for NoteResource {
    type Model = Note;
    type Form = tablo::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(ReadOnly)
            .tenancy(Tenancy::column(Note::fields().tenant_id()))
            .table(Table::new(TextColumn::new(lens!(Note.body))))
    }
}

pub async fn db() -> Db {
    test_db(toasty::models!(Member, Seat, Note, AuthSession)).await
}

/// The `admin` panel signing members in with `auth`, listing notes.
pub fn mount(db: Db, auth: Auth) -> topcoat::Result<Router> {
    Ok(Router::builder()
        .discover()
        .app_context(db)
        .panel(Panel::new("admin").auth(auth).resource::<NoteResource>())?
        .build())
}

/// A member named `name` who owns a tenant named after each of `tenants`.
pub async fn member(db: &Db, name: &str, tenants: &[&str]) -> SignedMember {
    let member = toasty::create!(Member {
        name: name.to_string(),
    })
    .exec(&mut db.clone())
    .await
    .unwrap();
    let mut signed = SignedMember {
        member,
        tenants: Vec::new(),
        owned: Vec::new(),
    };
    for tenant_name in tenants {
        let tenant = Uuid::new_v4();
        seat(db, &signed.member, tenant, tenant_name, true).await;
        signed.tenants.push(Membership::new(tenant, *tenant_name));
        signed.owned.push(tenant);
    }
    signed
}

/// Seats `member` in `tenant`, named `name`, as its owner or not.
pub async fn seat(db: &Db, member: &Member, tenant: Uuid, name: &str, owner: bool) {
    toasty::create!(Seat {
        member_id: member.id,
        tenant,
        tenant_name: name.to_string(),
        owner,
    })
    .exec(&mut db.clone())
    .await
    .unwrap();
}

/// The tenant the one session acts for.
pub async fn session_tenant(db: &Db) -> Option<Uuid> {
    let sessions = AuthSession::all().exec(&mut db.clone()).await.unwrap();
    assert_eq!(sessions.len(), 1);
    sessions[0].tenant
}
