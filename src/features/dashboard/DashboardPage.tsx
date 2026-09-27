import { useState } from 'react';
import {
  Alert,
  Box,
  Button,
  Card,
  CardContent,
  Link,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableRow,
  ToggleButton,
  ToggleButtonGroup,
  Tooltip,
  Typography,
} from '@mui/material';
import { api, type DateRange, type SectionSales } from '../../api';
import { useApp } from '../../app/AppContext';
import { EmptyState, ErrorAlert, Loading, PageHeader, StatusChip, useLoader } from '../../components/common';
import { MOD_KEY, t } from '../../i18n/en';
import { addDaysIso, formatDateTime, formatExpiry, monthStartIso, todayIso } from '../../lib/dates';
import { rupees } from '../../lib/money';
import { BillStatusChip } from '../billing/BillStatusChip';

type Period = 'today' | 'week' | 'month';

/**
 * Colours of the three kinds of sale. Checked with the data-viz palette validator (light surface,
 * all pairs): CVD ΔE ≥ 8.4, normal-vision ΔE ≥ 24, contrast ≥ 3:1. Always shown with a label.
 */
const KIND_COLORS = { consultation: '#7B4FA0', procedures: '#E0782F', medicines: '#1B9E77' } as const;

function rangeOf(period: Period): DateRange {
  const today = todayIso();
  return { from: period === 'today' ? today : period === 'week' ? addDaysIso(today, -6) : monthStartIso(), to: today };
}

function Tile({ label, value, tone, onClick }: { label: string; value: string | number; tone?: 'warning' | 'error' | undefined; onClick?: (() => void) | undefined }) {
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

/** Consultation / procedures / medicines: three labelled figures and one 100% bar. */
function SalesSplit({ split }: { split: SectionSales }) {
  const parts = [
    { key: 'consultation', label: t.billing.consultation, value: split.consultationPaise, color: KIND_COLORS.consultation },
    { key: 'procedures', label: t.billing.procedures, value: split.proceduresPaise, color: KIND_COLORS.procedures },
    { key: 'medicines', label: t.billing.products, value: split.medicinesPaise, color: KIND_COLORS.medicines },
  ];
  const total = parts.reduce((s, p) => s + p.value, 0);
  const share = (v: number) => (total > 0 ? Math.round((v / total) * 100) : 0);
  return (
    <Stack spacing={1.5}>
      <Box sx={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 2 }}>
        {parts.map((p) => (
          <Box key={p.key}>
            <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
              <Box sx={{ width: 10, height: 10, borderRadius: '3px', bgcolor: p.color, flexShrink: 0 }} />
              <Typography variant="body2" color="text.secondary">
                {p.label}
              </Typography>
            </Stack>
            <Typography variant="h6" sx={{ fontWeight: 700, fontVariantNumeric: 'tabular-nums' }}>
              {rupees(p.value)}
            </Typography>
            <Typography variant="body2" color="text.secondary">
              {share(p.value)}%
            </Typography>
          </Box>
        ))}
      </Box>
      {total > 0 && (
        <Box sx={{ display: 'flex', gap: '2px', height: 12 }} role="img" aria-label={parts.map((p) => `${p.label} ${share(p.value)}%`).join(', ')}>
          {parts
            .filter((p) => p.value > 0)
            .map((p) => (
              <Tooltip key={p.key} title={`${p.label}: ${rupees(p.value)} (${share(p.value)}%)`}>
                <Box sx={{ flexGrow: p.value, minWidth: 4, bgcolor: p.color, borderRadius: '4px' }} />
              </Tooltip>
            ))}
        </Box>
      )}
    </Stack>
  );
}

