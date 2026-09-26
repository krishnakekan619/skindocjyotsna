//! End-to-end service flows against a real (in-memory) SQLite database.

use clinic_core::auth::Role;
use clinic_services::auth::{self, NewAccount, SetupInput};
use clinic_services::settings::{self, ClinicSettings};
use clinic_services::users::{self, UserUpdate};
use clinic_services::{ServiceError, Session, audit};
use clinic_sqlite::Database;

type TestResult = Result<(), ServiceError>;

const T0: i64 = 1_790_000_000;
const OWNER_PASSWORD: &str = "owner-pass-1";
const DOCTOR_PASSWORD: &str = "doctor-pass-1";
const RECEPTION_PASSWORD: &str = "front-desk-1";

fn account(username: &str, password: &str, pin: Option<&str>) -> NewAccount {
    NewAccount { username: username.into(), full_name: format!("{username} name"), password: password.into(), pin: pin.map(Into::into) }
}

fn clinic() -> ClinicSettings {
    ClinicSettings {
        name: " SkinDocJyotsna Clinic ".into(),
        address_lines: vec!["12 MG Road, Pune".into(), "   ".into()],
        phone: "020-12345678".into(),
        ..ClinicSettings::default()
    }
}

fn setup(db: &mut Database, second_admin: Option<NewAccount>) -> Result<Session, ServiceError> {
    auth::complete_setup(db, SetupInput { clinic: clinic(), admin: account("Owner", OWNER_PASSWORD, Some("2580")), second_admin }, T0)
}

fn actions(db: &Database, admin: &Session) -> Result<Vec<String>, ServiceError> {
    Ok(audit::list(db, admin, 200, None)?.into_iter().map(|e| e.action).collect())
}

fn add_receptionist(db: &mut Database, owner: &Session) -> Result<Session, ServiceError> {
    users::create(db, owner, account("reception", RECEPTION_PASSWORD, Some("4719")), Role::Receptionist, T0)?;
    auth::login(db, "reception", RECEPTION_PASSWORD, T0)
}

#[test]
fn setup_runs_once_and_creates_the_owner() -> TestResult {
    let mut db = Database::open_in_memory()?;
    assert!(auth::needs_setup(&db)?);

    let owner = setup(&mut db, None)?;
    assert_eq!((owner.username.as_str(), owner.role, owner.has_pin), ("owner", Role::Admin, true));
    assert!(!auth::needs_setup(&db)?);
    assert!(matches!(setup(&mut db, None), Err(ServiceError::SetupAlreadyDone)));

    let saved = settings::get_clinic(&db)?;
    assert_eq!(saved.name, "SkinDocJyotsna Clinic", "text is trimmed");
    assert_eq!(saved.address_lines, vec!["12 MG Road, Pune"], "empty lines dropped");
    assert_eq!(saved.idle_lock_minutes, 15);
    assert!(!users::has_backup_admin(&db)?, "only one admin: the UI shows a warning (DEC-025)");
    assert!(actions(&db, &owner)?.contains(&"SETUP_COMPLETE".to_string()));
    Ok(())
}

#[test]
fn invalid_setup_creates_nothing() -> TestResult {
    let mut db = Database::open_in_memory()?;
    let weak = auth::complete_setup(&mut db, SetupInput { clinic: clinic(), admin: account("owner", "short", None), second_admin: None }, T0);
    assert!(matches!(weak, Err(ServiceError::Validation { field: "password", .. })));

    let same_name = setup(&mut db, Some(account("OWNER", DOCTOR_PASSWORD, None)));
    assert!(matches!(same_name, Err(ServiceError::Validation { field: "secondAdmin.username", .. })));

    let no_clinic_name = auth::complete_setup(
        &mut db,
        SetupInput { clinic: ClinicSettings::default(), admin: account("owner", OWNER_PASSWORD, None), second_admin: None },
        T0,
    );
    assert!(matches!(no_clinic_name, Err(ServiceError::Validation { field: "name", .. })));
    assert!(auth::needs_setup(&db)?, "nothing may be saved by a failed setup");
    Ok(())
}

#[test]
fn setup_with_a_second_admin_enables_recovery() -> TestResult {
    let mut db = Database::open_in_memory()?;
    setup(&mut db, Some(account("doctor", DOCTOR_PASSWORD, None)))?;
    assert!(users::has_backup_admin(&db)?);
    assert_eq!(auth::login(&mut db, "doctor", DOCTOR_PASSWORD, T0)?.role, Role::Admin);
    Ok(())
}

