import { invoke } from '@tauri-apps/api/core';
import type { ClinicApi } from './clinicApi';
import type { BackupFile, ReceiptData, RestoreResult, SystemInfo } from './types';

/** ClinicApi implementation that calls the Rust shell over Tauri IPC. */
export const tauriApi: ClinicApi = {
  getSystemInfo: () => invoke<SystemInfo>('get_system_info'),
  listBackups: () => invoke<BackupFile[]>('list_backups'),
  createBackup: () => invoke<BackupFile>('create_backup'),
  restoreBackup: (fileName) => invoke<RestoreResult>('restore_backup', { fileName }),
  getSampleReceipt: () => invoke<ReceiptData>('get_sample_receipt'),
  exportSampleReceiptPdf: () => invoke<string>('export_sample_receipt_pdf'),
};
