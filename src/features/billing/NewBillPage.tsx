import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Alert,
  Autocomplete,
  Box,
  Button,
  Card,
  CardContent,
  Chip,
  Dialog,
  DialogActions,
  DialogContent,
  DialogContentText,
  DialogTitle,
  IconButton,
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
import {
  api,
  isCommandError,
  PAYMENT_METHODS,
  type BillDetail,
  type BillInput,
  type BillLineInput,
  type ClientRow,
  type Discount,
  type PaymentMethod,
  type Quote,
  type SaleProduct,
} from '../../api';
import { useApp } from '../../app/AppContext';
import { ConfirmDialog, ErrorAlert, PageHeader } from '../../components/common';
import { t } from '../../i18n/en';
import { formatExpiry, newBillKey } from '../../lib/dates';
import { paiseToInput, parseRupees, percentLabel, rupees } from '../../lib/money';
import { ClientDialog } from '../clients/ClientDialog';
import { BillDetailDialog } from './BillDetailDialog';

interface Line {
  product: SaleProduct;
  qty: number;
  notSuppliedQty: number;
}

type DiscountKind = Discount['kind'];
interface PaymentDraft {
  method: PaymentMethod;
  amount: string;
  reference: string;
}

function discountOf(kind: DiscountKind, text: string): Discount | null {
  if (kind === 'NONE' || text.trim() === '') return { kind: 'NONE' };
  const paise = parseRupees(text);
  if (paise === null) return null;
  // "10" in percent mode means 10% = 1000 basis points; parseRupees("10") = 1000.
  return kind === 'PERCENT' ? { kind: 'PERCENT', value: paise } : { kind: 'AMOUNT', value: paise };
}

/** Starting lines for a correction: what the original bill contained (DEC-001). */
function linesFrom(detail: BillDetail): Line[] {
  const byProduct = new Map<number, Line>();
  for (const item of detail.items) {
    const existing = byProduct.get(item.productId);
    const product: SaleProduct = existing?.product ?? {
      productId: item.productId,
      name: item.productName,
      genericName: '',
      sku: item.sku,
      unit: item.unit,
      gstRateBp: item.gstRateBp,
      pricePaise: item.unitPricePaise,
      availableQty: Number.MAX_SAFE_INTEGER,
      nextExpiry: null,
      expiresSoon: false,
    };
    byProduct.set(item.productId, {
      product,
      qty: (existing?.qty ?? 0) + (item.qty - item.returnedQty),
      notSuppliedQty: (existing?.notSuppliedQty ?? 0) + item.notSuppliedQty,
    });
  }
  return [...byProduct.values()].filter((l) => l.qty + l.notSuppliedQty > 0);
}

