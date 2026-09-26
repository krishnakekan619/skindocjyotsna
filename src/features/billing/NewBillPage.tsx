import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import {
  Alert,
  Autocomplete,
  Box,
  Button,
  ButtonGroup,
  Card,
  CardContent,
  Chip,
  Dialog,
  DialogActions,
  DialogContent,
  DialogContentText,
  DialogTitle,
  IconButton,
  Menu,
  MenuItem,
  Stack,
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
  type ServiceKind,
  type ServiceLineInput,
  type ServiceRow,
} from '../../api';
import { useApp } from '../../app/AppContext';
import { ConfirmDialog, ErrorAlert, PageHeader } from '../../components/common';
import { MOD_KEY, t } from '../../i18n/en';
import { formatDateTime, formatExpiry, newBillKey } from '../../lib/dates';
import { paiseToInput, parseRupees, rupees } from '../../lib/money';
import { ClientDialog } from '../clients/ClientDialog';
import { BillDetailDialog } from './BillDetailDialog';

interface Line {
  product: SaleProduct;
  qty: number;
  notSuppliedQty: number;
}

interface ServiceLine {
  key: number;
  service: ServiceRow;
  qty: number;
  /** Price as typed, in rupees. */
  price: string;
}

type DiscountKind = Discount['kind'];
interface PaymentDraft {
  method: PaymentMethod;
  amount: string;
  reference: string;
}

/** A client suggestion, or the "create new client" choice at the end of the list. */
type ClientOption = ClientRow | { create: string };
const isCreate = (o: ClientOption): o is { create: string } => 'create' in o;

let serviceKeys = 0;

function discountOf(kind: DiscountKind, text: string): Discount | null {
  if (kind === 'NONE' || text.trim() === '') return { kind: 'NONE' };
  const paise = parseRupees(text);
  if (paise === null) return null;
  // "10" in percent mode means 10% = 1000 basis points; parseRupees("10") = 1000.
  return kind === 'PERCENT' ? { kind: 'PERCENT', value: paise } : { kind: 'AMOUNT', value: paise };
}

/** Starting product lines for a correction: what the original bill contained (DEC-001). */
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

/** Starting consultation/procedure lines for a correction. */
function servicesFrom(detail: BillDetail, catalog: ServiceRow[]): ServiceLine[] {
  return detail.services.map((s) => ({
    key: ++serviceKeys,
    service: catalog.find((c) => c.id === s.serviceId) ?? {
      id: s.serviceId,
      kind: s.kind,
      name: s.name,
      defaultPricePaise: s.defaultPricePaise,
      gstRateBp: s.gstRateBp,
      discountEligible: s.discountEligible,
      isActive: true,
      sortOrder: 0,
    },
    qty: s.qty,
    price: paiseToInput(s.unitPricePaise),
  }));
}

/**
 * The receptionist's main screen (v0.3 brief §3-13): client, consultation, procedures and
 * medicines on ONE screen, then payment and Finalize. All amounts are worked out in Rust.
 */
