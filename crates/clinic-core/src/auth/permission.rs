use super::Role;

/// Actions that need a permission check. Every desktop command and service call checks one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    /// Open the app and use the everyday screens (dashboard).
    UseApp,
    /// Change one's own password and PIN.
    ManageOwnSecurity,
    /// See products, batches and available stock.
    ViewInventory,
    /// Add/edit products, categories, suppliers; stock in; stock adjustments.
    ManageInventory,
    /// See the stock movement history (ledger).
    ViewStockLedger,
    /// Add, edit and search clients; see their history.
    ManageClients,
    /// Create bills, view bills and receipts, correct same-day bills.
    CreateBills,
    /// Return items (receptionists within the return window; the service checks the window).
    ProcessReturns,
    /// Cancel bills; correct older bills.
    CancelBills,
    /// Sales, product-sales and stock reports.
    ViewReports,
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
            Role::Receptionist => matches!(
                permission,
                Permission::UseApp
                    | Permission::ManageOwnSecurity
                    | Permission::ViewInventory
                    | Permission::ManageClients
                    | Permission::CreateBills
                    | Permission::ProcessReturns
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Permission; 15] = [
        Permission::UseApp,
        Permission::ManageOwnSecurity,
        Permission::ViewInventory,
        Permission::ManageInventory,
        Permission::ViewStockLedger,
        Permission::ManageClients,
        Permission::CreateBills,
        Permission::ProcessReturns,
        Permission::CancelBills,
        Permission::ViewReports,
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
        assert_eq!(
            allowed,
            vec![
                Permission::UseApp,
                Permission::ManageOwnSecurity,
                Permission::ViewInventory,
                Permission::ManageClients,
                Permission::CreateBills,
                Permission::ProcessReturns,
            ]
        );
    }
}