/** POS-style bill: client → product → qty → Enter → … → payment → F9 (design §9.3). */
export function NewBillPage({ initialClientId, correcting }: { initialClientId?: number | undefined; correcting?: BillDetail | undefined }) {
  const { notify, navigate } = useApp();
  const [billKey, setBillKey] = useState(newBillKey);
  const [client, setClient] = useState<ClientRow | null>(null);
  const [clientOptions, setClientOptions] = useState<ClientRow[]>([]);
  const [clientText, setClientText] = useState('');
  const [addingClient, setAddingClient] = useState(false);
  const [lines, setLines] = useState<Line[]>(() => (correcting ? linesFrom(correcting) : []));
  const [productText, setProductText] = useState('');
  const [productOptions, setProductOptions] = useState<SaleProduct[]>([]);
  const [picked, setPicked] = useState<SaleProduct | null>(null);
  const [qtyText, setQtyText] = useState('1');
  const [partial, setPartial] = useState<{ product: SaleProduct; requested: number } | null>(null);
  const [discountKind, setDiscountKind] = useState<DiscountKind>('NONE');
  const [discountText, setDiscountText] = useState('');
  const [split, setSplit] = useState(false);
  const [payments, setPayments] = useState<PaymentDraft[]>([{ method: 'CASH', amount: '', reference: '' }]);
  const [receivedText, setReceivedText] = useState('');
  const [note, setNote] = useState('');
  const [reason, setReason] = useState('');
  const [quote, setQuote] = useState<Quote | null>(null);
  const [quoteError, setQuoteError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [approval, setApproval] = useState<{ username: string; password: string } | null>(null);
  const [needApproval, setNeedApproval] = useState(false);
  const [approvalMessage, setApprovalMessage] = useState('');
  const [confirmClear, setConfirmClear] = useState(false);
  const [done, setDone] = useState<BillDetail | null>(null);
  const productInput = useRef<HTMLInputElement | null>(null);
  const qtyInput = useRef<HTMLInputElement | null>(null);

  // Preselected client (from the client screen or the bill being corrected).
  useEffect(() => {
    const id = correcting?.bill.clientId ?? initialClientId;
    if (id) api.getClientProfile(id, null, null).then((p) => setClient(p.client)).catch(() => undefined);
  }, [initialClientId, correcting]);

  // Client and product search, debounced.
  useEffect(() => {
    const timer = window.setTimeout(() => api.searchClients(clientText, false).then(setClientOptions).catch(() => undefined), 200);
    return () => window.clearTimeout(timer);
  }, [clientText]);
  useEffect(() => {
    if (!productText.trim()) {
      setProductOptions([]);
      return;
    }
    const timer = window.setTimeout(() => api.searchProductsForSale(productText).then(setProductOptions).catch(() => undefined), 150);
    return () => window.clearTimeout(timer);
  }, [productText]);

  const discount = discountOf(discountKind, discountText);
  const lineInputs: BillLineInput[] = useMemo(
    () => lines.filter((l) => l.qty + l.notSuppliedQty > 0).map((l) => ({ productId: l.product.productId, qty: l.qty, notSuppliedQty: l.notSuppliedQty })),
    [lines],
  );

  // All money arithmetic happens in Rust: the screen asks for a quote whenever the bill changes.
  useEffect(() => {
    if (lineInputs.length === 0 || discount === null) {
      setQuote(null);
      setQuoteError(null);
      return;
    }
    let current = true;
    const timer = window.setTimeout(() => {
      api
        .quoteBill(lineInputs, discount, correcting?.bill.id ?? null)
        .then((q) => current && (setQuote(q), setQuoteError(null)))
        .catch((e: unknown) => current && (setQuote(null), setQuoteError(e)));
    }, 120);
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lineInputs, discountKind, discountText, correcting]);

  const total = quote?.totalPaise ?? 0;
  const paymentDrafts: PaymentDraft[] = split ? payments : [{ ...(payments[0] ?? { method: 'CASH', reference: '' }), amount: paiseToInput(total) } as PaymentDraft];
  const parsedPayments = paymentDrafts.map((p) => ({ ...p, paise: parseRupees(p.amount || '0') }));
  const paidPaise = parsedPayments.reduce((sum, p) => sum + (p.paise ?? 0), 0);
  const cashPaise = parsedPayments.filter((p) => p.method === 'CASH').reduce((sum, p) => sum + (p.paise ?? 0), 0);
  const received = cashPaise > 0 && receivedText.trim() ? parseRupees(receivedText) : null;
  const changePaise = received !== null ? received - cashPaise : null;

  const focusProduct = () => window.setTimeout(() => productInput.current?.focus(), 0);

  const addLine = (product: SaleProduct, qty: number, notSupplied: number) => {
    setLines((current) => {
      const existing = current.find((l) => l.product.productId === product.productId);
      if (existing) return current.map((l) => (l === existing ? { ...l, qty: l.qty + qty, notSuppliedQty: l.notSuppliedQty + notSupplied } : l));
      return [...current, { product, qty, notSuppliedQty: notSupplied }];
    });
    setPicked(null);
    setProductText('');
    setQtyText('1');
    focusProduct();
  };

  const tryAdd = () => {
    const qty = Number.parseInt(qtyText, 10);
    if (!picked || !Number.isInteger(qty) || qty <= 0) return;
    const alreadyInBill = lines.find((l) => l.product.productId === picked.productId)?.qty ?? 0;
    const available = Math.max(0, picked.availableQty - alreadyInBill);
    if (correcting || qty <= available) addLine(picked, qty, 0);
    else setPartial({ product: { ...picked, availableQty: available }, requested: qty }); // DEC-007: ask
  };

  const setLineQty = (productId: number, qty: number) =>
    setLines((current) => current.map((l) => (l.product.productId === productId ? { ...l, qty: Math.max(0, qty) } : l)));

  const reset = () => {
    setBillKey(newBillKey());
    setLines([]);
    setClient(null);
    setDiscountKind('NONE');
    setDiscountText('');
    setSplit(false);
    setPayments([{ method: 'CASH', amount: '', reference: '' }]);
    setReceivedText('');
    setNote('');
    setApproval(null);
    setError(null);
  };

  const finalize = useCallback(
    async (withApproval: { username: string; password: string } | null) => {
      if (busy || lineInputs.length === 0 || !quote || discount === null) return;
      setBusy(true); // disables Finalize at once: a double-click cannot submit twice (D10)
      setError(null);
      const input: BillInput = {
        idempotencyKey: billKey,
        clientId: client?.id ?? null,
        lines: lineInputs,
        discount,
        payments: total === 0 ? [] : parsedPayments.map((p) => ({ method: p.method, amountPaise: p.paise ?? 0, reference: p.reference })).filter((p) => p.amountPaise > 0),
        amountReceivedPaise: received,
        note,
        approval: withApproval,
      };
      try {
        const saved = correcting ? await api.correctBill({ originalBillId: correcting.bill.id, reason, bill: input }) : await api.finalizeBill(input);
        notify(t.billing.saved(saved.bill.billNo));
        setDone(saved);
        reset();
        setNeedApproval(false);
      } catch (e) {
        if (isCommandError(e) && (e.code === 'DISCOUNT_APPROVAL_REQUIRED' || e.field === 'approval')) {
          setApproval(null);
          setApprovalMessage(e.message);
          setNeedApproval(true);
        } else setError(e);
      } finally {
        setBusy(false);
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [busy, lines, quote, discount, billKey, client, lineInputs, total, parsedPayments, received, note, correcting, reason],
  );

  const quoteFor = (productId: number) => quote?.lines.filter((q) => q.productId === productId) ?? [];
  const paymentsOk = total === 0 || paidPaise === total;
  const canFinalize =
    !busy && lineInputs.length > 0 && quote !== null && paymentsOk && (!correcting || reason.trim().length >= 3) && (changePaise === null || changePaise >= 0);
  const dialogOpen = partial !== null || needApproval || confirmClear || addingClient || done !== null;

  // F2: product search, F9: finalize (same checks as the button, and not behind a dialog).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'F2') {
        e.preventDefault();
        productInput.current?.focus();
      } else if (e.key === 'F9') {
        e.preventDefault();
        if (canFinalize && !dialogOpen) void finalize(approval);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [finalize, approval, canFinalize, dialogOpen]);


  return (
    <>
      <PageHeader
        title={correcting ? t.billing.correctionTitle(correcting.bill.billNo) : t.billing.title}
        subtitle={correcting ? t.billing.correctionIntro : undefined}
        actions={lines.length > 0 ? <Button color="error" onClick={() => setConfirmClear(true)}>{t.billing.clear}</Button> : undefined}
      />
      <Stack spacing={2}>
        <Card variant="outlined">
          <CardContent>
            <Stack direction="row" spacing={2} sx={{ alignItems: 'center' }}>
              <Autocomplete
                sx={{ flexGrow: 1, maxWidth: 520 }}
                options={clientOptions}
                value={client}
                onChange={(_, value) => setClient(value)}
                inputValue={clientText}
                onInputChange={(_, value) => setClientText(value)}
                getOptionLabel={(c) => `${c.fullName} · ${c.clientCode}${c.phone ? ` · ${c.phone}` : ''}`}
                isOptionEqualToValue={(a, b) => a.id === b.id}
                filterOptions={(x) => x}
                renderInput={(params) => <TextField {...params} label={t.billing.client} placeholder={t.billing.clientSearch} />}
              />
              {!client && <Chip label={t.billing.walkIn} variant="outlined" />}
              <Button onClick={() => setAddingClient(true)}>{t.billing.newClient}</Button>
            </Stack>
          </CardContent>
        </Card>

        <Card variant="outlined">
          <CardContent>
            <Stack direction="row" spacing={2} sx={{ mb: 2, alignItems: 'center' }}>
              <Autocomplete
                sx={{ flexGrow: 1 }}
                options={productOptions}
                value={picked}
                onChange={(_, value) => {
                  setPicked(value);
                  if (value) window.setTimeout(() => qtyInput.current?.select(), 0);
                }}
                inputValue={productText}
                onInputChange={(_, value, why) => why !== 'reset' && setProductText(value)}
                getOptionLabel={(p) => p.name}
                isOptionEqualToValue={(a, b) => a.productId === b.productId}
                filterOptions={(x) => x}
                noOptionsText={productText ? t.products.empty : t.billing.noLines}
                renderOption={({ key, ...props }, p) => (
                  <li key={key} {...props}>
                    <Box sx={{ width: '100%' }}>
                      <Typography sx={{ fontWeight: 600 }}>
                        {p.name} <Typography component="span" color="text.secondary">{p.genericName} · {p.unit}</Typography>
                      </Typography>
                      <Typography variant="body2" color={p.availableQty > 0 ? 'text.secondary' : 'error'}>
                        {rupees(p.pricePaise)} · {p.availableQty > 0 ? t.billing.available(p.availableQty, p.unit) : t.products.filterOut}
                        {p.nextExpiry ? ` · ${t.products.expiry} ${formatExpiry(p.nextExpiry)}` : ''}
                        {p.expiresSoon ? ` · ⚠ ${t.billing.expiresSoon}` : ''}
                      </Typography>
                    </Box>
                  </li>
                )}
                renderInput={(params) => <TextField {...params} label={t.billing.product} inputRef={productInput} autoFocus />}
              />
              <TextField
                label={t.common.qty}
                value={qtyText}
                onChange={(e) => setQtyText(e.target.value.replace(/\D/g, '').slice(0, 5))}
                onKeyDown={(e) => e.key === 'Enter' && tryAdd()}
                inputRef={qtyInput}
                sx={{ width: 90 }}
                slotProps={{ htmlInput: { inputMode: 'numeric' } }}
              />
              <Button variant="contained" onClick={tryAdd} disabled={!picked}>
                {t.billing.add}
              </Button>
            </Stack>

            {lines.length === 0 ? (
              <Typography color="text.secondary">{t.billing.noLines}</Typography>
            ) : (
              <Table size="small">
                <TableHead>
                  <TableRow>
                    <TableCell>{t.products.name}</TableCell>
                    <TableCell>{t.products.batchNo}</TableCell>
                    <TableCell align="right">{t.common.qty}</TableCell>
                    <TableCell align="right">{t.common.price}</TableCell>
                    <TableCell align="right">{t.products.gst}</TableCell>
                    <TableCell align="right">{t.common.amount}</TableCell>
                    <TableCell />
                  </TableRow>
                </TableHead>
                <TableBody>
                  {lines.map((line) => {
                    const quoted = quoteFor(line.product.productId).filter((q) => q.qty > 0);
                    const net = quoted.reduce((s, q) => s + q.netPaise, 0);
                    return (
                      <TableRow key={line.product.productId}>
                        <TableCell>
                          {line.product.name}
                          {line.notSuppliedQty > 0 && (
                            <Typography variant="body2" color="error">
                              {t.billing.notSupplied}: {line.notSuppliedQty}
                            </Typography>
                          )}
                          {quoted.some((q) => q.expiresSoon) && <Chip size="small" color="warning" label={t.billing.expiresSoon} sx={{ ml: 1 }} />}
                        </TableCell>
                        <TableCell>{quoted.flatMap((q) => q.batchNos).join(', ') || '—'}</TableCell>
                        <TableCell align="right">
                          <TextField
                            size="small"
                            value={line.qty}
                            onChange={(e) => setLineQty(line.product.productId, Number.parseInt(e.target.value.replace(/\D/g, '') || '0', 10))}
                            sx={{ width: 80 }}
                            slotProps={{ htmlInput: { inputMode: 'numeric', style: { textAlign: 'right' } } }}
                          />
                        </TableCell>
                        <TableCell align="right">{quoted.length ? quoted.map((q) => rupees(q.unitPricePaise)).join(' / ') : '—'}</TableCell>
                        <TableCell align="right">{percentLabel(line.product.gstRateBp)}</TableCell>
                        <TableCell align="right">{rupees(net)}</TableCell>
                        <TableCell align="right">
                          <IconButton aria-label="remove" onClick={() => setLines((c) => c.filter((l) => l !== line))}>
                            ✕
                          </IconButton>
                        </TableCell>
                      </TableRow>
                    );
                  })}
                </TableBody>
              </Table>
            )}
            <ErrorAlert error={quoteError} />
          </CardContent>
        </Card>

        <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: '1fr 380px' }, gap: 2 }}>
          <Card variant="outlined">
            <CardContent>
              <Stack spacing={2}>
                <Stack direction="row" spacing={2} sx={{ alignItems: 'center' }}>
                  <Typography sx={{ minWidth: 110 }}>{t.billing.discount}</Typography>
                  <ToggleButtonGroup size="small" exclusive value={discountKind} onChange={(_, v: DiscountKind | null) => v && setDiscountKind(v)}>
                    <ToggleButton value="NONE">{t.billing.discountNone}</ToggleButton>
                    <ToggleButton value="PERCENT">{t.billing.discountPercent}</ToggleButton>
                    <ToggleButton value="AMOUNT">{t.billing.discountAmount}</ToggleButton>
                  </ToggleButtonGroup>
                  {discountKind !== 'NONE' && <TextField size="small" value={discountText} onChange={(e) => setDiscountText(e.target.value)} error={discount === null} sx={{ width: 120 }} />}
                </Stack>
                <Stack direction="row" spacing={2} sx={{ alignItems: 'center' }}>
                  <Typography sx={{ minWidth: 110 }}>{t.billing.payment}</Typography>
                  {!split && (
                    <ToggleButtonGroup
                      size="small"
                      exclusive
                      value={payments[0]?.method ?? 'CASH'}
                      onChange={(_, v: PaymentMethod | null) => v && setPayments([{ method: v, amount: '', reference: payments[0]?.reference ?? '' }])}
                    >
                      {PAYMENT_METHODS.map((m) => (
                        <ToggleButton key={m} value={m}>
                          {t.billing.methods[m]}
                        </ToggleButton>
                      ))}
                    </ToggleButtonGroup>
                  )}
                  <Button size="small" onClick={() => { setSplit(!split); setPayments([{ method: 'CASH', amount: '', reference: '' }, { method: 'UPI', amount: '', reference: '' }]); }}>
                    {split ? t.common.cancel : t.billing.split}
                  </Button>
                </Stack>
                {split &&
                  payments.map((p, index) => (
                    <Stack direction="row" spacing={1} key={index}>
                      <TextField select size="small" value={p.method} onChange={(e) => setPayments(payments.map((x, i) => (i === index ? { ...x, method: e.target.value as PaymentMethod } : x)))} sx={{ width: 120 }}>
                        {PAYMENT_METHODS.map((m) => (
                          <MenuItem key={m} value={m}>
                            {t.billing.methods[m]}
                          </MenuItem>
                        ))}
                      </TextField>
                      <TextField size="small" label="₹" value={p.amount} onChange={(e) => setPayments(payments.map((x, i) => (i === index ? { ...x, amount: e.target.value } : x)))} sx={{ width: 120 }} />
                      <TextField size="small" label={t.billing.reference} value={p.reference} onChange={(e) => setPayments(payments.map((x, i) => (i === index ? { ...x, reference: e.target.value } : x)))} />
                    </Stack>
                  ))}
                {(split ? payments.some((p) => p.method === 'CASH') : payments[0]?.method === 'CASH') && total > 0 && (
                  <Stack direction="row" spacing={2} sx={{ alignItems: 'center' }}>
                    <TextField size="small" label={t.billing.received} value={receivedText} onChange={(e) => setReceivedText(e.target.value)} sx={{ width: 170 }} />
                    {changePaise !== null && (
                      <Typography color={changePaise < 0 ? 'error' : 'text.primary'} sx={{ fontWeight: 600 }}>
                        {t.billing.change}: {rupees(changePaise)}
                      </Typography>
                    )}
                  </Stack>
                )}
                <TextField size="small" label={t.billing.note} value={note} onChange={(e) => setNote(e.target.value)} />
                {correcting && <TextField label={t.billing.reason} value={reason} onChange={(e) => setReason(e.target.value)} required />}
              </Stack>
            </CardContent>
          </Card>

          <Card variant="outlined">
            <CardContent>
              <Stack spacing={1}>
                <Row label={t.billing.subtotal} value={rupees(quote?.subtotalPaise ?? 0)} />
                {(quote?.discountPaise ?? 0) > 0 && <Row label={t.billing.discount} value={`-${rupees(quote?.discountPaise ?? 0)}`} />}
                {(quote?.taxPaise ?? 0) > 0 && <Row label={t.billing.gstIncluded} value={rupees(quote?.taxPaise ?? 0)} />}
                {(quote?.roundOffPaise ?? 0) !== 0 && <Row label={t.billing.roundOff} value={rupees(quote?.roundOffPaise ?? 0)} />}
                <Row label={t.billing.total} value={rupees(total)} strong />
                {split && !paymentsOk && <Alert severity="warning">{t.billing.paymentsMismatch} ({rupees(paidPaise)} / {rupees(total)})</Alert>}
                <ErrorAlert error={error} />
                <Button variant="contained" size="large" disabled={!canFinalize} onClick={() => void finalize(approval)}>
                  {busy ? t.billing.finalizing : t.billing.finalize}
                </Button>
              </Stack>
            </CardContent>
          </Card>
        </Box>
      </Stack>

      {partial && (
        <Dialog open onClose={() => setPartial(null)}>
          <DialogTitle>{t.billing.partialTitle}</DialogTitle>
          <DialogContent>
            <DialogContentText>{t.billing.partialText(partial.product.availableQty, partial.requested, partial.product.unit)}</DialogContentText>
          </DialogContent>
          <DialogActions>
            <Button onClick={() => { setPartial(null); qtyInput.current?.select(); }}>{t.billing.changeQty}</Button>
            {partial.product.availableQty > 0 ? (
              <Button variant="contained" onClick={() => { addLine(partial.product, partial.product.availableQty, partial.requested - partial.product.availableQty); setPartial(null); }}>
                {t.billing.supplyAvailable(partial.product.availableQty)}
              </Button>
            ) : (
              <Button variant="contained" onClick={() => { addLine(partial.product, 0, partial.requested); setPartial(null); }}>
                {t.billing.recordNotSupplied}
              </Button>
            )}
          </DialogActions>
        </Dialog>
      )}

      {needApproval && (
        <ApprovalDialog
          message={approvalMessage}
          onCancel={() => setNeedApproval(false)}
          onApprove={(credentials) => {
            setApproval(credentials);
            setNeedApproval(false);
            void finalize(credentials);
          }}
        />
      )}
      <ConfirmDialog open={confirmClear} title={t.billing.clear} text={t.billing.clearConfirm} confirmLabel={t.billing.clear} danger onConfirm={() => reset()} onClose={() => setConfirmClear(false)} />
      {addingClient && (
        <ClientDialog
          client={null}
          initialName={clientText}
          onClose={() => setAddingClient(false)}
          onSaved={(c) => {
            setClient(c);
            setAddingClient(false);
          }}
        />
      )}
      {done && (
        <BillDetailDialog
          billId={done.bill.id}
          onClose={() => {
            setDone(null);
            if (correcting) navigate({ name: 'bills' });
            else focusProduct();
          }}
          onChanged={() => undefined}
        />
      )}
    </>
  );
}

