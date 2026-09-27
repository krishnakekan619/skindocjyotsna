import { useState } from 'react';
import { Button, Checkbox, Dialog, DialogActions, DialogContent, DialogTitle, FormControlLabel, MenuItem, Stack, TextField } from '@mui/material';
import { api, errorField, type Category, type ProductInput, type ProductRow } from '../../api';
import { ErrorAlert, FormGrid } from '../../components/common';
import { t } from '../../i18n/en';
import { paiseToInput, parseRupees } from '../../lib/money';

/** GST slabs used for medicines and cosmetics, in basis points. */
const GST_RATES = [0, 500, 1200, 1800, 2800];

function draftOf(p: ProductRow | null): ProductInput {
  return p
    ? {
        id: p.id,
        sku: p.sku,
        name: p.name,
        genericName: p.genericName,
        categoryId: p.categoryId,
        productType: p.productType,
        manufacturer: p.manufacturer,
        unit: p.unit,
        gstRateBp: p.gstRateBp,
        defaultSellingPricePaise: p.defaultSellingPricePaise,
        defaultPurchasePricePaise: p.defaultPurchasePricePaise,
        minStock: p.minStock,
        requiresExpiry: p.requiresExpiry,
        isActive: p.isActive,
        notes: p.notes,
      }
    : {
        id: null,
        sku: '',
        name: '',
        genericName: '',
        categoryId: null,
        // The product type shown to staff is the admin-managed list (`categoryId`).
        productType: 'OTHER',
        manufacturer: '',
        unit: 'strip',
        gstRateBp: 0, // prices include GST; no breakup unless an admin sets a rate (DEC-036)
        defaultSellingPricePaise: 0,
        defaultPurchasePricePaise: 0,
        minStock: 0,
        requiresExpiry: true,
        isActive: true,
        notes: '',
      };
}

export function ProductDialog({ product, categories, onSaved, onClose }: { product: ProductRow | null; categories: Category[]; onSaved: (p: ProductRow) => void; onClose: () => void }) {
  const [draft, setDraft] = useState<ProductInput>(() => draftOf(product));
  const [selling, setSelling] = useState(() => paiseToInput(draft.defaultSellingPricePaise));
  const [purchase, setPurchase] = useState(() => paiseToInput(draft.defaultPurchasePricePaise));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const set = (patch: Partial<ProductInput>) => setDraft({ ...draft, ...patch });
  const field = errorField(error);
  const sellingPaise = parseRupees(selling || '0');
  const purchasePaise = parseRupees(purchase || '0');

  const save = async () => {
    if (sellingPaise === null || purchasePaise === null) return;
    setBusy(true);
    setError(null);
    try {
      onSaved(await api.saveProduct({ ...draft, defaultSellingPricePaise: sellingPaise, defaultPurchasePricePaise: purchasePaise }));
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open onClose={busy ? undefined : onClose} maxWidth="sm" fullWidth>
      <DialogTitle>{product ? `${t.common.edit}: ${product.name}` : t.products.add}</DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ pt: 1 }}>
          <FormGrid>
            <TextField label={t.products.name} value={draft.name} onChange={(e) => set({ name: e.target.value })} required autoFocus error={field === 'name'} />
            <TextField label={t.products.generic} value={draft.genericName} onChange={(e) => set({ genericName: e.target.value })} error={field === 'genericName'} />
            <TextField label={t.products.sku} value={draft.sku} onChange={(e) => set({ sku: e.target.value })} helperText={product ? undefined : t.products.skuHint} error={field === 'sku'} />
            <TextField
              select
              label={t.products.category}
              value={draft.categoryId ?? ''}
              onChange={(e) => set({ categoryId: e.target.value === '' ? null : Number(e.target.value) })}
            >
              <MenuItem value="">{t.products.noCategory}</MenuItem>
              {categories
                .filter((c) => c.isActive || c.id === draft.categoryId)
                .map((c) => (
                  <MenuItem key={c.id} value={c.id}>
                    {c.name}
                  </MenuItem>
                ))}
            </TextField>
            <TextField label={t.products.manufacturer} value={draft.manufacturer} onChange={(e) => set({ manufacturer: e.target.value })} />
            <TextField label={t.products.unit} value={draft.unit} onChange={(e) => set({ unit: e.target.value })} required error={field === 'unit'} />
            <TextField select label={t.products.gst} value={draft.gstRateBp} onChange={(e) => set({ gstRateBp: Number(e.target.value) })} error={field === 'gstRateBp'}>
              {GST_RATES.map((bp) => (
                <MenuItem key={bp} value={bp}>
                  {bp / 100}%
                </MenuItem>
              ))}
            </TextField>
            <TextField label={t.products.sellingPrice} value={selling} onChange={(e) => setSelling(e.target.value)} error={sellingPaise === null || field === 'defaultSellingPricePaise'} />
            <TextField label={t.products.purchasePrice} value={purchase} onChange={(e) => setPurchase(e.target.value)} error={purchasePaise === null} />
            <TextField
              label={t.products.minStock}
              value={draft.minStock}
              onChange={(e) => set({ minStock: Number.parseInt(e.target.value.replace(/\D/g, '') || '0', 10) })}
              slotProps={{ htmlInput: { inputMode: 'numeric' } }}
            />
          </FormGrid>
          <TextField label={t.common.notes} value={draft.notes} onChange={(e) => set({ notes: e.target.value })} multiline minRows={2} />
          <Stack direction="row" spacing={2}>
            <FormControlLabel control={<Checkbox checked={draft.requiresExpiry} onChange={(e) => set({ requiresExpiry: e.target.checked })} />} label={t.products.requiresExpiry} />
            {product && <FormControlLabel control={<Checkbox checked={draft.isActive} onChange={(e) => set({ isActive: e.target.checked })} />} label={t.common.active} />}
          </Stack>
          <ErrorAlert error={error} />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} disabled={busy}>
          {t.common.cancel}
        </Button>
        <Button variant="contained" onClick={save} disabled={busy || !draft.name.trim() || sellingPaise === null || purchasePaise === null}>
          {t.common.save}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
