import { describe, expect, it } from 'vitest';
import { browserPreviewApi, createApi, DESKTOP_SHELL_REQUIRED, isCommandError } from './index';
import { tauriApi } from './tauriTransport';

describe('createApi', () => {
  it('uses the Tauri transport inside the desktop shell', () => {
    expect(createApi(true)).toBe(tauriApi);
  });

  it('falls back to the browser preview outside the desktop shell', () => {
    expect(createApi(false)).toBe(browserPreviewApi);
  });

  it('browser preview rejects with a user-friendly command error', async () => {
    await expect(browserPreviewApi.getSystemInfo()).rejects.toEqual(DESKTOP_SHELL_REQUIRED);
  });
});

describe('isCommandError', () => {
  it('recognises the command error shape', () => {
    expect(isCommandError({ code: 'X', message: 'y' })).toBe(true);
  });

  it.each([null, undefined, 'text', 42, { code: 1, message: 'y' }, { code: 'X' }])('rejects %p', (value) => {
    expect(isCommandError(value)).toBe(false);
  });
});