function Row({ label, value, strong }: { label: string; value: string; strong?: boolean }) {
  return (
    <Stack direction="row" sx={{ justifyContent: 'space-between' }}>
      <Typography sx={{ fontWeight: strong ? 700 : 400, fontSize: strong ? '1.25rem' : undefined }}>{label}</Typography>
      <Typography sx={{ fontWeight: strong ? 700 : 400, fontSize: strong ? '1.25rem' : undefined, fontVariantNumeric: 'tabular-nums' }}>{value}</Typography>
    </Stack>
  );
}

function ApprovalDialog({ message, onApprove, onCancel }: { message: string; onApprove: (c: { username: string; password: string }) => void; onCancel: () => void }) {
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  return (
    <Dialog open onClose={onCancel} maxWidth="xs" fullWidth>
      <DialogTitle>{t.billing.approvalTitle}</DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ pt: 1 }}>
          <DialogContentText>{message || t.billing.approvalTitle}</DialogContentText>
          <TextField label={t.account.username} value={username} onChange={(e) => setUsername(e.target.value)} autoFocus />
          <TextField label={t.account.password} type="password" value={password} onChange={(e) => setPassword(e.target.value)} />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onCancel}>{t.common.cancel}</Button>
        <Button variant="contained" disabled={!username || !password} onClick={() => onApprove({ username, password })}>
          {t.billing.approve}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
