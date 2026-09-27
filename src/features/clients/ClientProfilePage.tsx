import { useState } from 'react';
import { Alert, Box, Button, Card, CardContent, Divider, Stack, TextField, Typography } from '@mui/material';
import { api } from '../../api';
import { useApp } from '../../app/AppContext';
import { EmptyState, ErrorAlert, Loading, PageHeader, useLoader, rowActions } from '../../components/common';
import { t } from '../../i18n/en';
import { ageYears, formatDateTime, formatIsoDate } from '../../lib/dates';
import { rupees } from '../../lib/money';
import { BillStatusChip } from '../billing/BillStatusChip';
import { BillDetailDialog } from '../billing/BillDetailDialog';
import { ClientDialog } from './ClientDialog';

/** Profile and purchase/medication history (brief §16), filterable by date. */
export function ClientProfilePage({ clientId }: { clientId: number }) {
  const { navigate, notify } = useApp();
  const [from, setFrom] = useState('');
  const [to, setTo] = useState('');
  const [editing, setEditing] = useState(false);
  const [openBill, setOpenBill] = useState<number | null>(null);
  const { data, error, loading, reload } = useLoader(() => api.getClientProfile(clientId, from || null, to || null), [clientId, from, to]);

  if (loading && !data) return <Loading />;
  if (!data) return <ErrorAlert error={error} />;
  const c = data.client;
  const age = ageYears(c.dateOfBirth);
  return (
    <>
      <PageHeader
        title={`${c.fullName} · ${c.clientCode}`}
        subtitle={[c.phone, c.email, c.dateOfBirth ? `${formatIsoDate(c.dateOfBirth)}${age !== null ? ` (${age})` : ''}` : '', t.clients.genders[c.gender]].filter(Boolean).join(' · ')}
        actions={
          <>
            <Button onClick={() => navigate({ name: 'clients' })}>{t.common.back}</Button>
            <Button variant="outlined" onClick={() => setEditing(true)}>
              {t.common.edit}
            </Button>
            {c.isActive && c.mergedIntoClientId === null && (
              <Button variant="contained" onClick={() => navigate({ name: 'newBill', clientId: c.id })}>
                {t.clients.newBill}
              </Button>
            )}
          </>
        }
      />
      <Stack spacing={2}>
        {c.mergedIntoClientId !== null && (
          <Alert
            severity="info"
            action={
              <Button color="inherit" onClick={() => navigate({ name: 'client', clientId: c.mergedIntoClientId ?? c.id })}>
                {t.common.open}
              </Button>
            }
          >
            {t.clients.mergedInto}
          </Alert>
        )}
        <Typography color="text.secondary">
          {t.clients.totalVisits}: {data.visitCount} · {t.clients.totalBills}: {data.billCount} · {t.clients.lastVisit}: {formatDateTime(c.lastVisitAt)} · {t.clients.totalSpent}:{' '}
          {rupees(data.totalSpentPaise)}
          {c.address ? ` · ${c.address}` : ''}
          {c.emergencyContact ? ` · ${t.clients.emergency}: ${c.emergencyContact}` : ''}
        </Typography>
        {c.notes && <Typography>{c.notes}</Typography>}
        <Stack direction="row" spacing={2}>
          <TextField label={t.clients.from} type="date" value={from} onChange={(e) => setFrom(e.target.value)} slotProps={{ inputLabel: { shrink: true } }} size="small" />
          <TextField label={t.clients.to} type="date" value={to} onChange={(e) => setTo(e.target.value)} slotProps={{ inputLabel: { shrink: true } }} size="small" />
        </Stack>
        <Card variant="outlined">
          <CardContent>
            {data.visits.length === 0 ? (
              <EmptyState text={t.clients.noVisits} />
            ) : (
              <Stack divider={<Divider />} spacing={1.5}>
                {data.visits.map((v) => (
                  <Box key={v.bill.id} sx={{ cursor: 'pointer' }} {...rowActions(() => setOpenBill(v.bill.id))}>
                    <Stack direction="row" spacing={2} sx={{ alignItems: 'center' }}>
                      <Typography sx={{ fontWeight: 600, minWidth: 150 }}>{formatDateTime(v.bill.finalizedAt)}</Typography>
                      <Typography sx={{ minWidth: 170 }}>{v.bill.billNo}</Typography>
                      <Typography sx={{ minWidth: 110 }}>{rupees(v.bill.totalPaise)}</Typography>
                      <BillStatusChip status={v.bill.status} returned={v.bill.returnedPaise > 0} />
                    </Stack>
                    <Typography variant="body2" color="text.secondary" sx={{ pl: 1 }}>
                      {[
                        ...v.services.map((s) => (s.qty > 1 ? `${s.name} × ${s.qty}` : s.name)),
                        ...v.items.map((i) =>
                          i.qty > 0
                            ? `${i.productName} × ${i.qty}${i.returnedQty ? ` (${t.bills.returned.toLowerCase()} ${i.returnedQty})` : ''}`
                            : `${i.productName} × ${i.notSuppliedQty} (${t.billing.notSuppliedShort.toLowerCase()})`,
                        ),
                      ].join(' · ')}
                    </Typography>
                  </Box>
                ))}
              </Stack>
            )}
          </CardContent>
        </Card>
      </Stack>
      {editing && (
        <ClientDialog
          client={c}
          onClose={() => setEditing(false)}
          onSaved={() => {
            setEditing(false);
            notify(t.common.saved);
            reload();
          }}
        />
      )}
      {openBill !== null && <BillDetailDialog billId={openBill} onClose={() => setOpenBill(null)} onChanged={reload} />}
    </>
  );
}
