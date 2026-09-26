import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { ReceiptData } from '../../api';
import { ReceiptPreview } from './ReceiptPreview';

// Mirrors clinic_pdf::sample_receipt() so the preview and the PDF show the same content.
const sample: ReceiptData = {
  clinicName: 'SkinDocJyotsna Clinic',
  clinicAddressLines: ['12 MG Road, Pune 411001'],
  clinicPhone: '020-12345678',
  clinicGstin: null,
  statusBanner: null,
  billNo: 'INV/26-27/000123',
  dateTime: '25-Sep-2026 10:42',
  clientLabel: 'John Doe (CL-000045)',
  lines: [
    { name: 'Paracetamol 500mg', detail: 'Batch A23 · Exp 12/2026', qty: 2, unitPrice: 2000, amount: 4000, notSuppliedQty: 0, section: '' },
    { name: 'Sunscreen SPF 50', detail: null, qty: 0, unitPrice: 0, amount: 0, notSuppliedQty: 1, section: '' },
  ],
  subtotal: 16000,
  breakdown: [],
  discountLabel: 'Discount',
  discount: 1000,
  taxLabel: 'GST included',
  tax: 2118,
  roundOff: 0,
  total: 15000,
  payments: [{ method: 'Cash', amount: 15000 }],
  amountReceived: 20000,
  changeDue: 5000,
  billedBy: 'Priya',
  footer: 'Thank you',
  notice: 'This is a computer-generated e-receipt and does not require a signature or stamp.',
};

describe('ReceiptPreview', () => {
  const html = renderToStaticMarkup(<ReceiptPreview data={sample} />);

  it('shows the bill number, totals in rupees and payment', () => {
    expect(html).toContain('INV/26-27/000123');
    expect(html).toContain('₹ 150.00');
    expect(html).toContain('-10.00');
    expect(html).toContain('Received 200.00');
  });

  it('marks out-of-stock items as not supplied with zero value (DEC-002)', () => {
    expect(html).toContain('Not supplied (out of stock) – prescribed 1');
    expect(html).toContain('is-not-supplied');
  });

  it('omits zero round-off and a missing status banner', () => {
    expect(html).not.toContain('Round off');
    expect(html).not.toContain('receipt__banner');
  });

  it('shows a cancellation banner when present', () => {
    const cancelled = renderToStaticMarkup(<ReceiptPreview data={{ ...sample, statusBanner: 'CANCELLED' }} />);
    expect(cancelled).toContain('CANCELLED');
  });
});

describe('ReceiptPreview with consultation and procedures', () => {
  const consultation = { name: 'General Consultation', detail: null, qty: 1, unitPrice: 50000, amount: 50000, notSuppliedQty: 0, section: 'Consultation' };
  const medicine = { ...sample.lines[0]!, section: 'Medicines & Products' };
  const html = renderToStaticMarkup(
    <ReceiptPreview
      data={{
        ...sample,
        lines: [consultation, medicine],
        breakdown: [
          { label: 'Consultation', amount: 50000 },
          { label: 'Medicines & products', amount: 4000 },
        ],
        discountLabel: 'Discount on medicines',
      }}
    />,
  );

  it('groups lines under section headings and shows section subtotals', () => {
    expect(html).toContain('receipt__section');
    expect(html).toContain('Medicines &amp; Products');
    expect(html).toContain('Discount on medicines');
    expect(html).not.toContain('Subtotal');
  });

  it('says that an e-receipt needs no signature or stamp', () => {
    expect(html).toContain('does not require a signature or stamp');
  });
});