/** One "what sells most" column: name, how many, amount, and a thin bar for the amount. */
function TopList({ title, color, rows }: { title: string; color: string; rows: { key: number; name: string; qty: number; revenue: number }[] }) {
  const max = Math.max(1, ...rows.map((r) => r.revenue));
  return (
    <Box>
      <Stack direction="row" spacing={1} sx={{ alignItems: 'center', mb: 1 }}>
        <Box sx={{ width: 10, height: 10, borderRadius: '3px', bgcolor: color }} />
        <Typography sx={{ fontWeight: 700 }}>{title}</Typography>
      </Stack>
      {rows.length === 0 ? (
        <Typography variant="body2" color="text.secondary">
          {t.dashboard.nothingSold}
        </Typography>
      ) : (
        <Stack spacing={1}>
          {rows.map((r) => (
            <Tooltip key={r.key} title={`${r.name}: ${t.dashboard.sold(r.qty)}, ${rupees(r.revenue)}`} placement="top-start">
              <Box>
                <Stack direction="row" sx={{ justifyContent: 'space-between', gap: 1 }}>
                  <Typography variant="body2" sx={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {r.name} <Typography component="span" variant="body2" color="text.secondary">· {t.dashboard.times(r.qty)}</Typography>
                  </Typography>
                  <Typography variant="body2" sx={{ fontVariantNumeric: 'tabular-nums', flexShrink: 0 }}>
                    {rupees(r.revenue)}
                  </Typography>
                </Stack>
                <Box sx={{ mt: 0.5, height: 4, borderRadius: '2px', bgcolor: color, width: `${Math.max(2, (r.revenue / max) * 100)}%` }} />
              </Box>
            </Tooltip>
          ))}
        </Stack>
      )}
    </Box>
  );
}

export function DashboardPage() {
  const { navigate, isAdmin } = useApp();
  const [period, setPeriod] = useState<Period>('today');
  const { data, error, loading } = useLoader(() => api.getDashboard(), []);
  const top = useLoader(() => api.topSellers(rangeOf(period)), [period]);

  if (loading && !data) return <Loading />;
  if (!data) return <ErrorAlert error={error} />;
  return (
    <>
      <PageHeader
        title={t.nav.dashboard}
        subtitle={data.today}
        actions={
          <Button variant="contained" onClick={() => navigate({ name: 'newBill' })}>
            {t.nav.newBill} ({MOD_KEY}+N)
          </Button>
        }
      />
      <Stack spacing={2}>
        {isAdmin && !data.hasBackupAdmin && (
          <Alert severity="warning" action={<Button onClick={() => navigate({ name: 'users' })}>{t.nav.users}</Button>}>
            {t.dashboard.backupAdminWarning}
          </Alert>
        )}
        {isAdmin && data.ledgerProblems > 0 && <Alert severity="error">{t.dashboard.ledgerWarning}</Alert>}

        {/* Today at a glance */}
        <Box sx={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))', gap: 2 }}>
          <Tile label={t.dashboard.salesToday} value={rupees(data.netSalesTodayPaise)} />
          <Tile label={t.dashboard.billsToday} value={data.salesToday.billCount} />
          <Tile label={t.dashboard.clientsToday} value={data.salesToday.clientsServed} onClick={() => navigate({ name: 'clients' })} />
        </Box>

        {/* Sales by type and what sells most, for the chosen period */}
        <Card variant="outlined">
          <CardContent>
            <Stack direction="row" sx={{ alignItems: 'center', justifyContent: 'space-between', mb: 2, flexWrap: 'wrap', gap: 1 }}>
              <Box>
                <Typography variant="h6">{t.dashboard.salesSplit}</Typography>
                <Typography variant="body2" color="text.secondary">
                  {t.dashboard.salesSplitNote}
                </Typography>
              </Box>
              <ToggleButtonGroup size="small" exclusive value={period} onChange={(_, v: Period | null) => v && setPeriod(v)}>
                <ToggleButton value="today">{t.dashboard.periods.today}</ToggleButton>
                <ToggleButton value="week">{t.dashboard.periods.week}</ToggleButton>
                <ToggleButton value="month">{t.dashboard.periods.month}</ToggleButton>
              </ToggleButtonGroup>
            </Stack>
            <ErrorAlert error={top.error} />
            {top.loading && !top.data ? (
              <Loading />
            ) : top.data ? (
              <Stack spacing={3}>
                <SalesSplit split={top.data.split} />
                <Box>
                  <Typography variant="subtitle1" sx={{ fontWeight: 700, mb: 1.5 }}>
                    {t.dashboard.topSellers}
                  </Typography>
                  <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: 'repeat(3, 1fr)' }, gap: 3 }}>
                    <TopList
                      title={t.dashboard.topMedicines}
                      color={KIND_COLORS.medicines}
                      rows={top.data.medicines.slice(0, 5).map((m) => ({ key: m.productId, name: m.productName, qty: m.qtySold, revenue: m.revenuePaise }))}
                    />
                    <TopList
                      title={t.dashboard.topProcedures}
                      color={KIND_COLORS.procedures}
                      rows={top.data.procedures.slice(0, 5).map((s) => ({ key: s.serviceId, name: s.name, qty: s.qty, revenue: s.revenuePaise }))}
                    />
                    <TopList
                      title={t.dashboard.topConsultations}
                      color={KIND_COLORS.consultation}
                      rows={top.data.consultations.slice(0, 5).map((s) => ({ key: s.serviceId, name: s.name, qty: s.qty, revenue: s.revenuePaise }))}
                    />
                  </Box>
                </Box>
              </Stack>
            ) : null}
          </CardContent>
        </Card>

        {/* Stock */}
        <Box sx={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))', gap: 2 }}>
          <Tile label={t.dashboard.products} value={data.activeProducts} onClick={() => navigate({ name: 'products' })} />
          <Tile label={t.dashboard.lowStock} value={data.lowStock} tone={data.lowStock ? 'warning' : undefined} onClick={() => navigate({ name: 'products' })} />
          <Tile label={t.dashboard.outOfStock} value={data.outOfStock} tone={data.outOfStock ? 'error' : undefined} onClick={() => navigate({ name: 'products' })} />
          <Tile label={t.dashboard.expiring} value={data.expiringSoon} tone={data.expiringSoon ? 'warning' : undefined} onClick={() => navigate({ name: 'expiry' })} />
        </Box>
        <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: '1fr 1fr' }, gap: 2 }}>
          <Card variant="outlined">
            <CardContent>
              <Typography variant="h6">{t.dashboard.stockAlerts}</Typography>
              {data.stockAlerts.length === 0 ? (
                <EmptyState text={t.dashboard.allStocked} />
              ) : (
                <Table size="small">
                  <TableBody>
                    {data.stockAlerts.map((p) => (
                      <TableRow key={p.id} hover sx={{ cursor: 'pointer' }} onClick={() => navigate({ name: 'products' })}>
                        <TableCell>{p.name}</TableCell>
                        <TableCell align="right">
                          {p.sellableQty} {p.unit}
                        </TableCell>
                        <TableCell align="right">
                          {p.sellableQty === 0 ? <StatusChip label={t.products.filterOut} color="error" /> : <StatusChip label={t.products.filterLow} color="warning" />}
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
              <Typography variant="h6">{t.dashboard.expiringList}</Typography>
              {data.expiringBatches.length === 0 ? (
                <EmptyState text={t.dashboard.noneExpiring} />
              ) : (
                <Table size="small">
                  <TableBody>
                    {data.expiringBatches.map((b) => (
                      <TableRow key={b.id} hover sx={{ cursor: 'pointer' }} onClick={() => navigate({ name: 'expiry' })}>
                        <TableCell>{b.productName}</TableCell>
                        <TableCell>
                          {b.batchNo} · {formatExpiry(b.expiryDate)}
                        </TableCell>
                        <TableCell align="right">{b.quantity}</TableCell>
                        <TableCell align="right">
                          {b.daysLeft < 0 ? <StatusChip label={t.expiry.expiredAgo(-b.daysLeft)} color="error" /> : <StatusChip label={t.expiry.days(b.daysLeft)} color="warning" />}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </Card>
        </Box>

        {/* Recent activity */}
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
