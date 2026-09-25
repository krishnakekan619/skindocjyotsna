import type { BackupFile, ReceiptData, RestoreResult, SystemInfo } from './types';

/**
 * Everything the UI can ask the application to do.
 *
 * The UI depends only on this interface, never on Tauri directly, so a future
 * multi-computer version can swap in an HTTP implementation without touching screens.
 */
export interface ClinicApi {
  getSystemInfo(): Promise<SystemInfo>;
  listBackups(): Promise<BackupFile[]>;
  createBackup(): Promise<BackupFile>;
  /** `fileName` must be one returned by `listBackups`. */
  restoreBackup(fileName: string): Promise<RestoreResult>;
  getSampleReceipt(): Promise<ReceiptData>;
  /** Returns the path of the saved PDF. */
  exportSampleReceiptPdf(): Promise<string>;
}
