import { useState } from 'react';
import {
  Button,
  Checkbox,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  MenuItem,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
  TextField,
} from '@mui/material';
import { api, PAYMENT_METHODS, type BillDetail, type PaymentMethod } from '../../api';
import { ErrorAlert } from '../../components/common';
import { t } from '../../i18n/en';
import { shownBatchNo } from '../../lib/batch';
import { rupees } from '../../lib/money';

/** Returns some or all of a bill's items; the refund amount is worked out in Rust. */
export function ReturnDialog({ detail, onClose, onDone }: { detail: BillDetail; onClose: () => void; onDone: (message: string) => void }) {
  const items = detail.items.filter((i) => i.qty - i.returnedQty > 0);
  const [qty, setQty] = useState<Record<number, number>>({});
  const [restock, setRestock] = useState<Record<number, boolean>>(() => Object.fromEntries(items.map((i) => [i.id, true])));
  const [reason, setReason] = useState('');
  const [method, setMethod] = useState<PaymentMethod>(detail.payments.find((p) => p.direction === 'IN')?.method ?? 'CASH');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);

  const lines = items.filter((i) => (qty[i.id] ?? 0) > 0).map((i) => ({ billItemId: i.id, qty: qty[i.id] ?? 0, restock: restock[i.id] ?? true }));

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const result = await api.returnBillItems({ billId: detail.bill.id, lines, reason: reason.trim(), refundMethod: method });
      onDone(t.bills.returnSaved(result.returnNo, rupees(result.refundPaise)));
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open onClose={busy ? undefined : onClose} maxWidth="md" fullWidth>
      <DialogTitle>
        {t.bills.returnTitle} · {detail.bill.billNo}
      </DialogTitle>
      <DialogContent>
        <Stack spacing={2}>
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>{t.products.name}</TableCell>
                <TableCell align="right">{t.common.qty}</TableCell>
                <TableCell align="right">{t.common.amount}</TableCell>
                <TableCell align="right">{t.bills.returnQty}</TableCell>
                <TableCell align="center">{t.bills.restock}</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {items.map((item) => {
                const max = item.qty - item.returnedQty;
                return (
                  <TableRow key={item.id}>
                    <TableCell>
                      {item.productName}
                      {(() => {
                        const typed = item.batches.map((b) => shownBatchNo(b.batchNo)).filter(Boolean);
                        return typed.length > 0 ? ` (${typed.join(', ')})` : null;
                      })()}
                    </TableCell>
                    <TableCell align="right">
                      {item.qty}
                      {item.returnedQty > 0 && ` (${t.bills.returned}: ${item.returnedQty})`}
                    </TableCell>
                    <TableCell align="right">{rupees(item.lineTotalPaise)}</TableCell>
                    <TableCell align="right">
                      <TextField
                        size="small"
                        value={qty[item.id] ?? 0}
                        helperText={t.bills.returnable(max)}
                        onChange={(e) => {
                          const n = Number.parseInt(e.target.value.replace(/\D/g, '') || '0', 10);
                          setQty({ ...qty, [item.id]: Math.min(max, n) });
                        }}
                        sx={{ width: 120 }}
                        slotProps={{ htmlInput: { inputMode: 'numeric', style: { textAlign: 'right' } } }}
                      />
                    </TableCell>
                    <TableCell align="center">
                      <Checkbox checked={restock[item.id] ?? true} onChange={(e) => setRestock({ ...restock, [item.id]: e.target.checked })} />
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
          <Stack direction="row" spacing={2}>
            <TextField select label={t.bills.refundMethod} value={method} onChange={(e) => setMethod(e.target.value as PaymentMethod)} sx={{ width: 180 }}>
              {PAYMENT_METHODS.map((m) => (
                <MenuItem key={m} value={m}>
                  {t.billing.methods[m]}
                </MenuItem>
              ))}
            </TextField>
            <TextField label={t.common.reason} value={reason} onChange={(e) => setReason(e.target.value)} sx={{ flexGrow: 1 }} required />
          </Stack>
          <ErrorAlert error={error} />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} disabled={busy}>
          {t.common.cancel}
        </Button>
        <Button variant="contained" onClick={save} disabled={busy || lines.length === 0 || reason.trim().length < 3}>
          {t.bills.returnItems}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
