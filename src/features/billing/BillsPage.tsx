import { useEffect, useState } from 'react';
import { Button, Card, MenuItem, Stack, Table, TableBody, TableCell, TableHead, TableRow, TextField } from '@mui/material';
import { api, type BillStatus } from '../../api';
import { EmptyState, ErrorAlert, Loading, PageHeader, useLoader } from '../../components/common';
import { t } from '../../i18n/en';
import { addDaysIso, formatDateTime, todayIso } from '../../lib/dates';
import { rupees } from '../../lib/money';
import { BillDetailDialog } from './BillDetailDialog';
import { BillStatusChip } from './BillStatusChip';

const STATUSES: BillStatus[] = ['FINALIZED', 'CANCELLED', 'CORRECTED'];

/** Bill history with date / status / text filters. */
export function BillsPage() {
  const [from, setFrom] = useState(() => addDaysIso(todayIso(), -6));
  const [to, setTo] = useState(todayIso);
  const [status, setStatus] = useState<BillStatus | ''>('');
  const [text, setText] = useState('');
  const [query, setQuery] = useState('');
  const [openBill, setOpenBill] = useState<number | null>(null);
  const [limit, setLimit] = useState(300);

  useEffect(() => {
    const timer = window.setTimeout(() => setQuery(text), 250);
    return () => window.clearTimeout(timer);
  }, [text]);
  const { data, error, loading, reload } = useLoader(
    () => api.listBills({ fromDate: from || null, toDate: to || null, status: status || null, text: query, limit }),
    [from, to, status, query, limit],
  );

  return (
    <>
      <PageHeader title={t.bills.title} />
      <Stack direction="row" spacing={2} sx={{ mb: 2, alignItems: 'center', flexWrap: 'wrap' }} useFlexGap>
        <TextField label={t.common.search} placeholder={t.bills.searchHint} value={text} onChange={(e) => setText(e.target.value)} sx={{ minWidth: 280 }} />
        <TextField label={t.clients.from} type="date" value={from} onChange={(e) => setFrom(e.target.value)} slotProps={{ inputLabel: { shrink: true } }} />
        <TextField label={t.clients.to} type="date" value={to} onChange={(e) => setTo(e.target.value)} slotProps={{ inputLabel: { shrink: true } }} />
        <TextField select label={t.bills.status} value={status} onChange={(e) => setStatus(e.target.value as BillStatus | '')} sx={{ width: 160 }}>
          <MenuItem value="">{t.common.all}</MenuItem>
          {STATUSES.map((s) => (
            <MenuItem key={s} value={s}>
              {t.bills.statuses[s]}
            </MenuItem>
          ))}
        </TextField>
      </Stack>
      <ErrorAlert error={error} />
      <Card variant="outlined">
        {loading && !data ? (
          <Loading />
        ) : !data || data.length === 0 ? (
          <EmptyState text={t.bills.empty} />
        ) : (
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>{t.bills.billNo}</TableCell>
                <TableCell>{t.common.date}</TableCell>
                <TableCell>{t.billing.client}</TableCell>
                <TableCell>{t.bills.by}</TableCell>
                <TableCell>{t.bills.status}</TableCell>
                <TableCell align="right">{t.common.total}</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {data.map((b) => (
                <TableRow key={b.id} hover sx={{ cursor: 'pointer' }} onClick={() => setOpenBill(b.id)}>
                  <TableCell>{b.billNo}</TableCell>
                  <TableCell>{formatDateTime(b.finalizedAt)}</TableCell>
                  <TableCell>{b.clientName ? `${b.clientName} (${b.clientCode ?? ''})` : t.billing.walkIn}</TableCell>
                  <TableCell>{b.createdByName}</TableCell>
                  <TableCell>
                    <BillStatusChip status={b.status} returned={b.returnedPaise > 0} />
                  </TableCell>
                  <TableCell align="right">{rupees(b.totalPaise)}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </Card>
      {data && data.length >= limit && (
        <Stack direction="row" sx={{ mt: 2, justifyContent: 'center' }}>
          <Button onClick={() => setLimit(limit + 300)} disabled={loading}>
            {t.products.loadMore}
          </Button>
        </Stack>
      )}
      {openBill !== null && <BillDetailDialog billId={openBill} onClose={() => setOpenBill(null)} onChanged={reload} />}
    </>
  );
}
