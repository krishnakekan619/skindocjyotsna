import type { IsoDate, Timestamp } from '../api';

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

function pad(n: number): string {
  return String(n).padStart(2, '0');
}

/** "25-Sep-2026 10:42" in this computer's time zone. */
export function formatDateTime(ts: Timestamp | null | undefined): string {
  if (ts === null || ts === undefined) return '—';
  const d = new Date(ts * 1000);
  return `${pad(d.getDate())}-${MONTHS[d.getMonth()]}-${d.getFullYear()} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** "2026-12-31" -> "31-Dec-2026" */
export function formatIsoDate(date: IsoDate | null | undefined): string {
  if (!date) return '—';
  const [y, m, d] = date.split('-');
  const month = MONTHS[Number(m) - 1];
  return month && y && d ? `${d}-${month}-${y}` : date;
}

/** "2026-12-31" -> "12/2026", as printed on medicine packs. */
export function formatExpiry(date: IsoDate | null | undefined): string {
  if (!date) return '—';
  const [y, m] = date.split('-');
  return `${m}/${y}`;
}

/** Today's local date as YYYY-MM-DD. */
export function todayIso(): IsoDate {
  return toIso(new Date());
}

export function toIso(d: Date): IsoDate {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function addDaysIso(date: IsoDate, days: number): IsoDate {
  const [y, m, d] = date.split('-').map(Number);
  return toIso(new Date(y ?? 1970, (m ?? 1) - 1, (d ?? 1) + days));
}

/** First day of this month, YYYY-MM-DD. */
export function monthStartIso(): IsoDate {
  const d = new Date();
  return toIso(new Date(d.getFullYear(), d.getMonth(), 1));
}

/** Age in whole years on today's date. */
export function ageYears(dateOfBirth: IsoDate | null): number | null {
  if (!dateOfBirth) return null;
  const [y, m, d] = dateOfBirth.split('-').map(Number);
  if (!y || !m || !d) return null;
  const now = new Date();
  let age = now.getFullYear() - y;
  if (now.getMonth() + 1 < m || (now.getMonth() + 1 === m && now.getDate() < d)) age -= 1;
  return age;
}

/** A random id for a new bill's idempotency key. */
export function newBillKey(): string {
  return crypto.randomUUID();
}
