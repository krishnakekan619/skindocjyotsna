import { useEffect, useState } from 'react';
import {
  Button,
  Card,
  Checkbox,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  FormControlLabel,
  MenuItem,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
  TextField,
  ToggleButton,
  ToggleButtonGroup,
  Typography,
} from '@mui/material';
import { api, type BatchRow, type ProductRow, type StockFilter } from '../../api';
import { useApp } from '../../app/AppContext';
import { EmptyState, ErrorAlert, Loading, PageHeader, StatusChip, useLoader } from '../../components/common';
import { t } from '../../i18n/en';
import { formatExpiry, todayIso } from '../../lib/dates';
import { percentLabel, rupees } from '../../lib/money';
import { LedgerTable } from './LedgerPage';
import { ProductDialog } from './ProductDialog';
import { AdjustDialog, StockInDialog } from './StockDialogs';

export function ProductsPage() {
  const { isAdmin, notify } = useApp();
  const [text, setText] = useState('');
  const [query, setQuery] = useState('');
  const [stock, setStock] = useState<StockFilter>('ALL');
  const [categoryId, setCategoryId] = useState<number | null>(null);
  const [includeInactive, setIncludeInactive] = useState(false);
  const [editing, setEditing] = useState<ProductRow | 'new' | null>(null);
  const [stockIn, setStockIn] = useState<ProductRow | null>(null);
  const [detail, setDetail] = useState<number | null>(null);

  useEffect(() => {
    const timer = window.setTimeout(() => setQuery(text), 250);
    return () => window.clearTimeout(timer);
  }, [text]);
  const { data, error, loading, reload } = useLoader(() => api.listProducts({ text: query, categoryId, includeInactive, stock }), [query, categoryId, includeInactive, stock]);
  const categories = useLoader(() => api.listCategories(), []);
  const suppliers = useLoader(() => (isAdmin ? api.listSuppliers() : Promise.resolve([])), [isAdmin]);

  return (
    <>
      <PageHeader title={t.products.title} actions={isAdmin ? <Button variant="contained" onClick={() => setEditing('new')}>{t.products.add}</Button> : undefined} />
      <Stack direction="row" spacing={2} sx={{ mb: 2, alignItems: 'center', flexWrap: 'wrap' }} useFlexGap>
        <TextField label={t.common.search} placeholder={t.products.searchHint} value={text} onChange={(e) => setText(e.target.value)} autoFocus sx={{ flexGrow: 1, maxWidth: 420 }} />
        <TextField select label={t.products.category} value={categoryId ?? ''} onChange={(e) => setCategoryId(e.target.value === '' ? null : Number(e.target.value))} sx={{ width: 200 }}>
          <MenuItem value="">{t.common.all}</MenuItem>
          {(categories.data ?? []).map((c) => (
            <MenuItem key={c.id} value={c.id}>
              {c.name}
            </MenuItem>
          ))}
        </TextField>
        <ToggleButtonGroup size="small" exclusive value={stock} onChange={(_, v: StockFilter | null) => v && setStock(v)}>
          <ToggleButton value="ALL">{t.products.filterAll}</ToggleButton>
          <ToggleButton value="LOW">{t.products.filterLow}</ToggleButton>
          <ToggleButton value="OUT">{t.products.filterOut}</ToggleButton>
        </ToggleButtonGroup>
        <FormControlLabel control={<Checkbox checked={includeInactive} onChange={(e) => setIncludeInactive(e.target.checked)} />} label={t.common.showInactive} />
      </Stack>
      <ErrorAlert error={error} />
      <Card variant="outlined">
        {loading && !data ? (
          <Loading />
        ) : !data || data.length === 0 ? (
          <EmptyState text={t.products.empty} />
        ) : (
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>{t.products.sku}</TableCell>
                <TableCell>{t.products.name}</TableCell>
                <TableCell>{t.products.category}</TableCell>
                <TableCell align="right">{t.common.price}</TableCell>
                <TableCell align="right">{t.products.gst}</TableCell>
                <TableCell align="right">{t.products.sellable}</TableCell>
                <TableCell>{t.products.nextExpiry}</TableCell>
                <TableCell />
              </TableRow>
            </TableHead>
            <TableBody>
              {data.map((p) => (
                <TableRow key={p.id} hover sx={{ cursor: 'pointer' }} onClick={() => setDetail(p.id)}>
                  <TableCell>{p.sku}</TableCell>
                  <TableCell>
                    {p.name}
                    {p.genericName && <Typography component="span" color="text.secondary"> · {p.genericName}</Typography>} {!p.isActive && <StatusChip label={t.common.inactive} color="default" />}
                  </TableCell>
                  <TableCell>{p.categoryName ?? '—'}</TableCell>
                  <TableCell align="right">{rupees(p.defaultSellingPricePaise)}</TableCell>
                  <TableCell align="right">{percentLabel(p.gstRateBp)}</TableCell>
                  <TableCell align="right">
                    {p.sellableQty === 0 ? (
                      <StatusChip label={t.products.filterOut} color="error" />
                    ) : p.sellableQty <= p.minStock ? (
                      <StatusChip label={`${p.sellableQty}`} color="warning" />
                    ) : (
                      p.sellableQty
                    )}{' '}
                    {p.unit}
                  </TableCell>
                  <TableCell>{formatExpiry(p.nextExpiry)}</TableCell>
                  <TableCell align="right" onClick={(e) => e.stopPropagation()}>
                    {isAdmin && (
                      <Stack direction="row" spacing={1} sx={{ justifyContent: 'flex-end' }}>
                        <Button size="small" onClick={() => setStockIn(p)}>
                          {t.products.stockIn}
                        </Button>
                        <Button size="small" onClick={() => setEditing(p)}>
                          {t.common.edit}
                        </Button>
                      </Stack>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </Card>
      {editing && (
        <ProductDialog
          product={editing === 'new' ? null : editing}
          categories={categories.data ?? []}
          onClose={() => setEditing(null)}
          onSaved={(p) => {
            setEditing(null);
            notify(`${p.name} (${p.sku}) ${t.common.saved.toLowerCase()}`);
            reload();
          }}
        />
      )}
      {stockIn && (
        <StockInDialog
          product={stockIn}
          suppliers={suppliers.data ?? []}
          onClose={() => setStockIn(null)}
          onSaved={(b) => {
            setStockIn(null);
            notify(`${b.productName}: +${b.batchNo} ${t.common.saved.toLowerCase()}`);
            reload();
          }}
        />
      )}
      {detail !== null && <ProductDetailDialog productId={detail} suppliers={suppliers.data ?? []} onClose={() => setDetail(null)} onChanged={reload} />}
    </>
  );
}

/** Batches of one product (FEFO order) with stock in / adjust, and its stock history for admins. */
function ProductDetailDialog({ productId, suppliers, onClose, onChanged }: { productId: number; suppliers: import('../../api').Supplier[]; onClose: () => void; onChanged: () => void }) {
  const { isAdmin, notify } = useApp();
  const [version, setVersion] = useState(0);
  const [stockIn, setStockIn] = useState(false);
  const [adjusting, setAdjusting] = useState<BatchRow | null>(null);
  const { data, error } = useLoader(() => api.getProduct(productId), [productId, version]);
  const ledger = useLoader(() => (isAdmin ? api.listStockLedger(productId, 100) : Promise.resolve([])), [productId, version, isAdmin]);
  const today = todayIso();
  const changed = () => {
    setVersion((v) => v + 1);
    onChanged();
  };

  return (
    <Dialog open onClose={onClose} maxWidth="md" fullWidth>
      <DialogTitle>
        {data ? `${data.product.name} · ${data.product.sku}` : t.common.loading}
      </DialogTitle>
      <DialogContent>
        <ErrorAlert error={error} />
        {!data ? (
          <Loading />
        ) : (
          <Stack spacing={2}>
            <Typography color="text.secondary">
              {[data.product.genericName, data.product.manufacturer, t.products.types[data.product.productType], data.product.categoryName].filter(Boolean).join(' · ')}
            </Typography>
            <Typography variant="subtitle1" sx={{ fontWeight: 600 }}>
              {t.products.batches}
            </Typography>
            {data.batches.length === 0 ? (
              <EmptyState text={t.products.noBatches} />
            ) : (
              <Table size="small">
                <TableHead>
                  <TableRow>
                    <TableCell>{t.products.batchNo}</TableCell>
                    <TableCell>{t.products.expiry}</TableCell>
                    <TableCell>{t.products.supplier}</TableCell>
                    <TableCell align="right">{t.common.price}</TableCell>
                    <TableCell align="right">{t.common.qty}</TableCell>
                    <TableCell />
                  </TableRow>
                </TableHead>
                <TableBody>
                  {data.batches.map((b) => (
                    <TableRow key={b.id}>
                      <TableCell>{b.batchNo}</TableCell>
                      <TableCell>
                        {formatExpiry(b.expiryDate)} {b.expiryDate !== null && b.expiryDate < today && <StatusChip label={t.products.expired} color="error" />}
                      </TableCell>
                      <TableCell>{b.supplierName ?? '—'}</TableCell>
                      <TableCell align="right">{rupees(b.sellingPricePaise)}</TableCell>
                      <TableCell align="right">{b.quantity}</TableCell>
                      <TableCell align="right">
                        {isAdmin && (
                          <Button size="small" onClick={() => setAdjusting(b)}>
                            {t.products.adjust}
                          </Button>
                        )}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
            {isAdmin && (
              <>
                <Typography variant="subtitle1" sx={{ fontWeight: 600 }}>
                  {t.products.history}
                </Typography>
                <LedgerTable rows={ledger.data ?? []} showProduct={false} />
              </>
            )}
          </Stack>
        )}
      </DialogContent>
      <DialogActions>
        {isAdmin && data && <Button onClick={() => setStockIn(true)}>{t.products.stockIn}</Button>}
        <Button variant="contained" onClick={onClose}>
          {t.common.close}
        </Button>
      </DialogActions>
      {stockIn && data && (
        <StockInDialog
          product={data.product}
          suppliers={suppliers}
          onClose={() => setStockIn(false)}
          onSaved={() => {
            setStockIn(false);
            notify(t.common.saved);
            changed();
          }}
        />
      )}
      {adjusting && (
        <AdjustDialog
          batch={adjusting}
          onClose={() => setAdjusting(null)}
          onSaved={() => {
            setAdjusting(null);
            notify(t.common.saved);
            changed();
          }}
        />
      )}
    </Dialog>
  );
}
