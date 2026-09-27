import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import {
  Alert,
  Autocomplete,
  Box,
  Button,
  Card,
  CardContent,
  Checkbox,
  Chip,
  Dialog,
  DialogActions,
  DialogContent,
  DialogContentText,
  DialogTitle,
  FormControlLabel,
  IconButton,
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
  type DuplicateMatch,
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
import { clearBillDraft, loadBillDraft, saveBillDraft } from '../../lib/billDraft';
import { formatDateTime, formatExpiry, newBillKey } from '../../lib/dates';
import { paiseToInput, parseRupees, rupees } from '../../lib/money';
import { BillDetailDialog } from './BillDetailDialog';

interface Line {
  product: SaleProduct;
  qty: number;
  notSuppliedQty: number;
}

/** A consultation or procedure: from the list (`serviceId`), or a name typed on the bill. */
interface ServiceLine {
  key: number;
  serviceId: number | null;
  kind: ServiceKind;
  name: string;
  /** The usual price when it comes from the list. */
  defaultPricePaise: number | null;
  qty: number;
  /** Amount as typed, in rupees. */
  price: string;
}

type DiscountKind = Discount['kind'];
interface PaymentDraft {
  method: PaymentMethod;
  amount: string;
  reference: string;
}

let serviceKeys = 0;

/** What an unfinished bill keeps on this computer (not a correction: that starts from the bill). */
interface DraftState {
  billKey: string;
  client: ClientRow | null;
  clientText: string;
  newPhone: string;
  allowDuplicate: boolean;
  serviceLines: ServiceLine[];
  lines: Line[];
  useStandard: boolean;
  discountKind: DiscountKind;
  discountText: string;
  split: boolean;
  payments: PaymentDraft[];
  receivedText: string;
  note: string;
}

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
function servicesFrom(detail: BillDetail): ServiceLine[] {
  return detail.services.map((s) => ({
    key: ++serviceKeys,
    serviceId: s.serviceId,
    kind: s.kind,
    name: s.name,
    defaultPricePaise: s.defaultPricePaise,
    qty: s.qty,
    price: paiseToInput(s.unitPricePaise),
  }));
}

const fromCatalog = (service: ServiceRow): ServiceLine => ({
  key: ++serviceKeys,
  serviceId: service.id,
  kind: service.kind,
  name: service.name,
  defaultPricePaise: service.defaultPricePaise,
  qty: 1,
  price: paiseToInput(service.defaultPricePaise),
});

/**
 * The receptionist's main screen: client, consultation, procedures and medicines on ONE screen,
 * then payment and Finalize. A new client, and new consultation/procedure names, are typed
 * right here and saved with the bill (DEC-034). All amounts are worked out in Rust.
 */
export function NewBillPage({ initialClientId, correcting }: { initialClientId?: number | undefined; correcting?: BillDetail | undefined }) {
  const { navigate, status, session } = useApp();
  // An unfinished bill from before (crash, restart, idle lock): only for a plain new bill.
  const [restored] = useState(() => (correcting || initialClientId ? null : loadBillDraft<DraftState>(session.userId)));
  const draft = restored?.state;
  if (draft) serviceKeys = Math.max(serviceKeys, ...draft.serviceLines.map((s) => s.key));
  const [draftNotice, setDraftNotice] = useState(restored !== null);
  const [billKey, setBillKey] = useState(() => draft?.billKey ?? newBillKey());
  // Client: an existing one, or a new one typed here (name + mobile), saved at Finalize.
  const [client, setClient] = useState<ClientRow | null>(draft?.client ?? null);
  const [clientText, setClientText] = useState(draft?.clientText ?? '');
  const [newPhone, setNewPhone] = useState(draft?.newPhone ?? '');
  const [allowDuplicate, setAllowDuplicate] = useState(draft?.allowDuplicate ?? false);
  const [matches, setMatches] = useState<DuplicateMatch[]>([]);
  const [clientOptions, setClientOptions] = useState<ClientRow[]>([]);
  const [recentClients, setRecentClients] = useState<ClientRow[]>([]);
  // Consultations and procedures
  const [catalog, setCatalog] = useState<ServiceRow[]>([]);
  const [serviceLines, setServiceLines] = useState<ServiceLine[]>(() => (correcting ? servicesFrom(correcting) : (draft?.serviceLines ?? [])));
  // Medicines and products
  const [lines, setLines] = useState<Line[]>(() => (correcting ? linesFrom(correcting) : (draft?.lines ?? [])));
  const [recentProducts, setRecentProducts] = useState<SaleProduct[]>([]);
  const [productText, setProductText] = useState('');
  const [productOptions, setProductOptions] = useState<SaleProduct[]>([]);
  const [picked, setPicked] = useState<SaleProduct | null>(null);
  const [qtyText, setQtyText] = useState('1');
  const [partial, setPartial] = useState<{ product: SaleProduct; requested: number } | null>(null);
  // Discount: the clinic's standard medicine discount is ticked by default (DEC-034).
  const [standardPercent, setStandardPercent] = useState(0);
  const [useStandard, setUseStandard] = useState(draft?.useStandard ?? true);
  const [discountKind, setDiscountKind] = useState<DiscountKind>(draft?.discountKind ?? 'NONE');
  const [discountText, setDiscountText] = useState(draft?.discountText ?? '');
  // Payment
  const [split, setSplit] = useState(draft?.split ?? false);
  const [payments, setPayments] = useState<PaymentDraft[]>(draft?.payments ?? [{ method: 'CASH', amount: '', reference: '' }]);
  const [receivedText, setReceivedText] = useState(draft?.receivedText ?? '');
  const [note, setNote] = useState(draft?.note ?? '');
  const [reason, setReason] = useState('');
  // The quote and the exact bill it was worked out for: Finalize waits until they match.
  const [quoted, setQuoted] = useState<{ quote: Quote; key: string } | null>(null);
  const [quoteError, setQuoteError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [approval, setApproval] = useState<{ username: string; password: string } | null>(null);
  const [needApproval, setNeedApproval] = useState<{ print: boolean } | null>(null);
  const [approvalMessage, setApprovalMessage] = useState('');
  const [confirmClear, setConfirmClear] = useState(false);
  const [done, setDone] = useState<{ detail: BillDetail; print: boolean } | null>(null);
  const clientInput = useRef<HTMLInputElement | null>(null);
  const phoneInput = useRef<HTMLInputElement | null>(null);
  const productInput = useRef<HTMLInputElement | null>(null);
  const qtyInput = useRef<HTMLInputElement | null>(null);
  // Set at once on Finalize (state updates later): a held Ctrl+Enter or F9 sends one request.
  const submitting = useRef(false);

  // Catalog, clinic settings, recent clients and recent products: loaded once.
  const loadShortcuts = useCallback(() => {
    api.searchClients('', false).then((rows) => setRecentClients(rows.slice(0, 8))).catch(() => undefined);
    api.recentProductsForSale().then(setRecentProducts).catch(() => undefined);
    api.listServices(false).then(setCatalog).catch(() => undefined);
  }, []);
  useEffect(() => {
    loadShortcuts();
    api
      .getClinicSettings()
      .then((s) => setStandardPercent(s.defaultMedicineDiscountPercent))
      .catch(() => undefined);
  }, [loadShortcuts]);

  // Preselected client (from the client screen or the bill being corrected).
  useEffect(() => {
    const id = correcting?.bill.clientId ?? initialClientId;
    if (id) api.getClientProfile(id, null, null).then((p) => setClient(p.client)).catch(() => undefined);
  }, [initialClientId, correcting]);

  // Client and product search: from the 2nd character, debounced, done in the database.
  useEffect(() => {
    if (client || clientText.trim().length < 2) {
      setClientOptions([]);
      return;
    }
    let current = true;
    const timer = window.setTimeout(
      () =>
        api
          .searchClients(clientText, false)
          .then((rows) => current && setClientOptions(rows.slice(0, 8)))
          .catch(() => undefined),
      200,
    );
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [clientText, client]);
  // A new client being typed: is this person already a client (same phone or same name)?
  useEffect(() => {
    if (client || clientText.trim().length < 2) {
      setMatches([]);
      return;
    }
    let current = true;
    const timer = window.setTimeout(
      () =>
        api
          .checkClientDuplicates({ fullName: clientText, phone: newPhone, dateOfBirth: null, excludeId: null })
          .then((found) => current && setMatches(found.filter((m) => m.strong)))
          .catch(() => undefined),
      400,
    );
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [clientText, newPhone, client]);
  useEffect(() => {
    if (!productText.trim()) {
      setProductOptions([]);
      return;
    }
    let current = true;
    const timer = window.setTimeout(
      () =>
        api
          .searchProductsForSale(productText)
          .then((rows) => current && setProductOptions(rows))
          .catch(() => undefined),
      150,
    );
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [productText]);

  const lineInputs: BillLineInput[] = useMemo(
    () => lines.filter((l) => l.qty + l.notSuppliedQty > 0).map((l) => ({ productId: l.product.productId, qty: l.qty, notSuppliedQty: l.notSuppliedQty })),
    [lines],
  );
  const hasProducts = lineInputs.some((l) => l.qty > 0);
  const standardOn = useStandard && standardPercent > 0;
  // No medicines, no discount (it applies to medicines only): never send a hidden one.
  const discount: Discount | null = !hasProducts
    ? { kind: 'NONE' }
    : standardOn
      ? { kind: 'PERCENT', value: standardPercent * 100 }
      : discountOf(discountKind, discountText);
  const servicesValid = serviceLines.every((s) => s.qty > 0 && s.price.trim() !== '' && parseRupees(s.price) !== null);
  const serviceInputs: ServiceLineInput[] = useMemo(
    () =>
      serviceLines.map((s) => {
        const paise = parseRupees(s.price || '0');
        // Always the amount on screen, so a list price changed meanwhile never alters this bill.
        return s.serviceId !== null
          ? { serviceId: s.serviceId, kind: null, name: null, qty: s.qty, unitPricePaise: paise }
          : { serviceId: null, kind: s.kind, name: s.name, qty: s.qty, unitPricePaise: paise };
      }),
    [serviceLines],
  );
  const itemCount = lineInputs.length + serviceInputs.length;
  const quoteKey = JSON.stringify([lineInputs, serviceInputs, discount, correcting?.bill.id ?? null]);
  const quote = quoted && quoted.key === quoteKey ? quoted.quote : null;

  // Keep the unfinished bill on this computer (debounced). The same billKey is kept, so a bill
  // that was in fact saved just before a crash is not saved twice. Removing every line drops it,
  // but a bill opened from a client page leaves an older draft alone until something is added.
  const ownsDraft = useRef(restored !== null);
  useEffect(() => {
    if (correcting) return;
    const timer = window.setTimeout(() => {
      if (lines.length + serviceLines.length > 0) {
        ownsDraft.current = true;
        const state: DraftState = { billKey, client, clientText, newPhone, allowDuplicate, serviceLines, lines, useStandard, discountKind, discountText, split, payments, receivedText, note };
        saveBillDraft(session.userId, state);
      } else if (ownsDraft.current) {
        clearBillDraft(session.userId);
      }
    }, 500);
    return () => window.clearTimeout(timer);
  }, [correcting, session.userId, billKey, client, clientText, newPhone, allowDuplicate, serviceLines, lines, useStandard, discountKind, discountText, split, payments, receivedText, note]);

  // All money arithmetic happens in Rust: the screen asks for a quote whenever the bill changes.
  useEffect(() => {
    if (itemCount === 0 || discount === null || !servicesValid) {
      setQuoted(null);
      setQuoteError(null);
      return;
    }
    const key = quoteKey;
    let current = true;
    const timer = window.setTimeout(() => {
      api
        .quoteBill(lineInputs, serviceInputs, discount, correcting?.bill.id ?? null)
        .then((q) => {
          if (current) {
            setQuoted({ quote: q, key });
            setQuoteError(null);
          }
        })
        .catch((e: unknown) => {
          if (current) {
            setQuoted(null);
            setQuoteError(e);
          }
        });
    }, 120);
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [quoteKey]);

  const total = quote?.totalPaise ?? 0;
  const paymentDrafts: PaymentDraft[] = split ? payments : [{ method: payments[0]?.method ?? 'CASH', reference: payments[0]?.reference ?? '', amount: paiseToInput(total) }];
  const parsedPayments = paymentDrafts.map((p) => ({ ...p, paise: parseRupees(p.amount || '0') }));
  const paidPaise = parsedPayments.reduce((sum, p) => sum + (p.paise ?? 0), 0);
  const cashPaise = parsedPayments.filter((p) => p.method === 'CASH').reduce((sum, p) => sum + (p.paise ?? 0), 0);
  const received = cashPaise > 0 && receivedText.trim() ? parseRupees(receivedText) : null;
  const changePaise = received !== null ? received - cashPaise : null;

  const focusProduct = () => window.setTimeout(() => productInput.current?.focus(), 0);

  // ---- Client ----
  const typingNewClient = !client && clientText.trim().length >= 2;
  // A number typed in the name box (searching by phone): not a name for a new client.
  const nameIsNumber = typingNewClient && /^[+\d\s()-]+$/.test(clientText.trim());
  const duplicateBlocks = typingNewClient && matches.length > 0 && !allowDuplicate;
  const selectClient = (c: ClientRow) => {
    setClient(c);
    setClientText('');
    setNewPhone('');
    setMatches([]);
    setAllowDuplicate(false);
    focusProduct();
  };

  // ---- Consultations and procedures ----
  const consultations = catalog.filter((s) => s.kind === 'CONSULTATION');
  const procedures = catalog.filter((s) => s.kind === 'PROCEDURE');
  const standardConsultation = consultations[0];
  const addFromCatalog = (service: ServiceRow) => setServiceLines((current) => [...current, fromCatalog(service)]);
  /** A name typed on the bill: the list entry if it exists (any case), otherwise a new one. */
  const addTyped = (kind: ServiceKind, name: string) => {
    const clean = name.trim();
    if (!clean) return;
    const known = catalog.find((s) => s.kind === kind && s.name.toLowerCase() === clean.toLowerCase());
    if (known) addFromCatalog(known);
    else setServiceLines((current) => [...current, { key: ++serviceKeys, serviceId: null, kind, name: clean, defaultPricePaise: null, qty: 1, price: '' }]);
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
    if (!correcting) clearBillDraft(session.userId);
    setDraftNotice(false);
    setBillKey(newBillKey());
    setLines([]);
    setServiceLines([]);
    setClient(null);
    setClientText('');
    setNewPhone('');
    setMatches([]);
    setAllowDuplicate(false);
    setUseStandard(true);
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
    !busy &&
    itemCount > 0 &&
    servicesValid &&
    quote !== null &&
    paymentsOk &&
    !duplicateBlocks &&
    !nameIsNumber &&
    !(!client && clientText.trim().length === 1) &&
    (!correcting || reason.trim().length >= 3) &&
    (changePaise === null || changePaise >= 0);
  const dialogOpen = partial !== null || needApproval !== null || confirmClear || done !== null;

  const finalize = useCallback(
    async (withApproval: { username: string; password: string } | null, print: boolean) => {
      if (submitting.current || busy || itemCount === 0 || !quote || discount === null) return;
      submitting.current = true;
      setBusy(true); // disables Finalize at once: a double-click cannot submit twice (D10)
      setError(null);
      const input: BillInput = {
        idempotencyKey: billKey,
        clientId: client?.id ?? null,
        // Typed on the bill and not picked from the list: saved together with this bill.
        newClient: typingNewClient && !nameIsNumber ? { fullName: clientText.trim(), phone: newPhone.trim(), allowDuplicate } : null,
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
        setNeedApproval(null);
        loadShortcuts();
      } catch (e) {
        if (isCommandError(e) && (e.code === 'DISCOUNT_APPROVAL_REQUIRED' || e.field === 'approval')) {
          setApproval(null);
          setApprovalMessage(e.message);
          setNeedApproval({ print });
        } else {
          setError(e);
          // Someone with this name or phone exists: show the "Use / different person" choice now.
          if (isCommandError(e) && e.code === 'POSSIBLE_DUPLICATE') {
            api
              .checkClientDuplicates({ fullName: clientText, phone: newPhone, dateOfBirth: null, excludeId: null })
              .then((found) => setMatches(found.filter((m) => m.strong)))
              .catch(() => undefined);
          }
        }
      } finally {
        submitting.current = false;
        setBusy(false);
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [busy, itemCount, quote, discount, billKey, client, clientText, newPhone, allowDuplicate, lineInputs, serviceInputs, total, parsedPayments, received, note, correcting, reason],
  );

  // F2: client search, F4: product search, Ctrl/Cmd+Enter (or F9): finalize & print.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (status.locked) return;
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
  }, [finalize, approval, canFinalize, dialogOpen, status.locked]);

  const quoteFor = (productId: number) => quote?.lines.filter((q) => q.productId === productId) ?? [];

  return (
    <>
      <PageHeader
        title={correcting ? t.billing.correctionTitle(correcting.bill.billNo) : t.billing.title}
        subtitle={correcting ? t.billing.correctionIntro : undefined}
        actions={itemCount > 0 ? <Button color="error" onClick={() => setConfirmClear(true)}>{t.billing.clear}</Button> : undefined}
      />
      <Stack spacing={2}>
        {draftNotice && restored && (
          <Alert
            severity="info"
            onClose={() => setDraftNotice(false)}
            action={
              <Button color="inherit" size="small" onClick={() => reset()}>
                {t.billing.discardDraft}
              </Button>
            }
          >
            {t.billing.draftRestored(formatDateTime(restored.savedAt))}
          </Alert>
        )}
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
                    window.setTimeout(() => clientInput.current?.focus(), 0);
                  }}
                >
                  {t.billing.changeClient}
                </Button>
              </Stack>
            ) : (
              <Stack spacing={1.5}>
                <Stack direction={{ xs: 'column', sm: 'row' }} spacing={2}>
                  <Autocomplete<ClientRow, false, false, true>
                    freeSolo
                    sx={{ flexGrow: 1 }}
                    options={clientOptions}
                    value={null}
                    onChange={(_, option) => {
                      if (option && typeof option !== 'string') selectClient(option);
                      else if (typingNewClient) window.setTimeout(() => phoneInput.current?.focus(), 0);
                    }}
                    inputValue={clientText}
                    onInputChange={(_, value, why) => {
                      if (why !== 'reset') setClientText(value);
                    }}
                    getOptionLabel={(o) => (typeof o === 'string' ? o : o.fullName)}
                    filterOptions={(x) => x}
                    renderOption={({ key, ...props }, o) => (
                      <li key={key} {...props}>
                        <Box>
                          <Typography sx={{ fontWeight: 600 }}>{o.fullName}</Typography>
                          <Typography variant="body2" color="text.secondary">
                            {[o.clientCode, o.phone, o.lastVisitAt && t.billing.lastVisit(formatDateTime(o.lastVisitAt))].filter(Boolean).join(' · ')}
                          </Typography>
                        </Box>
                      </li>
                    )}
                    renderInput={(params) => <TextField {...params} label={t.clients.name} placeholder={t.billing.clientPlaceholder} inputRef={clientInput} autoFocus={!correcting} />}
                  />
                  {typingNewClient && (
                    <TextField
                      label={t.billing.newClientPhone}
                      value={newPhone}
                      onChange={(e) => {
                        setNewPhone(e.target.value);
                        setAllowDuplicate(false);
                      }}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey) focusProduct();
                      }}
                      inputRef={phoneInput}
                      sx={{ width: { xs: '100%', sm: 220 } }}
                      slotProps={{ htmlInput: { inputMode: 'tel' } }}
                    />
                  )}
                </Stack>
                {nameIsNumber && (
                  <Alert
                    severity="warning"
                    action={
                      <Button
                        color="inherit"
                        onClick={() => {
                          setNewPhone(clientText.trim());
                          setClientText('');
                          window.setTimeout(() => clientInput.current?.focus(), 0);
                        }}
                      >
                        {t.billing.useAsMobile}
                      </Button>
                    }
                  >
                    {t.billing.nameIsNumber}
                  </Alert>
                )}
                {typingNewClient && !nameIsNumber && matches.length === 0 && <Typography color="text.secondary">{t.billing.newClientNote(clientText.trim())}</Typography>}
                {typingNewClient && matches.length > 0 && (
                  <Alert severity={allowDuplicate ? 'info' : 'warning'}>
                    <Stack spacing={1}>
                      <strong>{t.billing.alreadyClient}</strong>
                      {matches.map((m) => (
                        <Stack key={m.client.id} direction="row" spacing={2} sx={{ alignItems: 'center' }}>
                          <Typography sx={{ flexGrow: 1 }}>
                            {m.client.fullName} · {m.client.clientCode}
                            {m.client.phone && ` · ${m.client.phone}`} · {t.clients.reasons[m.reason]}
                          </Typography>
                          <Button size="small" variant="contained" onClick={() => selectClient(m.client)}>
                            {t.billing.useExisting}
                          </Button>
                        </Stack>
                      ))}
                      <FormControlLabel control={<Checkbox checked={allowDuplicate} onChange={(e) => setAllowDuplicate(e.target.checked)} />} label={t.billing.differentPerson} />
                    </Stack>
                  </Alert>
                )}
                {!clientText && recentClients.length > 0 && (
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
              actions={standardConsultation && <Button variant="outlined" onClick={() => addFromCatalog(standardConsultation)}>{t.billing.addConsultation(rupees(standardConsultation.defaultPricePaise))}</Button>}
            >
              <ServiceTyper kind="CONSULTATION" options={consultations} placeholder={t.billing.typeConsultation} onPick={addFromCatalog} onTyped={addTyped} />
              <ServiceRows lines={serviceLines.filter((s) => s.kind === 'CONSULTATION')} onChange={updateService} onRemove={removeService} showQty={false} />
            </Section>
            <Section title={t.billing.procedures}>
              <ServiceTyper kind="PROCEDURE" options={procedures} placeholder={t.billing.typeProcedure} onPick={addFromCatalog} onTyped={addTyped} />
              <ServiceRows lines={serviceLines.filter((s) => s.kind === 'PROCEDURE')} onChange={updateService} onRemove={removeService} showQty />
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
                          {p.availableQty > 0 ? t.billing.stock(p.availableQty, p.unit) : t.products.filterOut} · {t.products.mrp} {rupees(p.pricePaise)}
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
                    const qty = quoted.reduce((s, q) => s + q.qty, 0);
                    const off = quoted.reduce((s, q) => s + q.discountSharePaise, 0);
                    // Each line: quantity × MRP, its share of the discount, and the price per unit it sells at.
                    const detail = quoted.length
                      ? [
                          quoted.map((q) => t.billing.lineDetail(q.qty, rupees(q.unitPricePaise))).join(' + '),
                          off > 0 ? t.billing.lineDiscount(rupees(off)) : null,
                          qty > 0 ? t.billing.sellingPrice(rupees(Math.round(net / qty))) : null,
                        ]
                          .filter(Boolean)
                          .join(' · ')
                      : '—';
                    return (
                      <Stack key={line.product.productId} direction="row" spacing={2} sx={{ alignItems: 'center', py: 1 }}>
                        <Box sx={{ flexGrow: 1 }}>
                          <Typography sx={{ fontWeight: 600 }}>{line.product.name}</Typography>
                          <Typography variant="body2" color="text.secondary">
                            {detail}
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
                        <IconButton aria-label={t.billing.removeLine(line.product.name)} onClick={() => setLines((c) => c.filter((l) => l !== line))}>
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
                  {standardPercent > 0 && (
                    <FormControlLabel
                      control={<Checkbox checked={useStandard} onChange={(e) => setUseStandard(e.target.checked)} />}
                      label={t.billing.standardDiscount(standardPercent)}
                      disabled={!hasProducts}
                    />
                  )}
                  {!standardOn && (
                    <>
                      <Typography>{standardPercent > 0 ? t.billing.otherDiscount : t.billing.discountOnMedicines}</Typography>
                      <ToggleButtonGroup size="small" exclusive value={discountKind} disabled={!hasProducts} onChange={(_, v: DiscountKind | null) => v && setDiscountKind(v)}>
                        <ToggleButton value="NONE">{t.billing.discountNone}</ToggleButton>
                        <ToggleButton value="PERCENT">{t.billing.discountPercent}</ToggleButton>
                        <ToggleButton value="AMOUNT">{t.billing.discountAmount}</ToggleButton>
                      </ToggleButtonGroup>
                      {discountKind !== 'NONE' && hasProducts && <TextField size="small" value={discountText} onChange={(e) => setDiscountText(e.target.value)} error={discount === null} sx={{ width: 120 }} />}
                    </>
                  )}
                  {!hasProducts && (
                    <Typography variant="body2" color="text.secondary">
                      {t.billing.discountNeedsMedicines}
                    </Typography>
                  )}
                </Stack>
                <Stack direction="row" spacing={2} sx={{ alignItems: 'center', flexWrap: 'wrap' }} useFlexGap>
                  <Typography sx={{ minWidth: 100 }}>{t.billing.payment}</Typography>
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
                {duplicateBlocks && <Alert severity="warning">{t.billing.duplicateBlocks}</Alert>}
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
          onCancel={() => setNeedApproval(null)}
          onApprove={(credentials) => {
            const print = needApproval.print;
            setApproval(credentials);
            setNeedApproval(null);
            void finalize(credentials, print);
          }}
        />
      )}
      <ConfirmDialog open={confirmClear} title={t.billing.clear} text={t.billing.clearConfirm} confirmLabel={t.billing.clear} danger onConfirm={() => reset()} onClose={() => setConfirmClear(false)} />
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

/**
 * Type a consultation or procedure like a client name: pick one from the list (with its usual
 * price), or type a new name and press Enter / Add; its amount is then typed on the line.
 */
function ServiceTyper({
  kind,
  options,
  placeholder,
  onPick,
  onTyped,
}: {
  kind: ServiceKind;
  options: ServiceRow[];
  placeholder: string;
  onPick: (s: ServiceRow) => void;
  onTyped: (kind: ServiceKind, name: string) => void;
}) {
  const [text, setText] = useState('');
  const add = () => {
    onTyped(kind, text);
    setText('');
  };
  return (
    <Stack direction="row" spacing={2} sx={{ alignItems: 'center', mb: 1 }}>
      <Autocomplete<ServiceRow, false, false, true>
        freeSolo
        sx={{ flexGrow: 1 }}
        options={options}
        value={null}
        inputValue={text}
        onInputChange={(_, value, why) => {
          if (why !== 'reset') setText(value);
        }}
        onChange={(_, option) => {
          if (option && typeof option !== 'string') {
            onPick(option);
            setText('');
          } else if (typeof option === 'string') add();
        }}
        getOptionLabel={(o) => (typeof o === 'string' ? o : o.name)}
        renderOption={({ key, ...props }, o) => (
          <li key={key} {...props}>
            <Stack direction="row" sx={{ justifyContent: 'space-between', width: '100%', gap: 2 }}>
              <span>{o.name}</span>
              <Typography color="text.secondary">{rupees(o.defaultPricePaise)}</Typography>
            </Stack>
          </li>
        )}
        renderInput={(params) => <TextField {...params} size="small" placeholder={placeholder} />}
      />
      <Button variant="outlined" disabled={!text.trim()} onClick={add}>
        {text.trim() ? t.billing.addTyped(text.trim()) : t.billing.add}
      </Button>
    </Stack>
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
  if (lines.length === 0) return null;
  return (
    <Stack spacing={1}>
      {lines.map((line) => {
        const paise = line.price.trim() ? parseRupees(line.price) : null;
        const missing = !line.price.trim();
        return (
          <Stack key={line.key} direction="row" spacing={2} sx={{ alignItems: 'center' }}>
            <Typography sx={{ fontWeight: 600, flexGrow: 1 }}>{line.name}</Typography>
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
            <TextField
              size="small"
              label="₹"
              value={line.price}
              autoFocus={missing}
              error={!missing && paise === null}
              helperText={missing ? t.billing.amountNeeded : undefined}
              onChange={(e) => onChange(line.key, { price: e.target.value })}
              sx={{ width: 130 }}
            />
            <Typography sx={{ minWidth: 100, textAlign: 'right', fontVariantNumeric: 'tabular-nums' }}>{paise === null ? '—' : rupees(paise * line.qty)}</Typography>
            <IconButton aria-label={t.billing.removeLine(line.name)} onClick={() => onRemove(line.key)}>
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
    <Dialog
      open
      onClose={onCancel}
      maxWidth="xs"
      fullWidth
      slotProps={{
        paper: {
          component: 'form',
          onSubmit: (e: React.FormEvent) => {
            e.preventDefault();
            if (username && password) onApprove({ username, password });
          },
        },
      }}
    >
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
        <Button type="submit" variant="contained" disabled={!username || !password}>
          {t.billing.approve}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
