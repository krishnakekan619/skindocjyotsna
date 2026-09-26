import { Fragment } from 'react';
import type { ReceiptData } from '../../api';
import { formatPaise } from '../../lib/money';
import './receipt.css';

/**
 * On-screen and printable receipt. Shows the same data as the Rust-generated PDF, so the
 * printed copy matches the PDF. Print styles (A5) are in receipt.css.
 */
export function ReceiptPreview({ data }: { data: ReceiptData }) {
  const contact = [data.clinicPhone && `Ph ${data.clinicPhone}`, data.clinicGstin && `GSTIN ${data.clinicGstin}`].filter(Boolean);
  type TotalRow = { label: string; value: string; strong?: boolean };
  const totals: TotalRow[] =
    data.breakdown.length > 0 ? data.breakdown.map((row) => ({ label: row.label, value: formatPaise(row.amount) })) : [{ label: 'Subtotal', value: formatPaise(data.subtotal) }];
  if (data.discount) totals.push({ label: data.discountLabel, value: `-${formatPaise(data.discount)}` });
  if (data.tax) totals.push({ label: data.taxLabel, value: formatPaise(data.tax) });
  if (data.roundOff) totals.push({ label: 'Round off', value: formatPaise(data.roundOff) });
  totals.push({ label: 'TOTAL', value: `₹ ${formatPaise(data.total)}`, strong: true });

  return (
    <article className="receipt print-area" aria-label={`Receipt ${data.billNo}`}>
      <header className="receipt__header">
        <h2>{data.clinicName}</h2>
        {data.clinicAddressLines.map((line) => (
          <p key={line}>{line}</p>
        ))}
        {contact.length > 0 && <p>{contact.join('  ·  ')}</p>}
        {data.statusBanner && <p className="receipt__banner">{data.statusBanner}</p>}
      </header>
      <div className="receipt__meta">
        <strong>Bill No: {data.billNo}</strong>
        <span>{data.dateTime}</span>
      </div>
      {data.clientLabel && <div className="receipt__client">Client: {data.clientLabel}</div>}
      <table className="receipt__items">
        <thead>
          <tr>
            <th>Item</th>
            <th className="num">Qty</th>
            <th className="num">Rate</th>
            <th className="num">Amount</th>
          </tr>
        </thead>
        <tbody>
          {data.lines.map((line, index) => {
            const notSupplied = line.notSuppliedQty > 0;
            const detail = notSupplied ? `Not supplied (out of stock) – prescribed ${line.notSuppliedQty}` : line.detail;
            const heading = line.section && line.section !== data.lines[index - 1]?.section ? line.section : null;
            return (
              <Fragment key={`${line.name}-${index}`}>
                {heading && (
                  <tr className="receipt__section">
                    <td colSpan={4}>{heading}</td>
                  </tr>
                )}
              <tr className={notSupplied ? 'is-not-supplied' : undefined}>
                <td>
                  {line.name}
                  {detail && <div className="receipt__detail">{detail}</div>}
                </td>
                <td className="num">{line.qty}</td>
                <td className="num">{notSupplied ? '–' : formatPaise(line.unitPrice)}</td>
                <td className="num">{formatPaise(line.amount)}</td>
              </tr>
              </Fragment>
            );
          })}
        </tbody>
      </table>
      <table className="receipt__totals">
        <tbody>
          {totals.map(({ label, value, strong }) => (
            <tr key={label} className={strong ? 'is-total' : undefined}>
              <td>{label}</td>
              <td className="num">{value}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <footer className="receipt__footer">
        {data.payments.length > 0 && (
          <p>Paid: {data.payments.map((p) => `${p.method} ${formatPaise(p.amount)}`).join(' + ')}</p>
        )}
        {data.amountReceived !== null && data.changeDue !== null && (
          <p>
            Received {formatPaise(data.amountReceived)} · Change {formatPaise(data.changeDue)}
          </p>
        )}
        {data.billedBy && <p className="receipt__muted">Billed by: {data.billedBy}</p>}
        {data.footer && <p className="receipt__thanks">{data.footer}</p>}
        {data.notice && <p className="receipt__notice">{data.notice}</p>}
      </footer>
    </article>
  );
}
