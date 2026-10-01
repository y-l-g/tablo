//! The people who sign in to the panel: the showcase's own user table, with
//! a seat in each blog (tenant) they work on.
//!
//! [`StaffAuth`] is the one [`Authenticator`] the panel installs. It loads a
//! [`Staff`] row with its seats into a [`SignedStaff`], the panel's user
//! type, which app code reads back with `auth::user::<SignedStaff>(cx)`.

use jiff::Timestamp;
use tablo_core::{
    Membership, PanelUser,
    auth::{Authenticator, verify_password},
};
use toasty::Db;
use topcoat::context::Cx;
use uuid::Uuid;

/// A member of staff: the credentials the login page checks.
#[derive(Debug, Clone, toasty::Model)]
pub struct Staff {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub email: String,
    /// Argon2id password hash in PHC string format.
    pub password_hash: String,
    pub display_name: String,
    /// `false` denies login and panel access immediately.
    pub active: bool,
    pub created_at: Timestamp,
}

/// A blog: the showcase's tenant. Its id is the `tenant_id` every
/// tenant-owned row carries.
#[derive(Debug, Clone, toasty::Model)]
pub struct Workspace {
    #[key]
    pub id: Uuid,
    pub name: String,
}

/// A member of staff's seat in one workspace.
#[derive(Debug, Clone, toasty::Model)]
pub struct Seat {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub staff_id: Uuid,
    pub workspace_id: Uuid,
}

/// The signed-in member of staff: the row and the workspaces they hold a
/// seat in.
#[derive(Debug)]
pub struct SignedStaff {
    pub staff: Staff,
    pub workspaces: Vec<Membership>,
}

impl PanelUser for SignedStaff {
    fn user_id(&self) -> String {
        self.staff.id.to_string()
    }

    fn display_name(&self) -> &str {
        &self.staff.display_name
    }

    fn can_access_panel(&self) -> bool {
        self.staff.active
    }

    fn tenants(&self) -> &[Membership] {
        &self.workspaces
    }
}

/// Signs staff in by email and password and loads their seats.
pub struct StaffAuth;

impl StaffAuth {
    /// `staff` with the workspaces they hold a seat in, ordered by name.
    async fn signed(cx: &Cx, staff: Staff) -> topcoat::Result<SignedStaff> {
        let mut db = tablo_core::db::db(cx);
        let seats: Vec<Uuid> = Seat::filter(Seat::fields().staff_id().eq(staff.id))
            .exec(&mut db)
            .await?
            .into_iter()
            .map(|seat| seat.workspace_id)
            .collect();
        let mut workspaces: Vec<Membership> =
            Workspace::filter(Workspace::fields().id().in_list(seats))
                .exec(&mut db)
                .await?
                .into_iter()
                .map(|workspace| Membership::new(workspace.id, workspace.name))
                .collect();
        workspaces.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(SignedStaff { staff, workspaces })
    }
}

impl Authenticator for StaffAuth {
    type User = SignedStaff;

    async fn verify(
        &self,
        cx: &Cx,
        login: &str,
        password: &str,
    ) -> topcoat::Result<Option<SignedStaff>> {
        let mut db = tablo_core::db::db(cx);
        let staff = Staff::filter(Staff::fields().email().eq(login.to_string()))
            .first()
            .exec(&mut db)
            .await?;
        if !verify_password(password, staff.as_ref().map(|s| s.password_hash.as_str())) {
            return Ok(None);
        }
        match staff {
            Some(staff) => Ok(Some(Self::signed(cx, staff).await?)),
            None => Ok(None),
        }
    }

    async fn find_by_id(&self, cx: &Cx, id: &str) -> topcoat::Result<Option<SignedStaff>> {
        let Ok(id) = Uuid::parse_str(id) else {
            return Ok(None);
        };
        let mut db = tablo_core::db::db(cx);
        let staff = Staff::filter(Staff::fields().id().eq(id))
            .first()
            .exec(&mut db)
            .await?;
        // A deactivated member of staff stops resolving: their sessions are
        // purged and the next request redirects to login.
        match staff.filter(|staff| staff.active) {
            Some(staff) => Ok(Some(Self::signed(cx, staff).await?)),
            None => Ok(None),
        }
    }
}

/// Create an active member of staff with an Argon2id-hashed password and a
/// seat in each of `workspaces`. Used by the showcase seed and the tenancy
/// test fixtures.
pub async fn create_staff(
    db: &mut Db,
    email: &str,
    display_name: &str,
    password: &str,
    workspaces: &[Uuid],
) -> toasty::Result<Staff> {
    let staff = toasty::create!(Staff {
        email: email.to_string(),
        password_hash: crate::seed::memoized_password_hash(password),
        display_name: display_name.to_string(),
        active: true,
        created_at: Timestamp::now(),
    })
    .exec(db)
    .await?;
    for workspace in workspaces {
        toasty::create!(Seat {
            staff_id: staff.id,
            workspace_id: *workspace,
        })
        .exec(db)
        .await?;
    }
    Ok(staff)
}

/// Create the workspace `id` named `name`, unless it exists.
pub async fn ensure_workspace(db: &mut Db, id: Uuid, name: &str) -> toasty::Result<()> {
    if Workspace::filter(Workspace::fields().id().eq(id))
        .first()
        .exec(db)
        .await?
        .is_none()
    {
        toasty::create!(Workspace {
            id,
            name: name.to_string(),
        })
        .exec(db)
        .await?;
    }
    Ok(())
}
