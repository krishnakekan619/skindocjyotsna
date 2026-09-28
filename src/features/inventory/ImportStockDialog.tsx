import { useRef, useState, type ChangeEvent } from 'react';
import {
  Alert,
  Box,
  Button,
  Checkbox,
  Chip,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  FormControlLabel,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
  Typography,
} from '@mui/material';
import { api, type ImportPreview, type ImportResult } from '../../api';
import { ErrorAlert } from '../../components/common';
import { t } from '../../i18n/en';
import { formatDateTime, formatIsoDate, newBillKey } from '../../lib/dates';
import { rupees } from '../../lib/money';

const COLUMNS = ['Vendor Name', 'Product Name', 'MRP', 'Clinic Bought Price', 'Expiry Date', 'Quantity'];
const EXAMPLE = [
  ['Derma Pharma', 'Sunscreen SPF 50', '650', '480', '31-12-2027', '10'],
  ['Derma Pharma', 'Moisturising Soap', '120', '80', '', '24'],
];

/**
 * Inventory → Import from CSV (administrators, DEC-041). Choose the file, see every row checked,
 * then import all rows at once. Nothing is imported while any row has a problem.
 */
export function ImportStockDialog({ onImported, onClose }: { onImported: (r: ImportResult) => void; onClose: () => void }) {
  const [fileName, setFileName] = useState('');
  const [text, setText] = useState<string | null>(null);
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [importAgain, setImportAgain] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  // One key per dialog: a double-click or retry imports the file once (the backend checks it).
  const [requestKey] = useState(newBillKey);
  const sending = useRef(false);
  const fileInput = useRef<HTMLInputElement | null>(null);

  const openSheet = async () => {
    setError(null);
    try {
      await api.openStockImportTemplate();
    } catch (e) {
      setError(e);
    }
  };

  const choose = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = ''; // the same file can be chosen again after fixing it
    if (!file) return;
    setError(null);
    setPreview(null);
    setImportAgain(false);
    setFileName(file.name);
    setBusy(true);
    try {
      const content = await file.text();
      setText(content);
      setPreview(await api.previewStockImport(content));
    } catch (err) {
      setText(null);
      setError(err);
    } finally {
      setBusy(false);
    }
  };

  const importAll = async () => {
    if (!text || !preview || sending.current) return;
    sending.current = true;
    setBusy(true);
    setError(null);
    try {
      onImported(await api.importStock(text, requestKey, importAgain));
    } catch (e) {
      setError(e);
    } finally {
      sending.current = false;
      setBusy(false);
    }
  };

  const repeated = preview?.importedBeforeAt ?? null;
  const canImport = !busy && preview !== null && preview.errorRows === 0 && (repeated === null || importAgain);

  return (
    <Dialog open onClose={busy ? undefined : onClose} maxWidth="lg" fullWidth>
      <DialogTitle>{t.stockImport.title}</DialogTitle>
      <DialogContent>
        <Stack spacing={2}>
          {!preview && (
            <>
              <Typography>{t.stockImport.intro}</Typography>
              <Box sx={{ overflowX: 'auto' }}>
                <Table size="small" sx={{ '& td, & th': { whiteSpace: 'nowrap' } }}>
                  <TableHead>
                    <TableRow>
                      {COLUMNS.map((c) => (
                        <TableCell key={c}>{c}</TableCell>
                      ))}
                    </TableRow>
                  </TableHead>
                  <TableBody>
                    {EXAMPLE.map((row) => (
                      <TableRow key={row[1]}>
                        {row.map((cell, i) => (
                          <TableCell key={COLUMNS[i]} sx={{ color: 'text.secondary' }}>
                            {cell || '—'}
                          </TableCell>
                        ))}
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </Box>
              <Box component="ul" sx={{ m: 0, pl: 2.5, '& li': { mb: 0.5 } }}>
                {t.stockImport.rules.map((rule) => (
                  <li key={rule}>
                    <Typography variant="body2">{rule}</Typography>
                  </li>
                ))}
              </Box>
              <Stack direction="row" spacing={1.5} sx={{ flexWrap: 'wrap' }} useFlexGap>
                <Button variant="outlined" onClick={() => void openSheet()}>
                  {t.stockImport.openSheet}
                </Button>
              </Stack>
            </>
          )}

          <Stack direction="row" spacing={1.5} sx={{ alignItems: 'center', flexWrap: 'wrap' }} useFlexGap>
            <Button variant="contained" onClick={() => fileInput.current?.click()} disabled={busy}>
              {preview ? t.stockImport.chooseAgain : t.stockImport.choose}
            </Button>
            {fileName && <Typography color="text.secondary">{fileName}</Typography>}
            <input ref={fileInput} type="file" accept=".csv,text/csv" hidden onChange={(e) => void choose(e)} data-testid="stock-file" />
          </Stack>

          <ErrorAlert error={error} />

          {preview && (
            <>
              {preview.errorRows > 0 ? (
                <Alert severity="error">{t.stockImport.hasErrors(preview.errorRows)}</Alert>
              ) : (
                <Alert severity="success">
                  {t.stockImport.ready(preview.rows.length, preview.totalQty, preview.newProducts, preview.newVendors)}
                </Alert>
              )}
              {repeated !== null && (
                <Alert severity="warning">
                  <Stack spacing={0.5}>
                    <span>{t.stockImport.importedBefore(formatDateTime(repeated))}</span>
                    <FormControlLabel control={<Checkbox checked={importAgain} onChange={(e) => setImportAgain(e.target.checked)} />} label={t.stockImport.importAgain} />
                  </Stack>
                </Alert>
              )}
              <Box sx={{ overflowX: 'auto', maxHeight: 420 }}>
                <Table size="small" stickyHeader>
                  <TableHead>
                    <TableRow>
                      <TableCell>{t.stockImport.line}</TableCell>
                      <TableCell>{t.products.vendor}</TableCell>
                      <TableCell>{t.products.name}</TableCell>
                      <TableCell align="right">{t.products.mrp}</TableCell>
                      <TableCell align="right">{t.products.boughtPrice}</TableCell>
                      <TableCell>{t.products.expiry}</TableCell>
                      <TableCell align="right">{t.common.qty}</TableCell>
                      <TableCell>{t.stockImport.check}</TableCell>
                    </TableRow>
                  </TableHead>
                  <TableBody>
                    {preview.rows.map((r) => (
                      <TableRow key={r.line} sx={r.errors.length > 0 ? { bgcolor: 'rgba(180, 88, 90, 0.08)' } : undefined}>
                        <TableCell>{r.line}</TableCell>
                        <TableCell>
                          {r.vendorName || '—'} {r.newVendor && <Chip size="small" label={t.stockImport.newTag} />}
                        </TableCell>
                        <TableCell>
                          {r.productName || '—'} {r.newProduct && <Chip size="small" label={t.stockImport.newTag} />}
                        </TableCell>
                        <TableCell align="right">{r.mrpPaise !== null ? rupees(r.mrpPaise) : '—'}</TableCell>
                        <TableCell align="right">{r.purchasePricePaise !== null ? rupees(r.purchasePricePaise) : '—'}</TableCell>
                        <TableCell>{r.expiryDate ? formatIsoDate(r.expiryDate) : t.stockImport.noExpiry}</TableCell>
                        <TableCell align="right">{r.qty ?? '—'}</TableCell>
                        <TableCell>
                          {r.errors.length === 0 && r.warnings.length === 0 && <Chip size="small" color="success" label={t.stockImport.ok} />}
                          {r.errors.map((m) => (
                            <Typography key={m} variant="body2" color="error">
                              {m}
                            </Typography>
                          ))}
                          {r.warnings.map((m) => (
                            <Typography key={m} variant="body2" color="warning.main">
                              {m}
                            </Typography>
                          ))}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </Box>
            </>
          )}
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} disabled={busy}>
          {t.common.cancel}
        </Button>
        <Button variant="contained" onClick={() => void importAll()} disabled={!canImport}>
          {preview ? t.stockImport.importRows(preview.rows.length) : t.stockImport.importRows(0)}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
