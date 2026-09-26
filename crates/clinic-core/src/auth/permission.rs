use super::Role;

/// Actions that need a permission check. Every desktop command and service call checks one.
/// New phases add variants here (inventory, billing, ...) and decide per role below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    /// Open the app and use the everyday screens.
    UseApp,
    /// Change one's own password and PIN.
    ManageOwnSecurity,
    ManageUsers,
    ManageClinicSettings,
    ViewAuditLog,
    ManageBackups,
    ViewSystemInfo,
}

impl Role {
    /// The permission matrix (design §5.3). Deny by default: a receptionist gets only what is
    /// listed explicitly.
    pub fn allows(self, permission: Permission) -> bool {
        match self {
            Role::Admin => true,
            Role::Receptionist => matches!(permission, Permission::UseApp | Permission::ManageOwnSecurity),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Permission; 7] = [
        Permission::UseApp,
        Permission::ManageOwnSecurity,
        Permission::ManageUsers,
        Permission::ManageClinicSettings,
        Permission::ViewAuditLog,
        Permission::ManageBackups,
        Permission::ViewSystemInfo,
    ];

    #[test]
    fn admin_may_do_everything() {
        assert!(ALL.iter().all(|p| Role::Admin.allows(*p)));
    }

    #[test]
    fn receptionist_is_limited_to_everyday_work() {
        let allowed: Vec<_> = ALL.iter().copied().filter(|p| Role::Receptionist.allows(*p)).collect();
        assert_eq!(allowed, vec![Permission::UseApp, Permission::ManageOwnSecurity]);
    }
}