export function NewBillPage({ initialClientId, correcting }: { initialClientId?: number | undefined; correcting?: BillDetail | undefined }) {
  const { notify, navigate } = useApp();
  const [billKey, setBillKey] = useState(newBillKey);
  // Client
  const [client, setClient] = useState<ClientRow | null>(null);
  const [clientText, setClientText] = useState('');
  const [clientOptions, setClientOptions] = useState<ClientRow[]>([]);
  const [recentClients, setRecentClients] = useState<ClientRow[]>([]);
  const [creatingClient, setCreatingClient] = useState<string | null>(null);
  // Consultations and procedures
  const [catalog, setCatalog] = useState<ServiceRow[]>([]);
  const [serviceLines, setServiceLines] = useState<ServiceLine[]>([]);
  const [menu, setMenu] = useState<{ kind: ServiceKind; anchor: HTMLElement } | null>(null);
  // Medicines and products
  const [lines, setLines] = useState<Line[]>(() => (correcting ? linesFrom(correcting) : []));
  const [recentProducts, setRecentProducts] = useState<SaleProduct[]>([]);
  const [productText, setProductText] = useState('');
  const [productOptions, setProductOptions] = useState<SaleProduct[]>([]);
  const [picked, setPicked] = useState<SaleProduct | null>(null);
  const [qtyText, setQtyText] = useState('1');
  const [partial, setPartial] = useState<{ product: SaleProduct; requested: number } | null>(null);
  // Totals and payment
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
  const [done, setDone] = useState<{ detail: BillDetail; print: boolean } | null>(null);
  const clientInput = useRef<HTMLInputElement | null>(null);
  const productInput = useRef<HTMLInputElement | null>(null);
  const qtyInput = useRef<HTMLInputElement | null>(null);

  // Catalog, recent clients and recent products: loaded once.
  const loadShortcuts = useCallback(() => {
    api.searchClients('', false).then((rows) => setRecentClients(rows.slice(0, 8))).catch(() => undefined);
    api.recentProductsForSale().then(setRecentProducts).catch(() => undefined);
  }, []);
  useEffect(() => {
    loadShortcuts();
    api
      .listServices(false)
      .then((rows) => {
        setCatalog(rows);
        if (correcting) setServiceLines(servicesFrom(correcting, rows));
      })
      .catch(() => undefined);
  }, [loadShortcuts, correcting]);

  // Preselected client (from the client screen or the bill being corrected).
  useEffect(() => {
    const id = correcting?.bill.clientId ?? initialClientId;
    if (id) api.getClientProfile(id, null, null).then((p) => setClient(p.client)).catch(() => undefined);
  }, [initialClientId, correcting]);

  // Client and product search: from the 2nd character, debounced, done in the database.
  useEffect(() => {
    if (clientText.trim().length < 2) {
      setClientOptions([]);
      return;
    }
    const timer = window.setTimeout(() => api.searchClients(clientText, false).then((rows) => setClientOptions(rows.slice(0, 8))).catch(() => undefined), 200);
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
  const servicesValid = serviceLines.every((s) => s.qty > 0 && parseRupees(s.price || '0') !== null);
  const serviceInputs: ServiceLineInput[] = useMemo(
    () =>
      serviceLines.map((s) => {
        const paise = parseRupees(s.price || '0');
        return { serviceId: s.service.id, qty: s.qty, unitPricePaise: paise === null || paise === s.service.defaultPricePaise ? null : paise };
      }),
    [serviceLines],
  );
  const itemCount = lineInputs.length + serviceInputs.length;

  // All money arithmetic happens in Rust: the screen asks for a quote whenever the bill changes.
  useEffect(() => {
    if (itemCount === 0 || discount === null || !servicesValid) {
      setQuote(null);
      setQuoteError(null);
      return;
    }
    let current = true;
    const timer = window.setTimeout(() => {
      api
        .quoteBill(lineInputs, serviceInputs, discount, correcting?.bill.id ?? null)
        .then((q) => {
          if (current) {
            setQuote(q);
            setQuoteError(null);
          }
        })
        .catch((e: unknown) => {
          if (current) {
            setQuote(null);
            setQuoteError(e);
          }
        });
    }, 120);
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lineInputs, serviceInputs, discountKind, discountText, correcting, servicesValid]);

  const total = quote?.totalPaise ?? 0;
  const paymentDrafts: PaymentDraft[] = split ? payments : [{ method: payments[0]?.method ?? 'CASH', reference: payments[0]?.reference ?? '', amount: paiseToInput(total) }];
  const parsedPayments = paymentDrafts.map((p) => ({ ...p, paise: parseRupees(p.amount || '0') }));
  const paidPaise = parsedPayments.reduce((sum, p) => sum + (p.paise ?? 0), 0);
  const cashPaise = parsedPayments.filter((p) => p.method === 'CASH').reduce((sum, p) => sum + (p.paise ?? 0), 0);
  const received = cashPaise > 0 && receivedText.trim() ? parseRupees(receivedText) : null;
  const changePaise = received !== null ? received - cashPaise : null;
  const hasProducts = lineInputs.some((l) => l.qty > 0);

  const focusProduct = () => window.setTimeout(() => productInput.current?.focus(), 0);

  // ---- Consultations and procedures ----
  const consultations = catalog.filter((s) => s.kind === 'CONSULTATION');
  const procedures = catalog.filter((s) => s.kind === 'PROCEDURE');
  const standardConsultation = consultations[0];
  const addService = (service: ServiceRow) => {
    setServiceLines((current) => [...current, { key: ++serviceKeys, service, qty: 1, price: paiseToInput(service.defaultPricePaise) }]);
    setMenu(null);
  };
  const updateService = (key: number, patch: Partial<ServiceLine>) => setServiceLines((current) => current.map((s) => (s.key === key ? { ...s, ...patch } : s)));
  const removeService = (key: number) => setServiceLines((current) => current.filter((s) => s.key !== key));

  // ---- Medicines and products ----
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

  const tryAdd = (product: SaleProduct | null, qtyValue: string) => {
    const qty = Number.parseInt(qtyValue, 10);
    if (!product || !Number.isInteger(qty) || qty <= 0) return;
    const alreadyInBill = lines.find((l) => l.product.productId === product.productId)?.qty ?? 0;
    const available = Math.max(0, product.availableQty - alreadyInBill);
    // When correcting, the original bill's stock comes back first; the quote decides.
    if (correcting || qty <= available) addLine(product, qty, 0);
    else setPartial({ product: { ...product, availableQty: available }, requested: qty }); // DEC-007: ask
  };

  const setLineQty = (productId: number, qty: number) => setLines((current) => current.map((l) => (l.product.productId === productId ? { ...l, qty: Math.max(0, qty) } : l)));

  const reset = () => {
    setBillKey(newBillKey());
    setLines([]);
    setServiceLines([]);
    setClient(null);
    setClientText('');
    setDiscountKind('NONE');
    setDiscountText('');
    setSplit(false);
    setPayments([{ method: 'CASH', amount: '', reference: '' }]);
    setReceivedText('');
    setNote('');
    setApproval(null);
    setError(null);
  };

  const paymentsOk = total === 0 || paidPaise === total;
  const canFinalize =
    !busy && itemCount > 0 && servicesValid && quote !== null && paymentsOk && (!correcting || reason.trim().length >= 3) && (changePaise === null || changePaise >= 0);
  const dialogOpen = partial !== null || needApproval || confirmClear || creatingClient !== null || done !== null || menu !== null;

  const finalize = useCallback(
    async (withApproval: { username: string; password: string } | null, print: boolean) => {
      if (busy || itemCount === 0 || !quote || discount === null) return;
      setBusy(true); // disables Finalize at once: a double-click cannot submit twice (D10)
      setError(null);
      const input: BillInput = {
        idempotencyKey: billKey,
        clientId: client?.id ?? null,
        lines: lineInputs,
        services: serviceInputs,
        discount,
        payments: total === 0 ? [] : parsedPayments.map((p) => ({ method: p.method, amountPaise: p.paise ?? 0, reference: p.reference })).filter((p) => p.amountPaise > 0),
        amountReceivedPaise: received,
        note,
        approval: withApproval,
      };
      try {
        const saved = correcting ? await api.correctBill({ originalBillId: correcting.bill.id, reason, bill: input }) : await api.finalizeBill(input);
        setDone({ detail: saved, print });
        reset();
        setNeedApproval(false);
        loadShortcuts();
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
    [busy, itemCount, quote, discount, billKey, client, lineInputs, serviceInputs, total, parsedPayments, received, note, correcting, reason],
  );

  // F2: client search, F4: product search, Ctrl/Cmd+Enter (or F9): finalize & print.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'F2') {
        e.preventDefault();
        clientInput.current?.focus();
      } else if (e.key === 'F4') {
        e.preventDefault();
        productInput.current?.focus();
      } else if ((e.key === 'Enter' && (e.ctrlKey || e.metaKey)) || e.key === 'F9') {
        e.preventDefault();
        if (canFinalize && !dialogOpen) void finalize(approval, true);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [finalize, approval, canFinalize, dialogOpen]);

  const quoteFor = (productId: number) => quote?.lines.filter((q) => q.productId === productId) ?? [];
  const clientChoices: ClientOption[] = clientText.trim().length >= 2 ? [...clientOptions, { create: clientText.trim() }] : [];
  const selectClient = (c: ClientRow) => {
    setClient(c);
    focusProduct();
  };

  return (
    <>
      <PageHeader
        title={correcting ? t.billing.correctionTitle(correcting.bill.billNo) : t.billing.title}
        subtitle={correcting ? t.billing.correctionIntro : undefined}
        actions={itemCount > 0 ? <Button color="error" onClick={() => setConfirmClear(true)}>{t.billing.clear}</Button> : undefined}
      />
      <Stack spacing={2}>
        {/* ---- Client ---- */}
        <Card variant="outlined">
          <CardContent>
            <Typography variant="overline" sx={{ fontWeight: 700, letterSpacing: 1 }} color="text.secondary">
              {t.billing.client}
            </Typography>
            {client ? (
              <Stack direction="row" spacing={2} sx={{ alignItems: 'center', flexWrap: 'wrap' }} useFlexGap>
                <Typography variant="h6" sx={{ fontWeight: 700 }}>
                  {client.fullName}
                </Typography>
                <Typography color="text.secondary">
                  {[client.clientCode, client.phone && `📞 ${client.phone}`, client.lastVisitAt && t.billing.lastVisit(formatDateTime(client.lastVisitAt))].filter(Boolean).join(' · ')}
                </Typography>
                <Box sx={{ flexGrow: 1 }} />
                <Button
                  onClick={() => {
                    setClient(null);
                    setClientText('');
                    window.setTimeout(() => clientInput.current?.focus(), 0);
                  }}
                >
                  {t.billing.changeClient}
                </Button>
              </Stack>
            ) : (
              <Stack spacing={1.5}>
                <Autocomplete<ClientOption>
                  options={clientChoices}
                  value={null}
                  onChange={(_, option) => {
                    if (!option) return;
                    if (isCreate(option)) setCreatingClient(option.create);
                    else selectClient(option);
                  }}
                  inputValue={clientText}
                  onInputChange={(_, value, why) => {
                    if (why !== 'reset') setClientText(value);
                  }}
                  getOptionLabel={(o) => (isCreate(o) ? t.billing.createClient(o.create) : o.fullName)}
                  isOptionEqualToValue={(a, b) => !isCreate(a) && !isCreate(b) && a.id === b.id}
                  filterOptions={(x) => x}
                  noOptionsText={t.billing.noClientMatch}
                  renderOption={({ key, ...props }, o) =>
                    isCreate(o) ? (
                      <li key={key} {...props}>
                        <Typography color="primary" sx={{ fontWeight: 600 }}>
                          {clientOptions.length === 0 && `${t.billing.noClientMatch} `}
                          {t.billing.createClient(o.create)}
                        </Typography>
                      </li>
                    ) : (
                      <li key={key} {...props}>
                        <Box>
                          <Typography sx={{ fontWeight: 600 }}>{o.fullName}</Typography>
                          <Typography variant="body2" color="text.secondary">
                            {[o.clientCode, o.phone, o.lastVisitAt && t.billing.lastVisit(formatDateTime(o.lastVisitAt))].filter(Boolean).join(' · ')}
                          </Typography>
                        </Box>
                      </li>
                    )
                  }
                  renderInput={(params) => <TextField {...params} placeholder={t.billing.clientPlaceholder} inputRef={clientInput} autoFocus={!correcting} />}
                />
                {recentClients.length > 0 && (
                  <Stack direction="row" spacing={1} sx={{ alignItems: 'center', flexWrap: 'wrap' }} useFlexGap>
                    <Typography variant="body2" color="text.secondary">
                      {t.billing.recent}
                    </Typography>
                    {recentClients.map((c) => (
                      <Chip key={c.id} label={c.fullName} variant="outlined" onClick={() => selectClient(c)} />
                    ))}
                    <Chip label={t.billing.walkIn} variant="outlined" sx={{ borderStyle: 'dashed' }} onClick={focusProduct} />
                  </Stack>
                )}
              </Stack>
            )}
          </CardContent>
        </Card>

        {/* ---- Consultation, procedures, medicines ---- */}
        <Card variant="outlined">
          <CardContent>
            <Section
              title={t.billing.consultation}
              actions={
                <ButtonGroup variant="outlined">
                  {standardConsultation && <Button onClick={() => addService(standardConsultation)}>{t.billing.addConsultation(rupees(standardConsultation.defaultPricePaise))}</Button>}
                  {consultations.length > 1 && (
                    <Button aria-label={t.billing.moreConsultations} onClick={(e) => setMenu({ kind: 'CONSULTATION', anchor: e.currentTarget })}>
                      ▾
                    </Button>
                  )}
                </ButtonGroup>
              }
            >
              <ServiceRows lines={serviceLines.filter((s) => s.service.kind === 'CONSULTATION')} onChange={updateService} onRemove={removeService} showQty={false} />
            </Section>
            <Section
              title={t.billing.procedures}
              actions={
                <Button variant="outlined" disabled={procedures.length === 0} onClick={(e) => setMenu({ kind: 'PROCEDURE', anchor: e.currentTarget })}>
                  {t.billing.addProcedure}
                </Button>
              }
            >
              <ServiceRows lines={serviceLines.filter((s) => s.service.kind === 'PROCEDURE')} onChange={updateService} onRemove={removeService} showQty />
            </Section>
            <Section title={t.billing.products}>
              <Stack direction="row" spacing={2} sx={{ alignItems: 'center', mb: 1 }}>
                <Autocomplete
                  sx={{ flexGrow: 1 }}
                  options={productOptions}
                  value={picked}
                  onChange={(_, value) => {
                    setPicked(value);
                    if (value) window.setTimeout(() => qtyInput.current?.select(), 0);
                  }}
                  inputValue={productText}
                  onInputChange={(_, value, why) => {
                    if (why !== 'reset') setProductText(value);
                  }}
                  getOptionLabel={(p) => p.name}
                  isOptionEqualToValue={(a, b) => a.productId === b.productId}
                  filterOptions={(x) => x}
                  noOptionsText={productText ? t.products.empty : t.billing.productSearch}
                  renderOption={({ key, ...props }, p) => (
                    <li key={key} {...props}>
                      <Box sx={{ width: '100%' }}>
                        <Typography sx={{ fontWeight: 600 }}>
                          {p.name}{' '}
                          <Typography component="span" color="text.secondary">
                            {p.genericName}
                          </Typography>
                        </Typography>
                        <Typography variant="body2" color={p.availableQty > 0 ? 'text.secondary' : 'error'}>
                          {p.availableQty > 0 ? t.billing.stock(p.availableQty, p.unit) : t.products.filterOut} · {rupees(p.pricePaise)}
                          {p.nextExpiry ? ` · ${t.products.expiry} ${formatExpiry(p.nextExpiry)}` : ''}
                          {p.expiresSoon ? ` · ⚠ ${t.billing.expiresSoon}` : ''}
                        </Typography>
                      </Box>
                    </li>
                  )}
                  renderInput={(params) => <TextField {...params} placeholder={t.billing.productSearch} inputRef={productInput} />}
                />
                <TextField
                  label={t.common.qty}
                  value={qtyText}
                  onChange={(e) => setQtyText(e.target.value.replace(/\D/g, '').slice(0, 5))}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey) tryAdd(picked, qtyText);
                  }}
                  inputRef={qtyInput}
                  sx={{ width: 90 }}
                  slotProps={{ htmlInput: { inputMode: 'numeric' } }}
                />
                <Button variant="contained" onClick={() => tryAdd(picked, qtyText)} disabled={!picked}>
                  {t.billing.add}
                </Button>
              </Stack>
              {!productText && recentProducts.length > 0 && (
                <Stack direction="row" spacing={1} sx={{ alignItems: 'center', flexWrap: 'wrap', mb: 1 }} useFlexGap>
                  <Typography variant="body2" color="text.secondary">
                    {t.billing.recent}
                  </Typography>
                  {recentProducts.map((p) => (
                    <Chip key={p.productId} label={p.name} variant="outlined" disabled={p.availableQty === 0} onClick={() => tryAdd(p, '1')} />
                  ))}
                </Stack>
              )}
              {lines.length === 0 ? (
                <Typography color="text.secondary">{t.billing.noServices}</Typography>
              ) : (
                <Stack divider={<Box sx={{ borderTop: 1, borderColor: 'divider' }} />}>
                  {lines.map((line) => {
                    const quoted = quoteFor(line.product.productId).filter((q) => q.qty > 0);
                    const net = quoted.reduce((s, q) => s + q.netPaise, 0);
                    return (
                      <Stack key={line.product.productId} direction="row" spacing={2} sx={{ alignItems: 'center', py: 1 }}>
                        <Box sx={{ flexGrow: 1 }}>
                          <Typography sx={{ fontWeight: 600 }}>{line.product.name}</Typography>
                          <Typography variant="body2" color="text.secondary">
                            {quoted.length ? `${quoted.map((q) => rupees(q.unitPricePaise)).join(' / ')} · ${quoted.flatMap((q) => q.batchNos).join(', ')}` : '—'}
                            {quoted.some((q) => q.expiresSoon) && ` · ⚠ ${t.billing.expiresSoon}`}
                          </Typography>
                          {line.notSuppliedQty > 0 && (
                            <Typography variant="body2" color="error">
                              {t.billing.notSupplied}: {line.notSuppliedQty}
                            </Typography>
                          )}
                        </Box>
                        <TextField
                          size="small"
                          label={t.common.qty}
                          value={line.qty}
                          onChange={(e) => setLineQty(line.product.productId, Number.parseInt(e.target.value.replace(/\D/g, '') || '0', 10))}
                          sx={{ width: 80 }}
                          slotProps={{ htmlInput: { inputMode: 'numeric', style: { textAlign: 'right' } } }}
                        />
                        <Typography sx={{ minWidth: 100, textAlign: 'right', fontVariantNumeric: 'tabular-nums' }}>{rupees(net)}</Typography>
                        <IconButton aria-label="remove" onClick={() => setLines((c) => c.filter((l) => l !== line))}>
                          ✕
                        </IconButton>
                      </Stack>
                    );
                  })}
                </Stack>
              )}
            </Section>
            <ErrorAlert error={quoteError} />
          </CardContent>
        </Card>

        {/* ---- Discount, payment and totals ---- */}
        <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: '1fr 400px' }, gap: 2 }}>
          <Card variant="outlined">
            <CardContent>
              <Stack spacing={2}>
                <Stack direction="row" spacing={2} sx={{ alignItems: 'center', flexWrap: 'wrap' }} useFlexGap>
                  <Typography sx={{ minWidth: 170 }}>{t.billing.discountOnMedicines}</Typography>
                  <ToggleButtonGroup size="small" exclusive value={discountKind} disabled={!hasProducts} onChange={(_, v: DiscountKind | null) => v && setDiscountKind(v)}>
                    <ToggleButton value="NONE">{t.billing.discountNone}</ToggleButton>
                    <ToggleButton value="PERCENT">{t.billing.discountPercent}</ToggleButton>
                    <ToggleButton value="AMOUNT">{t.billing.discountAmount}</ToggleButton>
                  </ToggleButtonGroup>
                  {discountKind !== 'NONE' && hasProducts && <TextField size="small" value={discountText} onChange={(e) => setDiscountText(e.target.value)} error={discount === null} sx={{ width: 120 }} />}
                  {!hasProducts && (
                    <Typography variant="body2" color="text.secondary">
                      {t.billing.discountNeedsMedicines}
                    </Typography>
                  )}
                </Stack>
                <Stack direction="row" spacing={2} sx={{ alignItems: 'center', flexWrap: 'wrap' }} useFlexGap>
                  <Typography sx={{ minWidth: 170 }}>{t.billing.payment}</Typography>
                  {!split && (
                    <ToggleButtonGroup
                      exclusive
                      value={payments[0]?.method ?? 'CASH'}
                      onChange={(_, v: PaymentMethod | null) => v && setPayments([{ method: v, amount: '', reference: payments[0]?.reference ?? '' }])}
                    >
                      {PAYMENT_METHODS.map((m) => (
                        <ToggleButton key={m} value={m} sx={{ px: 2.5 }}>
                          {t.billing.methods[m]}
                        </ToggleButton>
                      ))}
                    </ToggleButtonGroup>
                  )}
                  <Button
                    size="small"
                    onClick={() => {
                      setSplit(!split);
                      setPayments([
                        { method: 'CASH', amount: '', reference: '' },
                        { method: 'UPI', amount: '', reference: '' },
                      ]);
                    }}
                  >
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
                {cashPaise > 0 && (
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
                {(quote?.consultationPaise ?? 0) > 0 && <Row label={t.billing.consultation} value={rupees(quote?.consultationPaise ?? 0)} />}
                {(quote?.proceduresPaise ?? 0) > 0 && <Row label={t.billing.procedures} value={rupees(quote?.proceduresPaise ?? 0)} />}
                {(quote?.productsPaise ?? 0) > 0 && <Row label={t.billing.medicinesSubtotal} value={rupees(quote?.productsPaise ?? 0)} />}
                {(quote?.discountPaise ?? 0) > 0 && <Row label={t.billing.discountOnMedicines} value={`-${rupees(quote?.discountPaise ?? 0)}`} />}
                {(quote?.taxPaise ?? 0) > 0 && <Row label={t.billing.gstIncluded} value={rupees(quote?.taxPaise ?? 0)} muted />}
                {(quote?.roundOffPaise ?? 0) !== 0 && <Row label={t.billing.roundOff} value={rupees(quote?.roundOffPaise ?? 0)} />}
                <Row label={t.billing.total} value={rupees(total)} strong />
                {split && !paymentsOk && (
                  <Alert severity="warning">
                    {t.billing.paymentsMismatch} ({rupees(paidPaise)} / {rupees(total)})
                  </Alert>
                )}
                <ErrorAlert error={error} />
                <Button variant="contained" size="large" sx={{ py: 1.5, fontSize: '1.05rem' }} disabled={!canFinalize} onClick={() => void finalize(approval, true)}>
                  {busy ? t.billing.finalizing : t.billing.finalizePrint(MOD_KEY)}
                </Button>
                <Button variant="outlined" disabled={!canFinalize} onClick={() => void finalize(approval, false)}>
                  {t.billing.finalizeOnly}
                </Button>
              </Stack>
            </CardContent>
          </Card>
        </Box>
      </Stack>

      <Menu open={menu !== null} anchorEl={menu?.anchor ?? null} onClose={() => setMenu(null)}>
        {(menu?.kind === 'PROCEDURE' ? procedures : consultations).map((s) => (
          <MenuItem key={s.id} onClick={() => addService(s)} sx={{ minWidth: 260, justifyContent: 'space-between', gap: 3 }}>
            <span>{s.name}</span>
            <Typography color="text.secondary">{rupees(s.defaultPricePaise)}</Typography>
          </MenuItem>
        ))}
      </Menu>

      {partial && (
        <Dialog open onClose={() => setPartial(null)}>
          <DialogTitle>{t.billing.partialTitle}</DialogTitle>
          <DialogContent>
            <DialogContentText>{t.billing.partialText(partial.product.availableQty, partial.requested, partial.product.unit)}</DialogContentText>
          </DialogContent>
          <DialogActions>
            <Button
              onClick={() => {
                setPartial(null);
                qtyInput.current?.select();
              }}
            >
              {t.billing.changeQty}
            </Button>
            {partial.product.availableQty > 0 ? (
              <Button
                variant="contained"
                onClick={() => {
                  addLine(partial.product, partial.product.availableQty, partial.requested - partial.product.availableQty);
                  setPartial(null);
                }}
              >
                {t.billing.supplyAvailable(partial.product.availableQty)}
              </Button>
            ) : (
              <Button
                variant="contained"
                onClick={() => {
                  addLine(partial.product, 0, partial.requested);
                  setPartial(null);
                }}
              >
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
            void finalize(credentials, true);
          }}
        />
      )}
      <ConfirmDialog open={confirmClear} title={t.billing.clear} text={t.billing.clearConfirm} confirmLabel={t.billing.clear} danger onConfirm={() => reset()} onClose={() => setConfirmClear(false)} />
      {creatingClient !== null && (
        <ClientDialog
          client={null}
          initialName={creatingClient}
          onClose={() => setCreatingClient(null)}
          onSaved={(c) => {
            // New or existing: either way it is now this bill's client, no extra steps (brief §5).
            setCreatingClient(null);
            setClientText('');
            notify(t.clients.created(c.fullName));
            selectClient(c);
          }}
        />
      )}
      {done && (
        <BillDetailDialog
          billId={done.detail.bill.id}
          savedMessage={t.billing.createdTitle(done.detail.bill.billNo)}
          autoPrint={done.print}
          onClose={() => {
            setDone(null);
            if (correcting) navigate({ name: 'bills' });
            else window.setTimeout(() => clientInput.current?.focus(), 0);
          }}
          onChanged={() => undefined}
        />
      )}
    </>
  );
}

