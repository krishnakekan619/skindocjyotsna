import { useEffect, useRef, useState } from 'react';
import { Alert, Autocomplete, Button, Dialog, DialogActions, DialogContent, DialogTitle, MenuItem, Stack, TextField, Typography } from '@mui/material';
import { api, errorField, type BatchRow, type Category, type ProductRow, type Supplier } from '../../api';
import { ErrorAlert, FormGrid } from '../../components/common';
import { t } from '../../i18n/en';
import { newBillKey } from '../../lib/dates';
import { paiseToInput, parseRupees, rupees } from '../../lib/money';

/**
 * Inventory → Add Inventory (DEC-036): one screen per delivery, seven fields, no batch numbers.
 * Vendor and product can be picked from the list or typed new; a new one is created on save.
 */
export function AddInventoryDialog({
  suppliers,
  types,
  onSaved,
  onClose,
}: {
  suppliers: Supplier[];
  types: Category[];
  onSaved: (b: BatchRow) => void;
  onClose: () => void;
}) {
  const [vendorText, setVendorText] = useState('');
  const [vendor, setVendor] = useState<Supplier | null>(null);
  const [productText, setProductText] = useState('');
  const [product, setProduct] = useState<ProductRow | null>(null);
  const [productOptions, setProductOptions] = useState<ProductRow[]>([]);
  const [typeId, setTypeId] = useState<number | ''>('');
  const [mrp, setMrp] = useState('');
  const [bought, setBought] = useState('');
  const [expiry, setExpiry] = useState('');
  const [qty, setQty] = useState('');
  const [standardPercent, setStandardPercent] = useState(0);
  const [busy, setBusy] = useState(false);
  // One key per form: a double-click or retry adds the stock once (the backend checks it).
  const [requestKey] = useState(newBillKey);
  const sending = useRef(false);
  const [error, setError] = useState<unknown>(null);
  const field = errorField(error);

  useEffect(() => {
    api
      .getClinicSettings()
      .then((s) => setStandardPercent(s.defaultMedicineDiscountPercent))
      .catch(() => undefined);
  }, []);
  // Product search in the database, as you type.
  useEffect(() => {
    if (product || productText.trim().length < 2) {
      setProductOptions([]);
      return;
    }
    const timer = window.setTimeout(() => api.listProducts({ text: productText, limit: 10 }).then(setProductOptions).catch(() => undefined), 200);
    return () => window.clearTimeout(timer);
  }, [productText, product]);

  const pickProduct = (p: ProductRow) => {
    setProduct(p);
    setProductText(p.name);
    if (!mrp && p.defaultSellingPricePaise > 0) setMrp(paiseToInput(p.defaultSellingPricePaise));
    if (!bought && p.defaultPurchasePricePaise > 0) setBought(paiseToInput(p.defaultPurchasePricePaise));
  };

  const mrpPaise = mrp.trim() ? parseRupees(mrp) : null;
  const boughtPaise = bought.trim() ? parseRupees(bought) : null;
  const qtyValue = Number.parseInt(qty, 10);
  const newProduct = !product && productText.trim().length > 0;
  const vendorName = vendor ? vendor.name : vendorText.trim();
  const ready =
    vendorName.length > 0 &&
    (product !== null || newProduct) &&
    mrpPaise !== null &&
    mrpPaise > 0 &&
    boughtPaise !== null &&
    expiry !== '' &&
    Number.isInteger(qtyValue) &&
    qtyValue > 0;
  const sellingPaise = mrpPaise !== null ? Math.round((mrpPaise * (100 - standardPercent)) / 100) : null;

  const save = async () => {
    if (sending.current || !ready || mrpPaise === null || boughtPaise === null) return;
    sending.current = true;
    setBusy(true);
    setError(null);
    try {
      onSaved(
        await api.addInventory({
          productId: product?.id ?? null,
          productName: product ? '' : productText.trim(),
          typeId: product ? null : typeId === '' ? null : typeId,
          vendorId: vendor?.id ?? null,
          vendorName: vendor ? '' : vendorText.trim(),
          mrpPaise,
          purchasePricePaise: boughtPaise,
          expiryDate: expiry,
          qty: qtyValue,
          requestKey,
        }),
      );
    } catch (e) {
      setError(e);
    } finally {
      sending.current = false;
      setBusy(false);
    }
  };

  return (
    <Dialog open onClose={busy ? undefined : onClose} maxWidth="sm" fullWidth>
      <DialogTitle>{t.products.addInventoryTitle}</DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ pt: 1 }}>
          <Typography variant="body2" color="text.secondary">
            {t.products.addInventoryIntro}
          </Typography>
          <Autocomplete<Supplier, false, false, true>
            freeSolo
            options={suppliers.filter((s) => s.isActive)}
            value={vendor}
            onChange={(_, value) => setVendor(value && typeof value !== 'string' ? value : null)}
            inputValue={vendorText}
            onInputChange={(_, value, why) => {
              if (why !== 'reset') {
                setVendorText(value);
                if (vendor && value !== vendor.name) setVendor(null);
              }
            }}
            getOptionLabel={(o) => (typeof o === 'string' ? o : o.name)}
            renderInput={(params) => (
              <TextField
                {...params}
                label={t.products.vendorName}
                required
                autoFocus
                error={field === 'vendorName'}
                helperText={!vendor && vendorText.trim() ? t.products.newVendor(vendorText.trim()) : ' '}
              />
            )}
          />
          <Autocomplete<ProductRow, false, false, true>
            freeSolo
            options={productOptions}
            value={product}
            filterOptions={(x) => x}
            onChange={(_, value) => {
              if (value && typeof value !== 'string') pickProduct(value);
            }}
            inputValue={productText}
            onInputChange={(_, value, why) => {
              if (why !== 'reset') {
                setProductText(value);
                if (product && value !== product.name) setProduct(null);
              }
            }}
            getOptionLabel={(o) => (typeof o === 'string' ? o : o.name)}
            renderOption={({ key, ...props }, o) => (
              <li key={key} {...props}>
                <Stack>
                  <Typography sx={{ fontWeight: 600 }}>{o.name}</Typography>
                  <Typography variant="body2" color="text.secondary">
                    {[o.categoryName, `${t.products.mrp} ${rupees(o.defaultSellingPricePaise)}`, `${o.sellableQty} ${o.unit}`].filter(Boolean).join(' · ')}
                  </Typography>
                </Stack>
              </li>
            )}
            renderInput={(params) => (
              <TextField
                {...params}
                label={t.products.productName}
                required
                error={field === 'productName'}
                helperText={product ? t.products.typeOfExisting(product.categoryName ?? t.products.noCategory) : newProduct ? t.products.newProduct(productText.trim()) : ' '}
              />
            )}
          />
          {newProduct && (
            <TextField select label={t.products.productType} value={typeId} onChange={(e) => setTypeId(e.target.value === '' ? '' : Number(e.target.value))}>
              <MenuItem value="">{t.products.noCategory}</MenuItem>
              {types.map((type) => (
                <MenuItem key={type.id} value={type.id}>
                  {type.name}
                </MenuItem>
              ))}
            </TextField>
          )}
          <FormGrid>
            <TextField label={t.products.mrpField} value={mrp} onChange={(e) => setMrp(e.target.value)} required error={(mrp.trim() !== '' && mrpPaise === null) || field === 'mrpPaise'} />
            <TextField label={t.products.boughtField} value={bought} onChange={(e) => setBought(e.target.value)} required error={(bought.trim() !== '' && boughtPaise === null) || field === 'purchasePricePaise'} />
            <TextField
              label={t.products.expiryField}
              type="date"
              value={expiry}
              onChange={(e) => setExpiry(e.target.value)}
              required
              error={field === 'expiryDate'}
              slotProps={{ inputLabel: { shrink: true } }}
            />
            <TextField
              label={t.products.qtyField}
              value={qty}
              onChange={(e) => setQty(e.target.value.replace(/\D/g, '').slice(0, 7))}
              required
              error={field === 'qty'}
              slotProps={{ htmlInput: { inputMode: 'numeric' } }}
            />
          </FormGrid>
          {mrpPaise !== null && mrpPaise > 0 && sellingPaise !== null && standardPercent > 0 && (
            <Alert severity="info">{t.products.sellsAt(rupees(mrpPaise), standardPercent, rupees(sellingPaise))}</Alert>
          )}
          <ErrorAlert error={error} />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} disabled={busy}>
          {t.common.cancel}
        </Button>
        <Button variant="contained" onClick={() => void save()} disabled={busy || !ready}>
          {t.products.addInventory}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
