import { Alert, Box, Button, Card, CardContent, Link, Stack, Table, TableBody, TableCell, TableRow, Typography } from '@mui/material';
import { api } from '../../api';
import { useApp } from '../../app/AppContext';
import { EmptyState, ErrorAlert, Loading, PageHeader, useLoader } from '../../components/common';
import { t } from '../../i18n/en';
import { formatDateTime } from '../../lib/dates';
import { rupees } from '../../lib/money';
import { BillStatusChip } from '../billing/BillStatusChip';

function Tile({ label, value, tone, onClick }: { label: string; value: string | number; tone?: 'warning' | 'error' | undefined; onClick?: () => void }) {
  return (
    <Card variant="outlined" sx={{ cursor: onClick ? 'pointer' : 'default', borderColor: tone ? `${tone}.main` : undefined }} onClick={onClick}>
      <CardContent>
        <Typography variant="body2" color="text.secondary">
          {label}
        </Typography>
        <Typography variant="h5" sx={{ fontWeight: 700, color: tone ? `${tone}.main` : undefined }}>
          {value}
        </Typography>
      </CardContent>
    </Card>
  );
}

export function DashboardPage() {
  const { navigate, isAdmin } = useApp();
  const { data, error, loading } = useLoader(() => api.getDashboard(), []);

  if (loading && !data) return <Loading />;
  if (!data) return <ErrorAlert error={error} />;
  return (
    <>
      <PageHeader
        title={t.nav.dashboard}
        subtitle={data.today}
        actions={
          <>
            <Button variant="contained" onClick={() => navigate({ name: 'newBill' })}>
              {t.nav.newBill} (F1)
            </Button>
            <Button variant="outlined" onClick={() => navigate({ name: 'clients' })}>
              {t.clients.add}
            </Button>
          </>
        }
      />
      <Stack spacing={2}>
        {isAdmin && !data.hasBackupAdmin && (
          <Alert severity="warning" action={<Button onClick={() => navigate({ name: 'users' })}>{t.nav.users}</Button>}>
            {t.dashboard.backupAdminWarning}
          </Alert>
        )}
        {isAdmin && data.ledgerProblems > 0 && <Alert severity="error">{t.dashboard.ledgerWarning}</Alert>}
        <Box sx={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(170px, 1fr))', gap: 2 }}>
          <Tile label={t.dashboard.salesToday} value={rupees(data.netSalesTodayPaise)} />
          <Tile label={t.dashboard.billsToday} value={data.salesToday.billCount} />
          <Tile label={t.dashboard.clientsToday} value={data.salesToday.clientsServed} />
          <Tile label={t.dashboard.products} value={data.activeProducts} onClick={() => navigate({ name: 'products' })} />
          <Tile label={t.dashboard.lowStock} value={data.lowStock} tone={data.lowStock ? 'warning' : undefined} onClick={() => navigate({ name: 'products' })} />
          <Tile label={t.dashboard.outOfStock} value={data.outOfStock} tone={data.outOfStock ? 'error' : undefined} onClick={() => navigate({ name: 'products' })} />
          <Tile label={t.dashboard.expiring} value={data.expiringSoon} tone={data.expiringSoon ? 'warning' : undefined} onClick={() => navigate({ name: 'expiry' })} />
        </Box>
        <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: '1fr 1fr' }, gap: 2 }}>
          <Card variant="outlined">
            <CardContent>
              <Typography variant="h6">{t.dashboard.recentBills}</Typography>
              {data.recentBills.length === 0 ? (
                <EmptyState text={t.dashboard.noBills} />
              ) : (
                <Table size="small">
                  <TableBody>
                    {data.recentBills.map((b) => (
                      <TableRow key={b.id} hover sx={{ cursor: 'pointer' }} onClick={() => navigate({ name: 'bills' })}>
                        <TableCell>{formatDateTime(b.finalizedAt)}</TableCell>
                        <TableCell>{b.billNo}</TableCell>
                        <TableCell>{b.clientName ?? t.billing.walkIn}</TableCell>
                        <TableCell align="right">{rupees(b.totalPaise)}</TableCell>
                        <TableCell>
                          <BillStatusChip status={b.status} />
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </Card>
          <Card variant="outlined">
            <CardContent>
              <Typography variant="h6">{t.dashboard.recentClients}</Typography>
              {data.recentClients.length === 0 ? (
                <EmptyState text={t.dashboard.noClients} />
              ) : (
                <Table size="small">
                  <TableBody>
                    {data.recentClients.map((c) => (
                      <TableRow key={c.id} hover>
                        <TableCell>
                          <Link component="button" onClick={() => navigate({ name: 'client', clientId: c.id })}>
                            {c.fullName}
                          </Link>
                        </TableCell>
                        <TableCell>{c.clientCode}</TableCell>
                        <TableCell>{formatDateTime(c.lastVisitAt)}</TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </Card>
        </Box>
      </Stack>
    </>
  );
}
