import type { Paise } from '../api';

/**
 * Formats whole paise as rupees with Indian digit grouping: 12345678 -> "1,23,456.78".
 * Uses integer arithmetic only, matching `Paise::to_indian_string` in clinic-core.
 */
export function formatPaise(paise: Paise): string {
  if (!Number.isSafeInteger(paise)) throw new RangeError(`Money must be whole paise, got ${paise}`);
  const sign = paise < 0 ? '-' : '';
  const abs = Math.abs(paise);
  const rupees = String(Math.trunc(abs / 100));
  const fraction = String(abs % 100).padStart(2, '0');
  const last3 = rupees.slice(-3);
  const head = rupees.slice(0, -3);
  const grouped = head ? `${head.replace(/\B(?=(\d{2})+$)/g, ',')},${last3}` : last3;
  return `${sign}${grouped}.${fraction}`;
}

/** "₹ 1,500.00" */
export function rupees(paise: Paise): string {
  return `₹ ${formatPaise(paise)}`;
}

/**
 * Parses what staff type in a rupee field ("150", "150.5", "1,500.00", "₹ 20") into whole
 * paise, with string arithmetic only (no floating point). Returns null if it is not a valid
 * non-negative amount with at most 2 decimals.
 */
export function parseRupees(text: string): Paise | null {
  const cleaned = text.replace(/[₹,\s]/g, '');
  const match = /^(\d{1,9})(?:\.(\d{0,2}))?$/.exec(cleaned);
  if (!match) return null;
  const whole = Number(match[1]);
  const fraction = Number((match[2] ?? '').padEnd(2, '0'));
  return whole * 100 + fraction;
}

/** Paise as a plain editable value: 15000 -> "150", 15050 -> "150.50". */
export function paiseToInput(paise: Paise): string {
  return paise % 100 === 0 ? String(paise / 100) : `${Math.trunc(paise / 100)}.${String(paise % 100).padStart(2, '0')}`;
}

/** GST basis points as a percentage label: 1200 -> "12%", 250 -> "2.5%". */
export function percentLabel(bp: number): string {
  return `${bp % 100 === 0 ? bp / 100 : (bp / 100).toFixed(2).replace(/0+$/, '')}%`;
}
