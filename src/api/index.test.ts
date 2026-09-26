import { describe, expect, it } from 'vitest';
import { browserPreviewApi, createApi, DESKTOP_SHELL_REQUIRED, errorField, errorMessage, isCommandError } from './index';
import { tauriApi } from './tauriTransport';

describe('createApi', () => {
  it('uses the Tauri transport inside the desktop shell', () => {
    expect(createApi(true)).toBe(tauriApi);
  });

  it('falls back to the browser preview outside the desktop shell', () => {
    expect(createApi(false)).toBe(browserPreviewApi);
  });

  it('browser preview rejects every call with a user-friendly command error', async () => {
    await expect(browserPreviewApi.getAppStatus()).rejects.toEqual(DESKTOP_SHELL_REQUIRED);
    await expect(browserPreviewApi.finalizeBill({} as never)).rejects.toEqual(DESKTOP_SHELL_REQUIRED);
  });
});

describe('errors', () => {
  it('recognises the command error shape', () => {
    expect(isCommandError({ code: 'X', message: 'y' })).toBe(true);
  });

  it.each([null, undefined, 'text', 42, { code: 1, message: 'y' }, { code: 'X' }])('rejects %p', (value) => {
    expect(isCommandError(value)).toBe(false);
  });

  it('uses the staff message or a fallback, and exposes the field', () => {
    const error = { code: 'VALIDATION', message: 'Name is required.', field: 'name' };
    expect(errorMessage(error, 'fallback')).toBe('Name is required.');
    expect(errorMessage(new Error('boom'), 'fallback')).toBe('fallback');
    expect(errorField(error)).toBe('name');
  });
});
