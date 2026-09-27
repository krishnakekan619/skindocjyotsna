// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { BillLineInput, Discount, Quote, SaleProduct, ServiceLineInput } from '../../api';
import { AppContext, type AppContextValue } from '../../app/AppContext';
import { t } from '../../i18n/en';
import { loadBillDraft, saveBillDraft } from '../../lib/billDraft';
import { NewBillPage } from './NewBillPage';

// The screen only talks to the API; every call is faked here (money itself is worked out in Rust).
const mocks = vi.hoisted(() => ({
  quoteBill: vi.fn(),
  finalizeBill: vi.fn(),
}));
vi.mock('../../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api')>();
  return {
    ...actual,
    api: {
      searchClients: vi.fn(() => Promise.resolve([])),
      recentProductsForSale: vi.fn(() => Promise.resolve([])),
      listServices: vi.fn(() => Promise.resolve([])),
      getClinicSettings: vi.fn(() => Promise.resolve({ defaultMedicineDiscountPercent: 10 })),
      getClientProfile: vi.fn(() => Promise.reject(new Error('not used'))),
      checkClientDuplicates: vi.fn(() => Promise.resolve([])),
      searchProductsForSale: vi.fn(() => Promise.resolve([])),
      quoteBill: mocks.quoteBill,
      finalizeBill: mocks.finalizeBill,
    },
  };
});

const USER_ID = 7;
const product: SaleProduct = {
  productId: 1,
  name: 'Sunscreen SPF 50',
  genericName: '',
  sku: 'P-1',
  unit: 'pcs',
  gstRateBp: 0,
  pricePaise: 50000,
  availableQty: 10,
  nextExpiry: null,
  expiresSoon: false,
};

/** A quote shaped like the Rust one, enough for the screen to show totals and enable Save. */
function fakeQuote(lines: BillLineInput[], services: ServiceLineInput[], discount: Discount): Quote {
  const products = lines.reduce((s, l) => s + l.qty * product.pricePaise, 0);
  const svc = services.reduce((s, l) => s + l.qty * (l.unitPricePaise ?? 0), 0);
  const off = discount.kind === 'PERCENT' ? Math.round((products * discount.value) / 10000) : 0;
  return {
    lines: lines.map((l) => ({
      productId: l.productId,
      productName: product.name,
      unit: product.unit,
      qty: l.qty,
      notSuppliedQty: l.notSuppliedQty,
      unitPricePaise: product.pricePaise,
      gstRateBp: 0,
      grossPaise: l.qty * product.pricePaise,
      discountSharePaise: off,
      netPaise: l.qty * product.pricePaise - off,
      taxPaise: 0,
      batchNos: ['LOT-1'],
      expiresSoon: false,
    })),
    serviceLines: [],
    consultationPaise: svc,
    proceduresPaise: 0,
    productsPaise: products,
    eligibleSubtotalPaise: products,
    subtotalPaise: products + svc,
    discountPaise: off,
    taxPaise: 0,
    roundOffPaise: 0,
    totalPaise: products + svc - off,
    discountRateBp: discount.kind === 'PERCENT' ? discount.value : 0,
    needsApproval: false,
  } as Quote;
}

const context: AppContextValue = {
  status: { needsSetup: false, clinicName: 'SkinDoc', session: null, locked: false, idleLockMinutes: 10, appVersion: 'test' },
  session: { userId: USER_ID, username: 'reception', fullName: 'Reception', role: 'RECEPTIONIST', hasPin: false },
  isAdmin: false,
  setStatus: () => undefined,
  navigate: () => undefined,
  notify: () => undefined,
};

/** Starts the screen from a saved unfinished bill: the quickest way to put lines on it. */
function draft(overrides: Record<string, unknown>) {
  saveBillDraft(USER_ID, {
    billKey: 'draft-key-1',
    client: null,
    clientText: '',
    newPhone: '',
    allowDuplicate: false,
    serviceLines: [],
    lines: [],
    useStandard: true,
    discountKind: 'NONE',
    discountText: '',
    split: false,
    payments: [{ method: 'CASH', amount: '', reference: '' }],
    receivedText: '',
    note: '',
    ...overrides,
  });
}
const consultation = { key: 1, serviceId: null, kind: 'CONSULTATION', name: 'General Consultation', defaultPricePaise: null, qty: 1, price: '500' };