#[test]
fn login_checks_password_and_locks_after_five_failures() -> TestResult {
    let mut db = Database::open_in_memory()?;
    let owner = setup(&mut db, None)?;

    assert_eq!(auth::login(&mut db, "  OWNER ", OWNER_PASSWORD, T0)?.user_id, owner.user_id, "username ignores case and spaces");
    assert!(matches!(auth::login(&mut db, "nobody", OWNER_PASSWORD, T0), Err(ServiceError::InvalidCredentials)));

    for attempt in 1..=4 {
        let result = auth::login(&mut db, "owner", "wrong-password", T0 + attempt);
        assert!(matches!(result, Err(ServiceError::InvalidCredentials)), "attempt {attempt}");
    }
    let until = match auth::login(&mut db, "owner", "wrong-password", T0 + 5) {
        Err(ServiceError::AccountLocked { until }) => until,
        other => panic!("5th failure must lock the account, got {other:?}"),
    };
    assert_eq!(until, T0 + 5 + 300);

    assert!(matches!(auth::login(&mut db, "owner", OWNER_PASSWORD, T0 + 60), Err(ServiceError::AccountLocked { .. })), "even the right password waits");
    let back = auth::login(&mut db, "owner", OWNER_PASSWORD, until + 1)?;

    let log = actions(&db, &back)?;
    for expected in ["LOGIN_FAILED", "ACCOUNT_LOCKED", "LOGIN"] {
        assert!(log.iter().any(|a| a == expected), "audit log should contain {expected}: {log:?}");
    }
    Ok(())
}

#[test]
fn pin_unlock_with_lockout_and_password_fallback() -> TestResult {
    let mut db = Database::open_in_memory()?;
    let owner = setup(&mut db, None)?;

    auth::unlock_with_pin(&mut db, owner.user_id, "2580", T0)?;
    assert!(matches!(auth::unlock_with_pin(&mut db, owner.user_id, "9999", T0), Err(ServiceError::WrongPin { remaining: 4 })));
    for _ in 0..3 {
        let _ = auth::unlock_with_pin(&mut db, owner.user_id, "9999", T0);
    }
    assert!(matches!(auth::unlock_with_pin(&mut db, owner.user_id, "9999", T0), Err(ServiceError::PinLocked)), "5th wrong PIN");
    assert!(matches!(auth::unlock_with_pin(&mut db, owner.user_id, "2580", T0), Err(ServiceError::PinLocked)), "even the right PIN is refused now");

    auth::unlock_with_password(&mut db, owner.user_id, OWNER_PASSWORD, T0)?;
    auth::unlock_with_pin(&mut db, owner.user_id, "2580", T0).map(|_| ()).map_err(|e| {
        ServiceError::Conflict(format!("PIN should work again after a password unlock: {e}"))
    })
}

#[test]
fn staff_change_their_own_password_and_pin() -> TestResult {
    let mut db = Database::open_in_memory()?;
    let owner = setup(&mut db, None)?;

    let wrong = auth::change_own_password(&mut db, &owner, "not-my-password", "new-pass-2026", T0);
    assert!(matches!(wrong, Err(ServiceError::Validation { field: "currentPassword", .. })));
    auth::change_own_password(&mut db, &owner, OWNER_PASSWORD, "new-pass-2026", T0)?;
    assert!(auth::login(&mut db, "owner", OWNER_PASSWORD, T0).is_err());
    auth::login(&mut db, "owner", "new-pass-2026", T0)?;

    assert!(matches!(auth::set_own_pin(&mut db, &owner, "new-pass-2026", Some("1234"), T0), Err(ServiceError::Validation { field: "pin", .. })));
    let without_pin = auth::set_own_pin(&mut db, &owner, "new-pass-2026", None, T0)?;
    assert!(!without_pin.has_pin);
    assert!(matches!(auth::unlock_with_pin(&mut db, owner.user_id, "2580", T0), Err(ServiceError::PinNotSet)));
    Ok(())
}

#[test]
fn receptionists_cannot_use_admin_functions() -> TestResult {
    let mut db = Database::open_in_memory()?;
    let owner = setup(&mut db, None)?;
    let reception = add_receptionist(&mut db, &owner)?;
    assert_eq!(reception.role, Role::Receptionist);

    let denied = |r: Result<(), ServiceError>| matches!(r, Err(ServiceError::PermissionDenied));
    assert!(denied(users::list(&db, &reception, T0).map(|_| ())));
    assert!(denied(users::create(&mut db, &reception, account("sneaky", "sneaky-pass-1", None), Role::Admin, T0).map(|_| ())));
    assert!(denied(settings::update_clinic(&mut db, &reception, clinic(), T0).map(|_| ())));
    assert!(denied(audit::list(&db, &reception, 10, None).map(|_| ())));
    assert!(denied(users::reset_password(&mut db, &reception, owner.user_id, "taken-over-1", T0)));
    Ok(())
}

