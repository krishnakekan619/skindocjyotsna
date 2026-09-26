import { Card, Table, TableBody, TableCell, TableHead, TableRow } from '@mui/material';
import { api, type LedgerRow } from '../../api';
import { EmptyState, ErrorAlert, Loading, PageHeader, useLoader } from '../../components/common';
import { t } from '../../i18n/en';
import { formatDateTime } from '../../lib/dates';

/** The immutable stock ledger: every change to every batch, newest first. */
export function LedgerPage() {
  const { data, error, loading } = useLoader(() => api.listStockLedger(null, 500), []);
  return (
    <>
      <PageHeader title={t.ledger.title} />
      <ErrorAlert error={error} />
      <Card variant="outlined">{loading && !data ? <Loading /> : <LedgerTable rows={data ?? []} showProduct />}</Card>
    </>
  );
}

export function LedgerTable({ rows, showProduct }: { rows: LedgerRow[]; showProduct: boolean }) {
  if (rows.length === 0) return <EmptyState text={t.ledger.empty} />;
  return (
    <Table size="small">
      <TableHead>
        <TableRow>
          <TableCell>{t.common.date}</TableCell>
          {showProduct && <TableCell>{t.products.name}</TableCell>}
          <TableCell>{t.products.batchNo}</TableCell>
          <TableCell>{t.ledger.kind}</TableCell>
          <TableCell align="right">{t.ledger.change}</TableCell>
          <TableCell align="right">{t.ledger.before}</TableCell>
          <TableCell align="right">{t.ledger.after}</TableCell>
          <TableCell>{t.common.reason}</TableCell>
          <TableCell>{t.ledger.by}</TableCell>
        </TableRow>
      </TableHead>
      <TableBody>
        {rows.map((r) => (
          <TableRow key={r.id}>
            <TableCell>{formatDateTime(r.occurredAt)}</TableCell>
            {showProduct && <TableCell>{r.productName}</TableCell>}
            <TableCell>{r.batchNo}</TableCell>
            <TableCell>{t.ledger.kinds[r.kind as keyof typeof t.ledger.kinds] ?? r.kind}</TableCell>
            <TableCell align="right" sx={{ color: r.qtyChange < 0 ? 'error.main' : 'success.main', fontWeight: 600 }}>
              {r.qtyChange > 0 ? '+' : ''}
              {r.qtyChange}
            </TableCell>
            <TableCell align="right">{r.previousQty}</TableCell>
            <TableCell align="right">{r.newQty}</TableCell>
            <TableCell>{[r.billNo, r.reason].filter(Boolean).join(' · ')}</TableCell>
            <TableCell>{r.username}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
