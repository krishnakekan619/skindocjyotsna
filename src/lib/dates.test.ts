import { describe, expect, it } from 'vitest';
import { addDaysIso, formatExpiry, formatIsoDate, toIso } from './dates';

describe('dates', () => {
  it('formats stored dates for people', () => {
    expect(formatIsoDate('2026-09-05')).toBe('05-Sep-2026');
    expect(formatExpiry('2026-12-31')).toBe('12/2026');
    expect(formatIsoDate(null)).toBe('—');
  });

  it('adds days across month and year ends', () => {
    expect(addDaysIso('2026-12-31', 1)).toBe('2027-01-01');
    expect(addDaysIso('2028-02-28', 1)).toBe('2028-02-29');
    expect(addDaysIso('2026-03-01', -1)).toBe('2026-02-28');
  });

  it('writes local dates as YYYY-MM-DD', () => {
    expect(toIso(new Date(2026, 8, 5))).toBe('2026-09-05');
  });
});