#[test]
fn user_management_rules() -> TestResult {
    let mut db = Database::open_in_memory()?;
    let owner = setup(&mut db, None)?;
    add_receptionist(&mut db, &owner)?;

    let duplicate = users::create(&mut db, &owner, account("RECEPTION", "other-pass-1", None), Role::Receptionist, T0);
    assert!(matches!(duplicate, Err(ServiceError::Conflict(_))));

    let self_disable = users::update(&mut db, &owner, owner.user_id, UserUpdate { full_name: "Owner".into(), role: Role::Admin, is_active: false }, T0);
    assert!(matches!(self_disable, Err(ServiceError::Validation { .. })), "admins cannot deactivate themselves");

    // A second admin deactivates the owner; the owner's still-open session loses its powers.
    let doctor = users::create(&mut db, &owner, account("doctor", DOCTOR_PASSWORD, None), Role::Admin, T0)?;
    let doctor_session = auth::login(&mut db, "doctor", DOCTOR_PASSWORD, T0)?;
    users::update(&mut db, &doctor_session, owner.user_id, UserUpdate { full_name: "Owner".into(), role: Role::Admin, is_active: false }, T0)?;
    assert!(matches!(auth::login(&mut db, "owner", OWNER_PASSWORD, T0), Err(ServiceError::AccountDisabled)));
    let stale = users::update(&mut db, &owner, doctor.id, UserUpdate { full_name: "Doctor".into(), role: Role::Receptionist, is_active: true }, T0);
    assert!(matches!(stale, Err(ServiceError::PermissionDenied)), "deactivated admin's old session must not work");

    let listed = users::list(&db, &doctor_session, T0)?;
    assert_eq!(listed.len(), 3);
    assert!(listed.iter().any(|u| u.username == "owner" && !u.is_active));
    Ok(())
}

#[test]
fn an_admin_can_reset_a_forgotten_password() -> TestResult {
    let mut db = Database::open_in_memory()?;
    let owner = setup(&mut db, None)?;
    let reception = add_receptionist(&mut db, &owner)?;
    for i in 0..5 {
        let _ = auth::login(&mut db, "reception", "forgot-it", T0 + i);
    }
    assert!(users::list(&db, &owner, T0 + 10)?.iter().any(|u| u.id == reception.user_id && u.is_locked));

    users::reset_password(&mut db, &owner, reception.user_id, "brand-new-1", T0 + 10)?;
    auth::login(&mut db, "reception", "brand-new-1", T0 + 11)?;
    assert!(actions(&db, &owner)?.contains(&"PASSWORD_RESET".to_string()));
    Ok(())
}

#[test]
fn clinic_settings_are_validated_and_saved() -> TestResult {
    let mut db = Database::open_in_memory()?;
    let owner = setup(&mut db, None)?;

    let bad_gstin = settings::update_clinic(&mut db, &owner, ClinicSettings { gstin: "12345".into(), ..clinic() }, T0);
    assert!(matches!(bad_gstin, Err(ServiceError::Validation { field: "gstin", .. })));
    let bad_idle = settings::update_clinic(&mut db, &owner, ClinicSettings { idle_lock_minutes: 3, ..clinic() }, T0);
    assert!(matches!(bad_idle, Err(ServiceError::Validation { field: "idleLockMinutes", .. })));

    let saved = settings::update_clinic(&mut db, &owner, ClinicSettings { gstin: "27abcde1234f1z5".into(), idle_lock_minutes: 10, ..clinic() }, T0)?;
    assert_eq!(saved.gstin, "27ABCDE1234F1Z5", "GSTIN is upper-cased");
    assert_eq!(settings::get_clinic(&db)?, saved);
    Ok(())
}

#[test]
fn the_audit_log_never_contains_secrets() -> TestResult {
    let mut db = Database::open_in_memory()?;
    let owner = setup(&mut db, None)?;
    add_receptionist(&mut db, &owner)?;
    let _ = auth::login(&mut db, "owner", "typo-password", T0);
    users::reset_password(&mut db, &owner, owner.user_id, "reset-pass-1", T0)?;

    let everything = format!("{:?}", audit::list(&db, &owner, 200, None)?);
    for secret in [OWNER_PASSWORD, RECEPTION_PASSWORD, "typo-password", "reset-pass-1", "2580", "4719", "$argon2"] {
        assert!(!everything.contains(secret), "audit log leaks {secret:?}");
    }
    Ok(())
}