function Section({ title, actions, children }: { title: string; actions?: ReactNode; children: ReactNode }) {
  return (
    <Box sx={{ '& + &': { mt: 2.5, pt: 2.5, borderTop: 1, borderColor: 'divider' } }}>
      <Stack direction="row" sx={{ alignItems: 'center', justifyContent: 'space-between', mb: 1 }}>
        <Typography variant="overline" sx={{ fontWeight: 700, letterSpacing: 1 }} color="text.secondary">
          {title}
        </Typography>
        {actions}
      </Stack>
      {children}
    </Box>
  );
}

function ServiceRows({
  lines,
  onChange,
  onRemove,
  showQty,
}: {
  lines: ServiceLine[];
  onChange: (key: number, patch: Partial<ServiceLine>) => void;
  onRemove: (key: number) => void;
  showQty: boolean;
}) {
  if (lines.length === 0) return <Typography color="text.secondary">{t.billing.noServices}</Typography>;
  return (
    <Stack spacing={1}>
      {lines.map((line) => {
        const paise = parseRupees(line.price || '0');
        const below = paise !== null && paise < line.service.defaultPricePaise;
        return (
          <Stack key={line.key} direction="row" spacing={2} sx={{ alignItems: 'center' }}>
            <Box sx={{ flexGrow: 1 }}>
              <Typography sx={{ fontWeight: 600 }}>{line.service.name}</Typography>
              {below && (
                <Typography variant="body2" color="warning.main">
                  {t.billing.belowDefault(rupees(line.service.defaultPricePaise))}
                </Typography>
              )}
            </Box>
            {showQty && (
              <TextField
                size="small"
                label={t.common.qty}
                value={line.qty}
                onChange={(e) => onChange(line.key, { qty: Math.min(100, Number.parseInt(e.target.value.replace(/\D/g, '') || '0', 10)) })}
                sx={{ width: 80 }}
                slotProps={{ htmlInput: { inputMode: 'numeric', style: { textAlign: 'right' } } }}
              />
            )}
            <TextField size="small" label="₹" value={line.price} error={paise === null} onChange={(e) => onChange(line.key, { price: e.target.value })} sx={{ width: 110 }} />
            <Typography sx={{ minWidth: 100, textAlign: 'right', fontVariantNumeric: 'tabular-nums' }}>{paise === null ? '—' : rupees(paise * line.qty)}</Typography>
            <IconButton aria-label="remove" onClick={() => onRemove(line.key)}>
              ✕
            </IconButton>
          </Stack>
        );
      })}
    </Stack>
  );
}

function Row({ label, value, strong = false, muted = false }: { label: string; value: string; strong?: boolean; muted?: boolean }) {
  const sx = { fontWeight: strong ? 700 : 400, fontSize: strong ? '1.35rem' : '1rem', color: muted ? 'text.secondary' : 'text.primary' };
  return (
    <Stack direction="row" sx={{ justifyContent: 'space-between', ...(strong ? { borderTop: 1, borderColor: 'divider', pt: 1, mt: 0.5 } : {}) }}>
      <Typography sx={sx}>{label}</Typography>
      <Typography sx={{ ...sx, fontVariantNumeric: 'tabular-nums' }}>{value}</Typography>
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
