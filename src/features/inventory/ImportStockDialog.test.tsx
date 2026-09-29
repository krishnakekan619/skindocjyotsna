// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { ImportPreview } from '../../api';
import { t } from '../../i18n/en';
import { ImportStockDialog } from './ImportStockDialog';

const mocks = vi.hoisted(() => ({ previewStockImport: vi.fn(), importStock: vi.fn(), openStockImportTemplate: vi.fn() }));
vi.mock('../../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api')>();
  return { ...actual, api: mocks };
});

const row = { line: 2, lotId: null, exportedQty: null, currentQty: null, action: 'ADD' as const, changes: [], vendorName: 'Derma Pharma', productName: 'Sunscreen SPF 50', mrpPaise: 65000, purchasePricePaise: 48000, expiryDate: '2027-12-31', qty: 10, newProduct: true, newVendor: true, errors: [], warnings: [] };
const preview = (over: Partial<ImportPreview> = {}): ImportPreview => ({ mode: 'ADD', fileId: 'f1', rows: [row], errorRows: 0, added: 1, updated: 0, unchanged: 0, newProducts: 1, newVendors: 1, totalQty: 10, importedBeforeAt: null, ...over });

async function chooseFile() {
  const input = screen.getByTestId('stock-file');
  fireEvent.change(input, { target: { files: [new File(['Vendor Name,Product Name\n'], 'stock.csv', { type: 'text/csv' })] } });
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('Import stock from CSV', () => {
  it('imports only when every row is correct', async () => {
    mocks.previewStockImport.mockResolvedValue(preview({ rows: [{ ...row, errors: ['MRP is missing.'] }], errorRows: 1 }));
    render(<ImportStockDialog onImported={() => undefined} onClose={() => undefined} />);
    await chooseFile();
    expect(await screen.findByText('MRP is missing.')).toBeTruthy();
    expect((screen.getByRole('button', { name: t.stockImport.importRows(1) }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('sends the file once and reports what was imported', async () => {
    mocks.previewStockImport.mockResolvedValue(preview());
    mocks.importStock.mockResolvedValue({ added: 1, updated: 0, unchanged: 0, newProducts: 1, newVendors: 1, totalQty: 10 });
    const onImported = vi.fn();
    render(<ImportStockDialog onImported={onImported} onClose={() => undefined} />);
    await chooseFile();
    const button = (await screen.findByRole('button', { name: t.stockImport.importRows(1) })) as HTMLButtonElement;
    await waitFor(() => expect(button.disabled).toBe(false));
    fireEvent.click(button);
    await waitFor(() => expect(onImported).toHaveBeenCalledWith({ added: 1, updated: 0, unchanged: 0, newProducts: 1, newVendors: 1, totalQty: 10 }));
    expect(mocks.importStock).toHaveBeenCalledTimes(1);
    expect(mocks.importStock.mock.lastCall?.[1]).toBe('ADD');
    expect(mocks.importStock.mock.lastCall?.[3]).toBe(false);
  });

  it('asks before importing the same file a second time', async () => {
    mocks.previewStockImport.mockResolvedValue(preview({ importedBeforeAt: 1_790_310_600 }));
    render(<ImportStockDialog onImported={() => undefined} onClose={() => undefined} />);
    await chooseFile();
    const button = (await screen.findByRole('button', { name: t.stockImport.importRows(1) })) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    fireEvent.click(screen.getByLabelText(t.stockImport.importAgain));
    await waitFor(() => expect(button.disabled).toBe(false));
  });

  it('checks the same file again as an edited export and lists what changes', async () => {
    mocks.previewStockImport.mockImplementation((_text: string, mode: string) =>
      Promise.resolve(
        mode === 'UPDATE'
          ? preview({ mode: 'UPDATE', added: 0, updated: 1, rows: [{ ...row, lotId: 7, action: 'UPDATE', changes: ['Quantity 10 → 7'], newProduct: false, newVendor: false }] })
          : preview({ errorRows: 1, rows: [{ ...row, lotId: 7, action: 'UNCHANGED', errors: ['Row from an export'] }] }),
      ),
    );
    render(<ImportStockDialog onImported={() => undefined} onClose={() => undefined} />);
    await chooseFile();
    expect(await screen.findByText('Row from an export')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: t.stockImport.modes.UPDATE }));
    expect(await screen.findByText('Quantity 10 → 7')).toBeTruthy();
    expect(mocks.previewStockImport.mock.lastCall?.[1]).toBe('UPDATE');
    await waitFor(() => expect((screen.getByRole('button', { name: t.stockImport.importRows(1) }) as HTMLButtonElement).disabled).toBe(false));
  });
});
