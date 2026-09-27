/**
 * The unfinished bill, kept on this computer so a crash, restart or idle lock does not lose it
 * (DEC-038). One draft per user; cleared when the bill is finalized or cleared and on sign-out.
 * Browser storage can be missing or blocked: every access is guarded and failures are ignored.
 */

const PREFIX = 'skindoc.billDraft.';
const VERSION = 1;

export interface BillDraft<T> {
  version: number;
  /** Unix seconds, like every timestamp in the app. */
  savedAt: number;
  state: T;
}

const keyOf = (userId: number) => `${PREFIX}${userId}`;

export function loadBillDraft<T>(userId: number): BillDraft<T> | null {
  try {
    const raw = window.localStorage.getItem(keyOf(userId));
    if (!raw) return null;
    const draft = JSON.parse(raw) as BillDraft<T>;
    return draft && draft.version === VERSION && draft.state ? draft : null;
  } catch {
    return null;
  }
}

export function saveBillDraft<T>(userId: number, state: T): void {
  try {
    const draft: BillDraft<T> = { version: VERSION, savedAt: Math.floor(Date.now() / 1000), state };
    window.localStorage.setItem(keyOf(userId), JSON.stringify(draft));
  } catch {
    // Storage full or blocked: the bill on screen is unaffected.
  }
}

export function clearBillDraft(userId: number): void {
  try {
    window.localStorage.removeItem(keyOf(userId));
  } catch {
    // Nothing to do.
  }
}
