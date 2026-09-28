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

const row = { line: 2, vendorName: 'Derma Pharma', productName: 'Sunscreen SPF 50', mrpPaise: 65000, purchasePricePaise: 48000, expiryDate: '2027-12-31', qty: 10, newProduct: true, newVendor: true, errors: [], warnings: [] };
const preview = (over: Partial<ImportPreview> = {}): ImportPreview => ({ fileId: 'f1', rows: [row], errorRows: 0, newProducts: 1, newVendors: 1, totalQty: 10, importedBeforeAt: null, ...over });

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
    mocks.importStock.mockResolvedValue({ rows: 1, newProducts: 1, newVendors: 1, totalQty: 10 });
    const onImported = vi.fn();
    render(<ImportStockDialog onImported={onImported} onClose={() => undefined} />);
    await chooseFile();
    const button = (await screen.findByRole('button', { name: t.stockImport.importRows(1) })) as HTMLButtonElement;
    await waitFor(() => expect(button.disabled).toBe(false));
    fireEvent.click(button);
    await waitFor(() => expect(onImported).toHaveBeenCalledWith({ rows: 1, newProducts: 1, newVendors: 1, totalQty: 10 }));
    expect(mocks.importStock).toHaveBeenCalledTimes(1);
    expect(mocks.importStock.mock.lastCall?.[2]).toBe(false);
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
});