function renderPage() {
  return render(
    <AppContext.Provider value={context}>
      <NewBillPage />
    </AppContext.Provider>,
  );
}
const lastDiscount = () => mocks.quoteBill.mock.lastCall?.[2] as Discount | undefined;

beforeEach(() => {
  window.localStorage.clear();
  mocks.quoteBill.mockImplementation((lines: BillLineInput[], services: ServiceLineInput[], discount: Discount) => Promise.resolve(fakeQuote(lines, services, discount)));
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('New bill screen', () => {
  it('sends the standard 10% medicine discount, and none once it is unticked', async () => {
    draft({ lines: [{ product, qty: 2, notSuppliedQty: 0 }] });
    renderPage();
    await waitFor(() => expect(lastDiscount()).toEqual({ kind: 'PERCENT', value: 1000 }));

    fireEvent.click(await screen.findByLabelText(t.billing.standardDiscount(10)));
    await waitFor(() => expect(lastDiscount()).toEqual({ kind: 'NONE' }));
  });

  it('sends no discount when the bill has no medicines', async () => {
    draft({ serviceLines: [consultation] });
    renderPage();
    await waitFor(() => expect(mocks.quoteBill).toHaveBeenCalled());
    expect(lastDiscount()).toEqual({ kind: 'NONE' });
    expect(mocks.quoteBill.mock.calls.every((c) => (c[2] as Discount).kind === 'NONE')).toBe(true);
  });

  it('warns when a number is typed as the client name, and will not save', async () => {
    draft({ serviceLines: [consultation], clientText: '9876543210' });
    renderPage();
    expect(await screen.findByText(t.billing.nameIsNumber)).toBeTruthy();
    await waitFor(() => expect(mocks.quoteBill).toHaveBeenCalled());
    const save = screen.getByRole('button', { name: t.billing.finalizeOnly }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
  });

  it('lets a new client with a real name be saved', async () => {
    draft({ serviceLines: [consultation], clientText: 'Rahul Sharma' });
    renderPage();
    const save = (await screen.findByRole('button', { name: t.billing.finalizeOnly })) as HTMLButtonElement;
    await waitFor(() => expect(save.disabled).toBe(false));
  });

  it('brings back an unfinished bill with the same bill key, and Discard drops it', async () => {
    draft({ lines: [{ product, qty: 1, notSuppliedQty: 0 }], note: 'Call back tomorrow' });
    renderPage();
    expect(await screen.findByText(product.name)).toBeTruthy();
    expect(screen.getByDisplayValue('Call back tomorrow')).toBeTruthy();
    await waitFor(() => expect(mocks.quoteBill).toHaveBeenCalled());

    fireEvent.click(screen.getByRole('button', { name: t.billing.discardDraft }));
    await waitFor(() => expect(screen.queryByText(product.name)).toBeNull());
    expect(loadBillDraft(USER_ID)).toBeNull();
  });

  it('keeps the bill on this computer as it is typed', async () => {
    vi.useFakeTimers();
    try {
      draft({ serviceLines: [consultation] });
      renderPage();
      fireEvent.change(screen.getByLabelText(t.billing.note), { target: { value: 'Paid by husband' } });
      await act(async () => {
        vi.advanceTimersByTime(600);
      });
      const saved = loadBillDraft<{ note: string; billKey: string }>(USER_ID);
      expect(saved?.state.note).toBe('Paid by husband');
      expect(saved?.state.billKey).toBe('draft-key-1');
    } finally {
      vi.useRealTimers();
    }
  });

  it('does not restore a draft into a correction', async () => {
    draft({ lines: [{ product, qty: 1, notSuppliedQty: 0 }] });
    const correcting = {
      bill: { id: 9, billNo: 'INV/9', clientId: null },
      items: [],
      services: [],
      payments: [],
    } as unknown as Parameters<typeof NewBillPage>[0]['correcting'];
    render(
      <AppContext.Provider value={context}>
        <NewBillPage correcting={correcting} />
      </AppContext.Provider>,
    );
    expect(screen.queryByText(product.name)).toBeNull();
    expect(screen.queryByRole('button', { name: t.billing.discardDraft })).toBeNull();
  });
});
