import { isTauri } from '@tauri-apps/api/core';
import type { ClinicApi } from './clinicApi';
import { isCommandError, type CommandError } from './types';
import { tauriApi } from './tauriTransport';

export { SESSION_CHANGED_EVENT } from './tauriTransport';

export type { ClinicApi } from './clinicApi';
export * from './types';

export const DESKTOP_SHELL_REQUIRED: CommandError = {
  code: 'DESKTOP_SHELL_REQUIRED',
  message: 'This screen needs the SkinDocJyotsna desktop app. Start it with "npm run dev".',
};

/** Used when the UI is opened in a plain browser (npm run web:dev): every call is refused. */
export const browserPreviewApi: ClinicApi = new Proxy({} as ClinicApi, {
  get: () => () => Promise.reject(DESKTOP_SHELL_REQUIRED),
});

export function createApi(runningInTauri: boolean): ClinicApi {
  return runningInTauri ? tauriApi : browserPreviewApi;
}

export const api: ClinicApi = createApi(isTauri());

/** Staff-safe message for any error thrown by the API. */
export function errorMessage(error: unknown, fallback: string): string {
  return isCommandError(error) ? error.message : fallback;
}

/** The form field an error belongs to, if the backend named one. */
export function errorField(error: unknown): string | undefined {
  return isCommandError(error) ? error.field : undefined;
}
