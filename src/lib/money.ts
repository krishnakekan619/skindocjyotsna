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
