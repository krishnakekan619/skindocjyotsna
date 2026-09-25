import { isTauri } from '@tauri-apps/api/core';
import type { ClinicApi } from './clinicApi';
import type { CommandError } from './types';
import { tauriApi } from './tauriTransport';

export type { ClinicApi } from './clinicApi';
export * from './types';

export const DESKTOP_SHELL_REQUIRED: CommandError = {
  code: 'DESKTOP_SHELL_REQUIRED',
  message: 'This screen needs the SkinDocJyotsna desktop app. Start it with "npm run dev".',
};

const unavailable = () => Promise.reject(DESKTOP_SHELL_REQUIRED);

/** Used when the UI is opened in a plain browser (npm run web:dev) for layout work. */
export const browserPreviewApi: ClinicApi = {
  getSystemInfo: unavailable,
  listBackups: unavailable,
  createBackup: unavailable,
  restoreBackup: unavailable,
  getSampleReceipt: unavailable,
  exportSampleReceiptPdf: unavailable,
};

export function createApi(runningInTauri: boolean): ClinicApi {
  return runningInTauri ? tauriApi : browserPreviewApi;
}

export const api: ClinicApi = createApi(isTauri());

/** Staff-safe message for any error thrown by the API. */
export function errorMessage(error: unknown, fallback: string): string {
  const e = error as Partial<CommandError> | null;
  return e && typeof e.code === 'string' && typeof e.message === 'string' ? e.message : fallback;
}
