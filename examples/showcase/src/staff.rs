//! The people who sign in to the panel.

use jiff::Timestamp;
use tablo::{
    FieldErrors, FormInput, Membership, PanelUser,
    auth::{
        Authenticator, EmailVerifier, PasswordResetter, Profile, ProfileEditor, ProfileUpdate,
        Registrar, hash_password, new_password_errors, verify_password,
    },
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
    /// When the member of staff opened the link mailed to `email`.
    pub email_verified_at: Option<Timestamp>,
    pub created_at: Timestamp,
}

/// A blog: the showcase's tenant.
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

    fn subtitle(&self) -> Option<&str> {
        Some(&self.staff.email)
    }

    fn email(&self) -> Option<&str> {
        Some(&self.staff.email)
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
    async fn signed(cx: &Cx, staff: Staff) -> topcoat::Result<SignedStaff> {
        let mut db = tablo::db::db(cx);
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
        let mut db = tablo::db::db(cx);
        let staff = Staff::filter(Staff::fields().email().eq(login.to_string()))
            .first()
            .exec(&mut db)
            .await?;
        if !verify_password(password, staff.as_ref().map(|s| s.password_hash.as_str())).await {
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
        let mut db = tablo::db::db(cx);
        let staff = Staff::filter(Staff::fields().id().eq(id))
            .first()
            .exec(&mut db)
            .await?;
        // Deactivated staff stop resolving.
        match staff.filter(|staff| staff.active) {
            Some(staff) => Ok(Some(Self::signed(cx, staff).await?)),
            None => Ok(None),
        }
    }
}

/// What the sign-up page asks a new member of staff for: their account, and the blog they start.
#[derive(FormInput)]
pub struct StaffSignUp {
    #[form(label = "Your name")]
    pub display_name: String,
    #[form(email)]
    pub email: String,
    #[form(label = "Blog name", placeholder = "My blog")]
    pub workspace: String,
    #[form(password)]
    pub password: String,
    #[form(password, label = "Confirm password")]
    pub password_confirmation: String,
}

/// Signs up a member of staff with a seat in a new workspace of their own.
impl Registrar for StaffAuth {
    type Input = StaffSignUp;
    type User = SignedStaff;

    async fn validate(
        &self,
        _cx: &Cx,
        sign_up: &StaffSignUp,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<FieldErrors> {
        let mut errors = new_password_errors(&sign_up.password, &sign_up.password_confirmation);
        let taken = Staff::filter(Staff::fields().email().eq(sign_up.email.clone()))
            .first()
            .exec(ex)
            .await?;
        if taken.is_some() {
            errors.add("email", "Email has already been taken");
        }
        Ok(errors)
    }

    async fn register(
        &self,
        _cx: &Cx,
        sign_up: StaffSignUp,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<SignedStaff> {
        let staff = toasty::create!(Staff {
            email: sign_up.email,
            password_hash: hash_password(&sign_up.password).await?,
            display_name: sign_up.display_name,
            active: true,
            email_verified_at: None,
            created_at: Timestamp::now(),
        })
        .exec(&mut *ex)
        .await?;
        let workspace = toasty::create!(Workspace {
            id: Uuid::new_v4(),
            name: sign_up.workspace,
        })
        .exec(&mut *ex)
        .await?;
        toasty::create!(Seat {
            staff_id: staff.id,
            workspace_id: workspace.id,
        })
        .exec(&mut *ex)
        .await?;
        // Built from the rows just written: the transaction holds the connection a reload
        // through `signed` would wait for.
        Ok(SignedStaff {
            staff,
            workspaces: vec![Membership::new(workspace.id, workspace.name)],
        })
    }
}

/// Lets a member of staff edit their name, email and password, asking for the current password
/// before the email or the password changes.
impl ProfileEditor for StaffAuth {
    type Input = Profile;
    type User = SignedStaff;

    fn fill(&self, _cx: &Cx, signed: &SignedStaff) -> Profile {
        Profile {
            name: signed.staff.display_name.clone(),
            email: signed.staff.email.clone(),
            password: None,
            password_confirmation: String::new(),
            current_password: None,
        }
    }

    async fn validate(
        &self,
        _cx: &Cx,
        signed: &SignedStaff,
        profile: &Profile,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<FieldErrors> {
        let staff = &signed.staff;
        let mut errors = profile.password_errors();
        let new_email = profile.email != staff.email;
        if new_email || profile.password.is_some() {
            let current = profile.current_password.as_deref().unwrap_or_default();
            if !verify_password(current, Some(&staff.password_hash)).await {
                errors.add("current_password", "The current password is incorrect");
            }
        }
        // Only the account's owner learns whether another account holds an email.
        if new_email && !errors.contains_key("current_password") {
            let taken = Staff::filter(Staff::fields().email().eq(profile.email.clone()))
                .first()
                .exec(ex)
                .await?;
            if taken.is_some() {
                errors.add("email", "Email has already been taken");
            }
        }
        Ok(errors)
    }

    async fn save(
        &self,
        _cx: &Cx,
        signed: &SignedStaff,
        profile: Profile,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<ProfileUpdate> {
        let new_email = profile.email != signed.staff.email;
        let credentials = new_email || profile.password.is_some();
        let mut update = Staff::filter(Staff::fields().id().eq(signed.staff.id))
            .update()
            .display_name(profile.name)
            .email(profile.email);
        if new_email {
            update.set_email_verified_at(None);
        }
        if let Some(password) = &profile.password {
            update.set_password_hash(hash_password(password).await?);
        }
        update.exec(ex).await?;
        Ok(if credentials {
            ProfileUpdate::Credentials
        } else {
            ProfileUpdate::Details
        })
    }
}

/// Mails a member of staff who forgot their password a link to choose a new one.
impl PasswordResetter for StaffAuth {
    type User = SignedStaff;

    async fn find_by_email(&self, cx: &Cx, email: &str) -> topcoat::Result<Option<SignedStaff>> {
        let mut db = tablo::db::db(cx);
        let staff = Staff::filter(Staff::fields().email().eq(email.to_string()))
            .first()
            .exec(&mut db)
            .await?;
        match staff {
            Some(staff) => Ok(Some(Self::signed(cx, staff).await?)),
            None => Ok(None),
        }
    }

    async fn reset_password(
        &self,
        _cx: &Cx,
        user_id: &str,
        password: &str,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        let Ok(id) = Uuid::parse_str(user_id) else {
            return Ok(());
        };
        Staff::filter(Staff::fields().id().eq(id))
            .update()
            .password_hash(hash_password(password).await?)
            .exec(ex)
            .await?;
        Ok(())
    }
}

/// Records the address a member of staff proved they read.
impl EmailVerifier for StaffAuth {
    type User = SignedStaff;

    fn is_verified(&self, signed: &SignedStaff) -> bool {
        signed.staff.email_verified_at.is_some()
    }

    async fn store_verified_email(
        &self,
        _cx: &Cx,
        user_id: &str,
        email: &str,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        let id = Uuid::parse_str(user_id).map_err(|_| topcoat::router::error::not_found())?;
        Staff::filter(Staff::fields().id().eq(id))
            .update()
            .email(email.to_string())
            .email_verified_at(Some(Timestamp::now()))
            .exec(ex)
            .await?;
        Ok(())
    }
}

/// Creates an active, verified member of staff with a seat in each of `workspaces`.
pub async fn create_staff(
    db: &mut Db,
    email: &str,
    display_name: &str,
    password: &str,
    workspaces: &[Uuid],
) -> toasty::Result<Staff> {
    let staff = toasty::create!(Staff {
        email: email.to_string(),
        password_hash: crate::seed::memoized_password_hash(password).await,
        display_name: display_name.to_string(),
        active: true,
        email_verified_at: Some(Timestamp::now()),
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
