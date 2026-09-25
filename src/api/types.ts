// Types shared with the Rust shell. Keep in sync with src-tauri/src/commands/*.rs
// (serde uses camelCase). Generated bindings may replace this file in Phase 1.

export interface DatabaseStatus {
  sqliteVersion: string;
  schemaVersion: number;
  journalMode: string;
  integrityOk: boolean;
  fts5Available: boolean;
}

export interface SystemInfo {
  appVersion: string;
  platform: string;
  arch: string;
  dataDir: string;
  databaseFile: string;
  backupDir: string;
  logDir: string;
  database: DatabaseStatus;
}

export type BackupKind = 'manual' | 'auto' | 'pre-restore';

export interface BackupFile {
  fileName: string;
  sizeBytes: number;
  kind: BackupKind | null;
  createdUtc: string | null;
  appVersion: string | null;
  /** Set when the file cannot be used; safe to show to staff. */
  problem: string | null;
}

export interface RestoreResult {
  restoredFrom: BackupFile;
  safetyBackup: BackupFile;
}

/** Money is always whole paise (integers) - never floating-point rupees. */
export type Paise = number;

export interface ReceiptLine {
  name: string;
  detail: string | null;
  qty: number;
  unitPrice: Paise;
  amount: Paise;
  notSuppliedQty: number;
}

export interface ReceiptPayment {
  method: string;
  amount: Paise;
}

export interface ReceiptData {
  clinicName: string;
  clinicAddressLines: string[];
  clinicPhone: string | null;
  clinicGstin: string | null;
  statusBanner: string | null;
  billNo: string;
  dateTime: string;
  clientLabel: string | null;
  lines: ReceiptLine[];
  subtotal: Paise;
  discount: Paise;
  taxLabel: string;
  tax: Paise;
  roundOff: Paise;
  total: Paise;
  payments: ReceiptPayment[];
  amountReceived: Paise | null;
  changeDue: Paise | null;
  billedBy: string | null;
  footer: string | null;
}

/** Error shape returned by every Rust command. `message` is safe to show to staff. */
export interface CommandError {
  code: string;
  message: string;
}

export function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as Record<string, unknown>)['code'] === 'string' &&
    typeof (value as Record<string, unknown>)['message'] === 'string'
  );
}
