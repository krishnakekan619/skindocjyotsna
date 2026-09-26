import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';
import { Alert, AppBar, Box, Button, Divider, Drawer, List, ListItemButton, ListItemText, ListSubheader, Snackbar, Toolbar, Typography } from '@mui/material';
import { api, type AppStatus, type Session } from '../api';
import { DashboardPage } from '../features/dashboard/DashboardPage';
import { BillsPage } from '../features/billing/BillsPage';
import { NewBillPage } from '../features/billing/NewBillPage';
import { ClientProfilePage } from '../features/clients/ClientProfilePage';
import { ClientsPage } from '../features/clients/ClientsPage';
import { CatalogPage } from '../features/inventory/CatalogPage';
import { ExpiryPage } from '../features/inventory/ExpiryPage';
import { LedgerPage } from '../features/inventory/LedgerPage';
import { ProductsPage } from '../features/inventory/ProductsPage';
import { ReportsPage } from '../features/reports/ReportsPage';
import { AuditLogPage } from '../features/settings/AuditLogPage';
import { ClinicSettingsPage } from '../features/settings/ClinicSettingsPage';
import { MySecurityPage } from '../features/settings/MySecurityPage';
import { SystemPage } from '../features/settings/SystemPage';
import { UsersPage } from '../features/settings/UsersPage';
import { t } from '../i18n/en';
import { AppContext, type AppContextValue, type NoticeSeverity, type Page } from './AppContext';
import { useIdleLock } from './useIdleLock';

const DRAWER_WIDTH = 232;

type NavItem = { page: Page; label: string; adminOnly?: boolean };
const NAV: { section: string; items: NavItem[] }[] = [
  {
    section: '',
    items: [
      { page: { name: 'dashboard' }, label: t.nav.dashboard },
      { page: { name: 'newBill' }, label: t.nav.newBill },
      { page: { name: 'bills' }, label: t.nav.bills },
      { page: { name: 'clients' }, label: t.nav.clients },
    ],
  },
  {
    section: 'Inventory',
    items: [
      { page: { name: 'products' }, label: t.nav.products },
      { page: { name: 'expiry' }, label: t.nav.expiry },
      { page: { name: 'ledger' }, label: t.nav.ledger, adminOnly: true },
      { page: { name: 'catalog' }, label: t.nav.catalog, adminOnly: true },
    ],
  },
  { section: '', items: [{ page: { name: 'reports' }, label: t.nav.reports, adminOnly: true }] },
  {
    section: t.nav.settings,
    items: [
      { page: { name: 'clinic' }, label: t.nav.clinic, adminOnly: true },
      { page: { name: 'users' }, label: t.nav.users, adminOnly: true },
      { page: { name: 'security' }, label: t.nav.security },
      { page: { name: 'audit' }, label: t.nav.audit, adminOnly: true },
      { page: { name: 'system' }, label: t.nav.system, adminOnly: true },
    ],
  },
];

function renderPage(page: Page): ReactNode {
  switch (page.name) {
    case 'dashboard':
      return <DashboardPage />;
    case 'newBill':
      return <NewBillPage key={page.correcting?.bill.id ?? `new-${page.clientId ?? ''}`} initialClientId={page.clientId} correcting={page.correcting} />;
    case 'bills':
      return <BillsPage />;
    case 'clients':
      return <ClientsPage />;
    case 'client':
      return <ClientProfilePage key={page.clientId} clientId={page.clientId} />;
    case 'products':
      return <ProductsPage />;
    case 'expiry':
      return <ExpiryPage />;
    case 'ledger':
      return <LedgerPage />;
    case 'catalog':
      return <CatalogPage />;
    case 'reports':
      return <ReportsPage />;
    case 'clinic':
      return <ClinicSettingsPage />;
    case 'users':
      return <UsersPage />;
    case 'security':
      return <MySecurityPage />;
    case 'audit':
      return <AuditLogPage />;
    case 'system':
      return <SystemPage />;
  }
}

export function MainLayout({ status, session, setStatus }: { status: AppStatus; session: Session; setStatus: (s: AppStatus) => void }) {
  const [page, setPage] = useState<Page>({ name: 'dashboard' });
  const [notice, setNotice] = useState<{ message: string; severity: NoticeSeverity } | null>(null);
  const isAdmin = session.role === 'ADMIN';

  const lock = useCallback(() => {
    api.lockScreen().then(setStatus).catch(() => undefined);
  }, [setStatus]);
  useIdleLock(status.idleLockMinutes, lock);

  // F1: new bill from anywhere (design §9 shortcuts).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'F1') {
        e.preventDefault();
        setPage({ name: 'newBill' });
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const context = useMemo<AppContextValue>(
    () => ({
      status,
      session,
      isAdmin,
      setStatus,
      navigate: setPage,
      notify: (message, severity = 'success') => setNotice({ message, severity }),
    }),
    [status, session, isAdmin, setStatus],
  );

  return (
    <AppContext.Provider value={context}>
      <AppBar position="fixed" elevation={0} sx={{ zIndex: (theme) => theme.zIndex.drawer + 1 }} className="no-print">
        <Toolbar>
          <Typography variant="h6" component="h1" sx={{ fontWeight: 700, flexGrow: 1 }}>
            {status.clinicName || t.app.name}
          </Typography>
          <Typography variant="body2" sx={{ mr: 2, opacity: 0.9 }}>
            {session.fullName} · {t.roles[session.role]}
          </Typography>
          <Button color="inherit" onClick={lock}>
            {t.nav.lock}
          </Button>
          <Button color="inherit" onClick={() => api.logout().then(setStatus).catch(() => undefined)}>
            {t.nav.logout}
          </Button>
        </Toolbar>
      </AppBar>
      <Drawer variant="permanent" className="no-print" sx={{ width: DRAWER_WIDTH, flexShrink: 0, '& .MuiDrawer-paper': { width: DRAWER_WIDTH, boxSizing: 'border-box' } }}>
        <Toolbar />
        <Box sx={{ overflow: 'auto' }}>
          {NAV.map((group, index) => {
            const items = group.items.filter((item) => isAdmin || !item.adminOnly);
            if (items.length === 0) return null;
            return (
              <List key={index} dense subheader={group.section ? <ListSubheader>{group.section}</ListSubheader> : undefined}>
                {items.map((item) => (
                  <ListItemButton key={item.page.name} selected={page.name === item.page.name} onClick={() => setPage(item.page)}>
                    <ListItemText primary={item.label} />
                  </ListItemButton>
                ))}
                <Divider sx={{ mt: 1 }} />
              </List>
            );
          })}
          <Typography variant="caption" color="text.secondary" sx={{ display: 'block', px: 2, py: 1 }}>
            v{status.appVersion}
          </Typography>
        </Box>
      </Drawer>
      <Box component="main" sx={{ ml: `${DRAWER_WIDTH}px`, p: 3, pt: 11 }}>
        {renderPage(page)}
      </Box>
      <Snackbar open={notice !== null} autoHideDuration={5000} onClose={() => setNotice(null)} anchorOrigin={{ vertical: 'bottom', horizontal: 'center' }}>
        <Alert severity={notice?.severity ?? 'success'} onClose={() => setNotice(null)} variant="filled" sx={{ whiteSpace: 'pre-line' }}>
          {notice?.message}
        </Alert>
      </Snackbar>
    </AppContext.Provider>
  );
}
