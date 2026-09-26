import { useState } from 'react';
import { Alert, Button, Checkbox, Dialog, DialogActions, DialogContent, DialogTitle, FormControlLabel, MenuItem, Stack, TextField } from '@mui/material';
import { api, errorField, type AdjustmentKind, type BatchRow, type ProductRow, type Supplier } from '../../api';
import { ErrorAlert, FormGrid } from '../../components/common';
import { t } from '../../i18n/en';
import { formatExpiry } from '../../lib/dates';
import { paiseToInput, parseRupees } from '../../lib/money';

const digits = (text: string) => Number.parseInt(text.replace(/\D/g, '') || '0', 10);

/** Receive stock into a (new or existing) batch. Same batch no. + expiry + price adds to it. */
export function StockInDialog({ product, suppliers, onSaved, onClose }: { product: ProductRow; suppliers: Supplier[]; onSaved: (b: BatchRow) => void; onClose: () => void }) {
  const [batchNo, setBatchNo] = useState('');
  const [expiry, setExpiry] = useState('');
  const [supplierId, setSupplierId] = useState<number | null>(null);
  const [purchase, setPurchase] = useState(() => paiseToInput(product.defaultPurchasePricePaise));
  const [selling, setSelling] = useState(() => paiseToInput(product.defaultSellingPricePaise));
  const [qty, setQty] = useState(0);
  const [opening, setOpening] = useState(false);
  const [note, setNote] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const field = errorField(error);
  const purchasePaise = parseRupees(purchase || '0');
  const sellingPaise = parseRupees(selling || '0');

  const save = async () => {
    if (purchasePaise === null || sellingPaise === null) return;
    setBusy(true);
    setError(null);
    try {
      onSaved(
        await api.stockIn({
          productId: product.id,
          batchNo: batchNo.trim(),
          expiryDate: expiry || null,
          supplierId,
          purchasePricePaise: purchasePaise,
          sellingPricePaise: sellingPaise,
          qty,
          opening,
          note,
        }),
      );
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open onClose={busy ? undefined : onClose} maxWidth="sm" fullWidth>
      <DialogTitle>
        {t.products.stockIn}: {product.name}
      </DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ pt: 1 }}>
          <FormGrid>
            <TextField label={t.products.batchNo} value={batchNo} onChange={(e) => setBatchNo(e.target.value)} required autoFocus error={field === 'batchNo'} />
            <TextField
              label={t.products.expiry}
              type="date"
              value={expiry}
              onChange={(e) => setExpiry(e.target.value)}
              required={product.requiresExpiry}
              error={field === 'expiryDate'}
              slotProps={{ inputLabel: { shrink: true } }}
            />
            <TextField label={t.common.qty} value={qty} onChange={(e) => setQty(digits(e.target.value))} error={field === 'qty'} slotProps={{ htmlInput: { inputMode: 'numeric' } }} />
            <TextField select label={t.products.supplier} value={supplierId ?? ''} onChange={(e) => setSupplierId(e.target.value === '' ? null : Number(e.target.value))}>
              <MenuItem value="">{t.products.noSupplier}</MenuItem>
              {suppliers
                .filter((s) => s.isActive)
                .map((s) => (
                  <MenuItem key={s.id} value={s.id}>
                    {s.name}
                  </MenuItem>
                ))}
            </TextField>
            <TextField label={t.products.purchasePrice} value={purchase} onChange={(e) => setPurchase(e.target.value)} error={purchasePaise === null} />
            <TextField label={t.products.sellingPrice} value={selling} onChange={(e) => setSelling(e.target.value)} error={sellingPaise === null || field === 'sellingPricePaise'} />
          </FormGrid>
          <TextField label={t.products.invoiceNote} value={note} onChange={(e) => setNote(e.target.value)} />
          <FormControlLabel control={<Checkbox checked={opening} onChange={(e) => setOpening(e.target.checked)} />} label={t.products.opening} />
          <ErrorAlert error={error} />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} disabled={busy}>
          {t.common.cancel}
        </Button>
        <Button variant="contained" onClick={save} disabled={busy || !batchNo.trim() || qty <= 0 || (product.requiresExpiry && !expiry) || purchasePaise === null || sellingPaise === null}>
          {t.products.stockIn}
        </Button>
      </DialogActions>
    </Dialog>
  );
}

/** Set a batch to the counted quantity; the difference is written to the stock ledger with a reason. */
export function AdjustDialog({ batch, onSaved, onClose }: { batch: BatchRow; onSaved: (b: BatchRow) => void; onClose: () => void }) {
  const [counted, setCounted] = useState(batch.quantity);
  const [kind, setKind] = useState<AdjustmentKind>('ADJUSTMENT');
  const [reason, setReason] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const change = counted - batch.quantity;

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      onSaved(await api.adjustStock({ batchId: batch.id, countedQty: counted, kind, reason: reason.trim() }));
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open onClose={busy ? undefined : onClose} maxWidth="xs" fullWidth>
      <DialogTitle>
        {t.products.adjust}: {batch.productName} · {batch.batchNo} ({formatExpiry(batch.expiryDate)})
      </DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ pt: 1 }}>
          <TextField select label={t.ledger.kind} value={kind} onChange={(e) => setKind(e.target.value as AdjustmentKind)}>
            {(Object.keys(t.products.adjustKinds) as AdjustmentKind[]).map((k) => (
              <MenuItem key={k} value={k}>
                {t.products.adjustKinds[k]}
              </MenuItem>
            ))}
          </TextField>
          <TextField label={t.products.countedQty} value={counted} onChange={(e) => setCounted(digits(e.target.value))} autoFocus slotProps={{ htmlInput: { inputMode: 'numeric' } }} />
          <Alert severity={change === 0 ? 'info' : change < 0 ? 'warning' : 'success'}>
            {t.ledger.before}: {batch.quantity} → {t.ledger.after}: {counted} ({change > 0 ? '+' : ''}
            {change})
          </Alert>
          <TextField label={t.common.reason} value={reason} onChange={(e) => setReason(e.target.value)} required />
          <ErrorAlert error={error} />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} disabled={busy}>
          {t.common.cancel}
        </Button>
        <Button variant="contained" onClick={save} disabled={busy || change === 0 || reason.trim().length < 3}>
          {t.common.save}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
